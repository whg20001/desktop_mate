use super::{
    model::{AudioError, AudioStart, AudioTimings, Cancellation},
    playback::{decode_wav, PcmAudio},
    provider::TtsProvider,
};
use parking_lot::Mutex;
use std::{
    collections::VecDeque,
    num::{NonZeroU16, NonZeroU32},
    sync::{
        atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering},
        mpsc, Arc,
    },
    thread,
    time::{Duration, Instant},
};

// Rate is wall-clock seconds required to produce one second of audio.
// Observe producer timestamps rather than consumption time (which includes playback).
#[derive(Default)]
struct BufferPolicy {
    last_ready: Option<Instant>,
    rate: Option<f64>,
}
impl BufferPolicy {
    fn observe(&mut self, ready: Instant, audio_seconds: f64) {
        if let Some(previous) = self.last_ready {
            let rate = ready.saturating_duration_since(previous).as_secs_f64() / audio_seconds;
            if rate.is_finite() && audio_seconds > 0.0 {
                // React quickly to slowdown, release the reserve more cautiously.
                self.rate = Some(match self.rate {
                    Some(old) if rate < old => old * 0.75 + rate * 0.25,
                    Some(old) => old * 0.25 + rate * 0.75,
                    None => rate,
                });
            }
        }
        self.last_ready = Some(ready);
    }
    fn target_seconds(&self) -> f64 {
        // Reserve against a four-second production horizon, capped for long text.
        (0.8 + 4.0 * (self.rate.unwrap_or(1.0) - 0.8).max(0.0)).clamp(0.8, 6.0)
    }
    fn ready(&self, buffered: f64, eof: bool) -> bool {
        eof || (self.rate.is_some() && buffered >= self.target_seconds())
    }
}
struct Chunk {
    audio: PcmAudio,
    ready_at: Instant,
}
#[derive(Default)]
struct Progress {
    samples: AtomicU64,
    silence: AtomicU64,
    level: AtomicU32,
    rebuffers: AtomicU64,
    target_ms: AtomicU64,
    rate_milli: AtomicU64,
}
struct Source {
    receiver: mpsc::Receiver<Chunk>,
    pending: VecDeque<std::vec::IntoIter<f32>>,
    buffered: usize,
    eof: bool,
    rebuffering: bool,
    policy: BufferPolicy,
    channels: u16,
    rate: u32,
    progress: Arc<Progress>,
    power: f64,
    window: usize,
}
impl Source {
    fn publish_policy(&self) {
        self.progress.target_ms.store(
            (self.policy.target_seconds() * 1000.0) as u64,
            Ordering::Relaxed,
        );
        self.progress.rate_milli.store(
            (self.policy.rate.unwrap_or(0.0) * 1000.0) as u64,
            Ordering::Relaxed,
        );
    }
    fn receive_chunk(&mut self) -> bool {
        if self.eof {
            return false;
        }
        match self.receiver.try_recv() {
            Ok(chunk) => {
                self.policy
                    .observe(chunk.ready_at, chunk.audio.duration_seconds());
                self.buffered += chunk.audio.samples.len();
                self.pending.push_back(chunk.audio.samples.into_iter());
                self.publish_policy();
                true
            }
            Err(mpsc::TryRecvError::Disconnected) => {
                self.eof = true;
                false
            }
            Err(mpsc::TryRecvError::Empty) => false,
        }
    }
    fn seconds(&self) -> f64 {
        self.buffered as f64 / self.rate as f64 / self.channels as f64
    }
}
impl Iterator for Source {
    type Item = f32;
    fn next(&mut self) -> Option<f32> {
        if self.buffered == 0 && !self.rebuffering {
            self.receive_chunk();
            if self.buffered == 0 && !self.eof {
                self.rebuffering = true;
                self.progress.rebuffers.fetch_add(1, Ordering::Relaxed);
            }
        }
        if self.rebuffering {
            while !self.policy.ready(self.seconds(), self.eof) && self.receive_chunk() {}
            if self.policy.ready(self.seconds(), self.eof) {
                self.rebuffering = false;
            } else {
                self.progress.silence.fetch_add(1, Ordering::Relaxed);
                self.progress
                    .level
                    .store(0_f32.to_bits(), Ordering::Relaxed);
                self.power = 0.0;
                self.window = 0;
                return Some(0.0);
            }
        }
        if self.buffered == 0 && self.eof {
            return None;
        }
        let value = loop {
            match self.pending.front_mut()?.next() {
                Some(value) => break value,
                None => {
                    self.pending.pop_front();
                }
            }
        };
        self.buffered -= 1;
        self.progress.samples.fetch_add(1, Ordering::Relaxed);
        self.power += (value as f64).powi(2);
        self.window += 1;
        if self.window >= self.rate as usize * self.channels as usize / 50 {
            let rms = (self.power / self.window as f64).sqrt() as f32;
            self.progress.level.store(
                (if rms < 0.008 {
                    0.0
                } else {
                    (rms * 4.0).min(1.0)
                })
                .to_bits(),
                Ordering::Relaxed,
            );
            self.power = 0.0;
            self.window = 0;
        }
        Some(value)
    }
}
impl rodio::Source for Source {
    fn current_span_len(&self) -> Option<usize> {
        None
    }
    fn channels(&self) -> NonZeroU16 {
        NonZeroU16::new(self.channels).unwrap()
    }
    fn sample_rate(&self) -> NonZeroU32 {
        NonZeroU32::new(self.rate).unwrap()
    }
    fn total_duration(&self) -> Option<Duration> {
        None
    }
}

pub fn stream(
    provider: &dyn TtsProvider,
    request: &AudioStart,
    cancel: &Cancellation,
    emit: &mut dyn FnMut(&str, u64, f32),
    timings: &Mutex<AudioTimings>,
) -> Result<(), AudioError> {
    let start = Instant::now();
    let queue_ms = timings.lock().queue_ms;
    let elapsed = || queue_ms + start.elapsed().as_secs_f64() * 1000.0;
    let deadline = start + Duration::from_secs(request.timeout_seconds);
    let (send, receive) = mpsc::sync_channel::<Chunk>(3);
    let abort = Cancellation::default();
    thread::scope(|scope| {
        let producer_abort = abort.clone();
        let producer = scope.spawn(move || {
            let mut format = None;
            let mut duration = 0.0;
            let result =
                provider.stream(&request.speech, &producer_abort, deadline, &mut |chunk| {
                    cancel.check()?;
                    let decode = Instant::now();
                    let audio = decode_wav(chunk.wav, cancel)?;
                    let decode_ms = decode.elapsed().as_secs_f64() * 1000.0;
                    let current = (audio.channels, audio.sample_rate);
                    if format.is_some_and(|f| f != current) {
                        return Err(AudioError::new("unsupported_audio", "音频块格式发生变化"));
                    }
                    format = Some(current);
                    duration += audio.duration_seconds();
                    if duration > 120.0 {
                        return Err(AudioError::new(
                            "audio_limit_exceeded",
                            "累计音频超过 120 秒",
                        ));
                    }
                    {
                        let mut t = timings.lock();
                        t.first_chunk_received_ms.get_or_insert_with(elapsed);
                        t.wav_decode_ms += decode_ms;
                        t.provider = chunk.timings;
                        t.chunks += 1;
                    }
                    let mut pending = Chunk {
                        audio,
                        ready_at: Instant::now(),
                    };
                    loop {
                        cancel.check()?;
                        if Instant::now() >= deadline {
                            return Err(AudioError::new("synthesis_timeout", "音频流超时"));
                        }
                        match send.try_send(pending) {
                            Ok(()) => return Ok(()),
                            Err(mpsc::TrySendError::Disconnected(_)) => {
                                return Err(AudioError::cancelled())
                            }
                            Err(mpsc::TrySendError::Full(audio)) => {
                                pending = audio;
                                thread::sleep(Duration::from_millis(5));
                            }
                        }
                    }
                });
            if let Ok(provider) = &result {
                let mut t = timings.lock();
                t.provider = provider.clone();
                t.synthesis_done_ms = Some(elapsed());
            }
            result
        });
        let result = consume(
            receive,
            cancel,
            deadline,
            request.speech.volume.unwrap_or(1.0),
            emit,
            timings,
            &elapsed,
        );
        if result.is_err() {
            abort.cancel();
        }
        let generated = producer
            .join()
            .map_err(|_| AudioError::new("synthesis_failed", "音频生成线程异常"))?;
        if let Err(error) = &result {
            if error.code != "synthesis_failed" {
                return result;
            }
        }
        generated?;
        result
    })
}
fn consume(
    receiver: mpsc::Receiver<Chunk>,
    cancel: &Cancellation,
    deadline: Instant,
    volume: f32,
    emit: &mut dyn FnMut(&str, u64, f32),
    timings: &Mutex<AudioTimings>,
    elapsed: &dyn Fn() -> f64,
) -> Result<(), AudioError> {
    let first = loop {
        cancel.check()?;
        if Instant::now() >= deadline {
            return Err(AudioError::new("synthesis_timeout", "首块音频等待超时"));
        }
        match receiver.recv_timeout(Duration::from_millis(10)) {
            Ok(chunk) => break chunk,
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                return Err(AudioError::new("synthesis_failed", "未收到音频块"))
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
        }
    };
    let buffering_start = Instant::now();
    let mut policy = BufferPolicy::default();
    policy.observe(first.ready_at, first.audio.duration_seconds());
    let progress = Arc::new(Progress::default());
    let rate = first.audio.sample_rate as f64 * first.audio.channels as f64;
    let mut source = Source {
        receiver,
        buffered: first.audio.samples.len(),
        pending: VecDeque::from([first.audio.samples.into_iter()]),
        eof: false,
        rebuffering: false,
        policy,
        channels: first.audio.channels,
        rate: first.audio.sample_rate,
        progress: progress.clone(),
        power: 0.0,
        window: 0,
    };
    // Prebuffer before opening the device: startup waiting is not an audio underrun.
    while !source.policy.ready(source.seconds(), source.eof) {
        cancel.check()?;
        if Instant::now() >= deadline {
            return Err(AudioError::new("synthesis_timeout", "启动缓冲等待超时"));
        }
        if !source.receive_chunk() {
            thread::sleep(Duration::from_millis(5));
        }
        let mut t = timings.lock();
        t.startup_buffer_ms = buffering_start.elapsed().as_secs_f64() * 1000.0;
        t.startup_audio_ms = source.seconds() * 1000.0;
        t.buffer_target_ms = source.policy.target_seconds() * 1000.0;
        t.generation_rtf = source.policy.rate;
    }
    source.publish_policy();
    // Keep MTA alive through WASAPI playback so the next WinRT synthesis
    // on this worker does not inherit CPAL's thread-local STA apartment.
    let _apartment = super::windows_tts::Apartment::initialize()?;
    let failed = Arc::new(AtomicBool::new(false));
    let callback = failed.clone();
    let mut device = rodio::DeviceSinkBuilder::from_default_device()
        .map_err(|_| AudioError::new("output_unavailable", "没有可用的音频输出设备"))?
        .with_error_callback(move |_| {
            callback.store(true, Ordering::Release);
        })
        .open_stream()
        .map_err(|_| AudioError::new("output_unavailable", "无法打开音频输出设备"))?;
    device.log_on_drop(false);
    let player = rodio::Player::connect_new(device.mixer());
    player.set_volume(volume);
    player.append(source);
    let mut started = false;
    let mut frame = Instant::now();
    let mut level = 0.0;
    let result = (|| loop {
        cancel.check()?;
        if failed.load(Ordering::Acquire) {
            return Err(AudioError::new("playback_failed", "音频输出设备已断开"));
        }
        if Instant::now() > deadline + Duration::from_secs(125) {
            return Err(AudioError::new("playback_failed", "音频流播放超时"));
        }
        let position = (progress.samples.load(Ordering::Relaxed) as f64 * 1000.0 / rate) as u64;
        if !started && player.get_pos() > Duration::ZERO {
            started = true;
            timings.lock().first_playback_ms = Some(elapsed());
            emit("started", position, 0.0);
        }
        if frame.elapsed() >= Duration::from_millis(40) {
            let target = f32::from_bits(progress.level.load(Ordering::Relaxed)) * volume;
            level += (target - level) * if target > level { 0.75 } else { 0.4 };
            emit("frame", position, level);
            frame = Instant::now();
            let mut t = timings.lock();
            t.underrun_ms = progress.silence.load(Ordering::Relaxed) as f64 * 1000.0 / rate;
            t.rebuffer_count = progress.rebuffers.load(Ordering::Relaxed);
            t.buffer_target_ms = progress.target_ms.load(Ordering::Relaxed) as f64;
            let measured_rate = progress.rate_milli.load(Ordering::Relaxed);
            t.generation_rtf = (measured_rate > 0).then_some(measured_rate as f64 / 1000.0);
        }
        if player.empty() {
            return Ok(());
        }
        thread::sleep(Duration::from_millis(10));
    })();
    player.stop();
    drop(player);
    drop(device);
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn policy_adapts_to_rate_and_caps_reserve() {
        let t = Instant::now();
        let mut p = BufferPolicy::default();
        p.observe(t, 0.48);
        assert!(!p.ready(6.0, false)); // One block cannot establish generation speed.
        assert!(p.ready(0.48, true)); // A complete single block can play immediately.
        p.observe(t + Duration::from_millis(400), 0.96);
        assert!(p.ready(1.44, false));
        p.observe(t + Duration::from_millis(2800), 0.96);
        assert!(p.target_seconds() > 4.0);
        assert!(!p.ready(1.44, false));
        assert!(p.ready(1.44, true));
        p.observe(t + Duration::from_secs(20), 0.96);
        assert_eq!(p.target_seconds(), 6.0);
    }
    fn chunk(values: Vec<f32>, ready_at: Instant) -> Chunk {
        Chunk {
            audio: PcmAudio {
                samples: values,
                channels: 1,
                sample_rate: 1000,
            },
            ready_at,
        }
    }
    #[test]
    fn underrun_waits_for_reserve_or_eof_without_losing_samples() {
        let (send, receiver) = mpsc::sync_channel(3);
        let progress = Arc::new(Progress::default());
        let mut source = Source {
            receiver,
            pending: VecDeque::from([vec![0.1, 0.2].into_iter()]),
            buffered: 2,
            eof: false,
            rebuffering: false,
            policy: BufferPolicy::default(),
            channels: 1,
            rate: 1000,
            progress: progress.clone(),
            power: 0.0,
            window: 0,
        };
        assert_eq!(source.next(), Some(0.1));
        assert_eq!(source.next(), Some(0.2));
        assert_eq!(source.next(), Some(0.0));
        send.send(chunk(vec![0.3, 0.4], Instant::now())).unwrap();
        assert_eq!(source.next(), Some(0.0)); // One tiny block must not restart playback.
        assert_eq!(progress.rebuffers.load(Ordering::Relaxed), 1);
        drop(send);
        assert_eq!(source.next(), Some(0.3));
        assert_eq!(source.next(), Some(0.4));
        assert_eq!(source.next(), None);
        assert_eq!(progress.samples.load(Ordering::Relaxed), 4);
    }
    #[test]
    fn live_rms_preserves_stereo_energy_and_gates_silence() {
        let (send, receiver) = mpsc::sync_channel(3);
        let progress = Arc::new(Progress::default());
        let samples: Vec<_> = [0.2, -0.2]
            .repeat(20)
            .into_iter()
            .chain([0.001; 40])
            .collect();
        let mut source = Source {
            receiver,
            buffered: samples.len(),
            pending: VecDeque::from([samples.into_iter()]),
            eof: false,
            rebuffering: false,
            policy: BufferPolicy::default(),
            channels: 2,
            rate: 1000,
            progress: progress.clone(),
            power: 0.0,
            window: 0,
        };
        for _ in 0..40 {
            assert!(source.next().is_some());
        }
        assert!((f32::from_bits(progress.level.load(Ordering::Relaxed)) - 0.8).abs() < 0.0001);
        for _ in 0..40 {
            assert_eq!(source.next(), Some(0.001));
        }
        assert_eq!(f32::from_bits(progress.level.load(Ordering::Relaxed)), 0.0);
        assert_eq!(source.next(), Some(0.0)); // Waiting for data is silent.
        assert_eq!(progress.samples.load(Ordering::Relaxed), 80);
        drop(send);
        assert_eq!(source.next(), None);
    }
    #[test]
    fn buffered_fast_stream_starts_before_eof() {
        let (send, receiver) = mpsc::sync_channel(3);
        let t = Instant::now();
        send.send(chunk(vec![0.2; 960], t + Duration::from_millis(300)))
            .unwrap();
        let mut policy = BufferPolicy::default();
        policy.observe(t, 0.48);
        let mut source = Source {
            receiver,
            pending: VecDeque::from([vec![0.1; 480].into_iter()]),
            buffered: 480,
            eof: false,
            rebuffering: true,
            policy,
            channels: 1,
            rate: 1000,
            progress: Arc::new(Progress::default()),
            power: 0.0,
            window: 0,
        };
        assert_eq!(source.next(), Some(0.1));
        assert!(!source.rebuffering);
        assert!(!source.eof);
        assert_eq!(source.buffered, 1439);
        drop(send);
    }
}
