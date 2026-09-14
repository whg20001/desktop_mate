use super::{
    model::{
        AudioError, AudioEvent, AudioStart, AudioStatus, AudioTimings, Cancellation, SessionKey,
    },
    provider::TtsProvider,
    stream_playback,
    windows_tts::WindowsTtsProvider,
};
use parking_lot::{Condvar, Mutex};
use std::{
    sync::Arc,
    thread::{self, JoinHandle},
    time::Instant,
};

type Runner = dyn Fn(
        &AudioStart,
        &Cancellation,
        &mut dyn FnMut(&str, u64, f32),
        &Mutex<AudioTimings>,
    ) -> Result<(), AudioError>
    + Send
    + Sync;
type EventSink = dyn Fn(AudioEvent) + Send + Sync;
struct Job {
    enqueued: Instant,
    timings: Mutex<AudioTimings>,
    request: AudioStart,
    cancel: Cancellation,
    events: Mutex<(u64, bool)>,
}
#[derive(Default)]
struct State {
    generation: u64,
    watermark: u64,
    stopped: bool,
    worker_alive: bool,
    pending: Option<Arc<Job>>,
    running: Option<Arc<Job>>,
    current: Option<SessionKey>,
    last_event: Option<AudioEvent>,
    timings: Option<Arc<Job>>,
}
struct Shared {
    state: Mutex<State>,
    wake: Condvar,
    emit: Arc<EventSink>,
}
pub struct AudioRuntime {
    shared: Arc<Shared>,
    worker: Mutex<Option<JoinHandle<()>>>,
}

impl AudioRuntime {
    pub fn new(emit: impl Fn(AudioEvent) + Send + Sync + 'static) -> Result<Arc<Self>, AudioError> {
        Self::with_provider(emit, Arc::new(WindowsTtsProvider))
    }
    pub fn with_provider(
        emit: impl Fn(AudioEvent) + Send + Sync + 'static,
        provider: Arc<dyn TtsProvider>,
    ) -> Result<Arc<Self>, AudioError> {
        Self::with_runner(
            Arc::new(emit),
            Arc::new(move |request, cancel, emit, timings| {
                stream_playback::stream(provider.as_ref(), request, cancel, emit, timings)
            }),
        )
    }
    fn with_runner(emit: Arc<EventSink>, runner: Arc<Runner>) -> Result<Arc<Self>, AudioError> {
        let shared = Arc::new(Shared {
            state: Mutex::new(State {
                worker_alive: true,
                ..State::default()
            }),
            wake: Condvar::new(),
            emit,
        });
        let worker_shared = shared.clone();
        let worker = thread::Builder::new()
            .name("native-audio".into())
            .spawn(move || worker_loop(worker_shared, runner))
            .map_err(|_| AudioError::new("audio_unavailable", "无法启动音频工作线程"))?;
        Ok(Arc::new(Self {
            shared,
            worker: Mutex::new(Some(worker)),
        }))
    }
    pub fn open(&self) -> Result<u64, AudioError> {
        let (generation, pending) = {
            let mut state = self.shared.state.lock();
            if state.stopped || !state.worker_alive {
                return Err(AudioError::new("audio_unavailable", "音频服务不可用"));
            }
            state.generation += 1;
            state.watermark = 0;
            state.current = None;
            state.last_event = None;
            if let Some(job) = &state.running {
                job.cancel.cancel();
            }
            (state.generation, state.pending.take())
        };
        if let Some(job) = pending {
            job.cancel.cancel();
            self.shared.event(&job, "cancelled", 0, 0.0, None);
        }
        Ok(generation)
    }
    pub fn start(&self, request: AudioStart) -> Result<(), AudioError> {
        request.validate()?;
        let replaced = {
            let mut state = self.shared.state.lock();
            if state.stopped || !state.worker_alive {
                return Err(AudioError::new("audio_unavailable", "音频服务不可用"));
            }
            if request.key.generation != state.generation || request.key.sequence <= state.watermark
            {
                return Err(AudioError::new("stale_session", "音频请求已过期或取消"));
            }
            state.watermark = request.key.sequence;
            state.current = Some(request.key.clone());
            state.last_event = None;
            if let Some(job) = &state.running {
                job.cancel.cancel();
            }
            state.pending.replace(Arc::new(Job {
                enqueued: Instant::now(),
                timings: Mutex::new(AudioTimings {
                    session_id: request.key.session_id.clone(),
                    ..Default::default()
                }),
                request,
                cancel: Cancellation::default(),
                events: Mutex::new((0, false)),
            }))
        };
        if let Some(job) = replaced {
            job.cancel.cancel();
            self.shared.event(&job, "cancelled", 0, 0.0, None);
        }
        self.shared.wake.notify_one();
        Ok(())
    }
    pub fn cancel(&self, key: &SessionKey) -> Result<bool, AudioError> {
        key.validate()?;
        let (pending, cancelled) = {
            let mut state = self.shared.state.lock();
            if key.generation != state.generation {
                return Ok(false);
            }
            state.watermark = state.watermark.max(key.sequence);
            let mut cancelled = false;
            if let Some(job) = &state.running {
                if job.request.key == *key {
                    job.cancel.cancel();
                    cancelled = true;
                }
            }
            let pending = if state
                .pending
                .as_ref()
                .is_some_and(|job| job.request.key == *key)
            {
                cancelled = true;
                state.pending.take()
            } else {
                None
            };
            (pending, cancelled)
        };
        if let Some(job) = pending {
            job.cancel.cancel();
            self.shared.event(&job, "cancelled", 0, 0.0, None);
        }
        Ok(cancelled)
    }
    pub fn status(&self) -> AudioStatus {
        let s = self.shared.state.lock();
        AudioStatus {
            timings: s.timings.as_ref().map(|job| job.timings.lock().clone()),
            generation: s.generation,
            last_event: s.last_event.clone(),
            worker_alive: s.worker_alive,
        }
    }
    pub fn cancel_all(&self) {
        let pending = {
            let mut state = self.shared.state.lock();
            if let Some(job) = &state.running {
                job.cancel.cancel();
            }
            state.pending.take()
        };
        if let Some(job) = pending {
            job.cancel.cancel();
            self.shared.event(&job, "cancelled", 0, 0.0, None);
        }
    }
    pub fn invalidate(&self) {
        let _ = self.open();
    }
    pub fn shutdown(&self) {
        let pending = {
            let mut s = self.shared.state.lock();
            s.stopped = true;
            if let Some(job) = &s.running {
                job.cancel.cancel();
            }
            s.pending.take()
        };
        if let Some(job) = pending {
            job.cancel.cancel();
            self.shared.event(&job, "cancelled", 0, 0.0, None);
        }
        self.shared.wake.notify_one();
        if let Some(worker) = self.worker.lock().take() {
            let _ = worker.join();
        }
    }
}
impl Drop for AudioRuntime {
    fn drop(&mut self) {
        self.shutdown();
    }
}
impl Shared {
    fn event(&self, job: &Job, kind: &str, position: u64, level: f32, error: Option<AudioError>) {
        let mut order = job.events.lock();
        if order.1 {
            return;
        }
        let kind = if job.cancel.is_cancelled() {
            if matches!(kind, "completed" | "failed" | "cancelled") {
                "cancelled"
            } else {
                return;
            }
        } else {
            kind
        };
        order.0 += 1;
        order.1 = matches!(kind, "completed" | "cancelled" | "failed");
        let event = AudioEvent {
            key: job.request.key.clone(),
            event_sequence: order.0,
            kind: kind.into(),
            position_ms: position,
            level: if order.1 { 0.0 } else { level },
            error,
        };
        {
            let mut state = self.state.lock();
            if state.current.as_ref() == Some(&event.key) {
                state.last_event = Some(event.clone());
            }
        }
        (self.emit)(event);
    }
}
fn worker_loop(shared: Arc<Shared>, runner: Arc<Runner>) {
    loop {
        let job = {
            let mut state = shared.state.lock();
            while state.pending.is_none() && !state.stopped {
                shared.wake.wait(&mut state);
            }
            if state.stopped {
                state.worker_alive = false;
                return;
            }
            let job = state.pending.take().unwrap();
            state.running = Some(job.clone());
            state.timings = Some(job.clone());
            job.timings.lock().queue_ms = job.enqueued.elapsed().as_secs_f64() * 1000.0;
            job
        };
        shared.event(&job, "preparing", 0, 0.0, None);
        let mut emit =
            |kind: &str, position: u64, level: f32| shared.event(&job, kind, position, level, None);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            job.cancel.check()?;
            runner(&job.request, &job.cancel, &mut emit, &job.timings)
        }))
        .unwrap_or_else(|_| Err(AudioError::new("playback_failed", "音频任务异常结束")));
        job.timings.lock().total_ms = Some(job.enqueued.elapsed().as_secs_f64() * 1000.0);
        eprintln!(
            "[audio-timing] {}",
            serde_json::to_string(&*job.timings.lock()).unwrap_or_default()
        );
        if job.cancel.is_cancelled() {
            shared.event(&job, "cancelled", 0, 0.0, None);
        } else {
            match result {
                Ok(()) => shared.event(&job, "completed", 0, 0.0, None),
                Err(error) => shared.event(&job, "failed", 0, 0.0, Some(error)),
            }
        }
        shared.state.lock().running = None;
    }
}

#[cfg(test)]
mod tests {
    use super::super::provider::SpeechSynthesisRequest;
    use super::*;
    use std::time::Duration;
    fn request(generation: u64, sequence: u64) -> AudioStart {
        AudioStart {
            key: SessionKey {
                generation,
                sequence,
                session_id: uuid::Uuid::new_v4().to_string(),
            },
            speech: SpeechSynthesisRequest {
                text: "hello".into(),
                voice_id: None,
                language: None,
                rate: None,
                pitch: None,
                volume: None,
            },
            timeout_seconds: 30,
        }
    }
    fn wait_until(test: impl Fn() -> bool) {
        let deadline = Instant::now() + Duration::from_secs(2);
        while !test() {
            assert!(Instant::now() < deadline);
            thread::sleep(Duration::from_millis(5));
        }
    }
    #[test]
    fn rejects_cancelled_and_reordered_requests_and_old_generations() {
        let runtime =
            AudioRuntime::with_runner(Arc::new(|_| {}), Arc::new(|_, _, _, _| Ok(()))).unwrap();
        let generation = runtime.open().unwrap();
        let a = request(generation, 1);
        runtime.cancel(&a.key).unwrap();
        assert_eq!(runtime.start(a).unwrap_err().code, "stale_session");
        runtime.start(request(generation, 3)).unwrap();
        assert!(runtime.start(request(generation, 2)).is_err());
        runtime.open().unwrap();
        assert!(runtime.start(request(generation, 4)).is_err());
        runtime.shutdown();
        assert!(!runtime.status().worker_alive);
    }
    #[test]
    fn replacing_and_shutdown_cancel_without_overlapping_playback() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let active = Arc::new(AtomicUsize::new(0));
        let active_runner = active.clone();
        let events = Arc::new(Mutex::new(Vec::new()));
        let sink = events.clone();
        let runtime = AudioRuntime::with_runner(
            Arc::new(move |e| sink.lock().push(e)),
            Arc::new(move |_, cancel, emit, _| {
                assert_eq!(active_runner.fetch_add(1, Ordering::SeqCst), 0);
                emit("started", 1, 0.0);
                while !cancel.is_cancelled() {
                    thread::sleep(Duration::from_millis(2));
                }
                active_runner.fetch_sub(1, Ordering::SeqCst);
                cancel.check()
            }),
        )
        .unwrap();
        let generation = runtime.open().unwrap();
        let a = request(generation, 1);
        let b = request(generation, 2);
        runtime.start(a.clone()).unwrap();
        wait_until(|| {
            events
                .lock()
                .iter()
                .any(|e| e.key == a.key && e.kind == "started")
        });
        runtime.start(b.clone()).unwrap();
        wait_until(|| {
            events
                .lock()
                .iter()
                .any(|e| e.key == b.key && e.kind == "started")
        });
        runtime.cancel(&a.key).unwrap();
        assert_eq!(active.load(Ordering::SeqCst), 1);
        runtime.shutdown();
        assert_eq!(active.load(Ordering::SeqCst), 0);
        for key in [a.key, b.key] {
            let events = events.lock();
            let ends: Vec<_> = events
                .iter()
                .filter(|e| e.key == key && e.kind == "cancelled")
                .collect();
            assert_eq!(ends.len(), 1);
            assert_eq!(ends[0].level, 0.0);
        }
    }
    #[test]
    fn failure_is_reported_and_next_request_can_run() {
        let runtime = AudioRuntime::with_runner(
            Arc::new(|_| {}),
            Arc::new(|_, _, _, _| Err(AudioError::new("synthesis_timeout", "timeout"))),
        )
        .unwrap();
        let generation = runtime.open().unwrap();
        runtime.start(request(generation, 1)).unwrap();
        wait_until(|| {
            runtime
                .status()
                .last_event
                .is_some_and(|e| e.kind == "failed")
        });
        runtime.start(request(generation, 2)).unwrap();
        wait_until(|| {
            runtime
                .status()
                .last_event
                .is_some_and(|e| e.key.sequence == 2 && e.kind == "failed")
        });
        runtime.shutdown();
    }
    #[test]
    fn invalid_requests_do_not_consume_the_next_sequence() {
        let runtime =
            AudioRuntime::with_runner(Arc::new(|_| {}), Arc::new(|_, _, _, _| Ok(()))).unwrap();
        let generation = runtime.open().unwrap();
        let mut invalid = request(generation, 1);
        invalid.speech.rate = Some(f32::NAN);
        assert_eq!(runtime.start(invalid).unwrap_err().code, "invalid_request");
        let mut invalid = request(generation, 1);
        invalid.speech.text = "a".repeat(8001);
        assert!(runtime.start(invalid).is_err());
        runtime.start(request(generation, 1)).unwrap();
        runtime.shutdown();
    }
    #[test]
    fn reloading_cancels_an_active_job_and_rejects_its_late_events() {
        let runtime = AudioRuntime::with_runner(
            Arc::new(|_| {}),
            Arc::new(|_, cancel, emit, _| {
                emit("started", 1, 0.0);
                while !cancel.is_cancelled() {
                    thread::sleep(Duration::from_millis(2));
                }
                emit("frame", 10, 1.0);
                Ok(())
            }),
        )
        .unwrap();
        let generation = runtime.open().unwrap();
        runtime.start(request(generation, 1)).unwrap();
        wait_until(|| {
            runtime
                .status()
                .last_event
                .is_some_and(|e| e.kind == "started")
        });
        assert!(runtime.open().unwrap() > generation);
        runtime.shutdown();
        assert!(runtime.status().last_event.is_none());
    }
}
