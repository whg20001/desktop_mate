//! Real CUDA/model/AudioEngine smoke. Add --play for audible playback.
use desktop_companion_lib::speech::{
    model::{AudioStart, Cancellation, SessionKey},
    playback,
    provider::{SpeechSynthesisRequest, TtsProvider},
    runtime::AudioRuntime,
    service::ModelService,
};
use std::{
    sync::{Arc, Mutex},
    thread,
    time::{Duration, Instant},
};
fn request(sequence: u64, generation: u64) -> AudioStart {
    AudioStart {
        key: SessionKey {
            session_id: uuid::Uuid::new_v4().to_string(),
            generation,
            sequence,
        },
        speech: SpeechSynthesisRequest {
            text: "你好，我是你的桌面伙伴。".into(),
            voice_id: Some("serena".into()),
            language: Some("zh-CN".into()),
            rate: None,
            pitch: None,
            volume: Some(0.35),
        },
        timeout_seconds: 180,
    }
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let directory =
        std::env::temp_dir().join(format!("desktop-qwen-smoke-{}", uuid::Uuid::new_v4()));
    let model = ModelService::new(&directory)?;
    let result = (|| -> Result<(), Box<dyn std::error::Error>> {
        let start = Instant::now();
        loop {
            let status = model.status();
            if status.phase == "failed" {
                return Err(status.detail.into());
            }
            if status.phase == "ready" {
                println!(
                    "Ready: {} device={:?} voices={} load={:.2}s",
                    status.name,
                    status.device,
                    status.speakers.len(),
                    start.elapsed().as_secs_f64()
                );
                break;
            }
            if start.elapsed() > Duration::from_secs(180) {
                return Err("Loading timed out".into());
            }
            thread::sleep(Duration::from_millis(100));
        }
        if model.status().selected_model == "windows" {
            return Err("Qwen was not selected".into());
        }
        let start = Instant::now();
        let bytes = model.synthesize(
            &request(1, 1).speech,
            &Cancellation::default(),
            Instant::now() + Duration::from_secs(180),
        )?;
        let audio = playback::decode_wav(bytes, &Cancellation::default())?;
        println!(
            "Synthesis+decode: {:.2}s, {}Hz, {} channels, {:.2}s audio",
            start.elapsed().as_secs_f64(),
            audio.sample_rate,
            audio.channels,
            audio.duration_seconds()
        );
        let events = Arc::new(Mutex::new(Vec::new()));
        let sink = events.clone();
        let runtime = AudioRuntime::with_provider(
            move |event| {
                sink.lock().unwrap().push(event);
            },
            model.clone(),
        )?;
        let generation = runtime.open()?;
        let pending = request(1, generation);
        runtime.start(pending.clone())?;
        thread::sleep(Duration::from_millis(1000));
        let start = Instant::now();
        runtime.cancel(&pending.key)?;
        loop {
            if events
                .lock()
                .unwrap()
                .iter()
                .any(|e| e.key == pending.key && e.kind == "cancelled")
            {
                break;
            }
            if start.elapsed() > Duration::from_secs(5) {
                return Err("Cancel timed out".into());
            }
            thread::sleep(Duration::from_millis(10));
        }
        println!(
            "Generation cancelled in {}ms; model={}",
            start.elapsed().as_millis(),
            model.status().phase
        );
        if std::env::args().any(|s| s == "--play") {
            let pending = request(2, generation);
            runtime.start(pending.clone())?;
            let start = Instant::now();
            loop {
                let done = events
                    .lock()
                    .unwrap()
                    .iter()
                    .find(|e| {
                        e.key == pending.key && matches!(e.kind.as_str(), "completed" | "failed")
                    })
                    .cloned();
                if let Some(done) = done {
                    if let Some(error) = done.error {
                        return Err(error.into());
                    }
                    break;
                }
                if start.elapsed() > Duration::from_secs(300) {
                    return Err("Playback timed out".into());
                }
                thread::sleep(Duration::from_millis(50));
            }
            println!(
                "Playback completed, RMS frames={}",
                events
                    .lock()
                    .unwrap()
                    .iter()
                    .filter(|e| e.kind == "frame")
                    .count()
            );
        }
        println!(
            "Timings: {}",
            serde_json::to_string(&runtime.status().timings)?
        );
        if std::env::args().any(|s| s == "--play") {
            let timing = runtime.status().timings.unwrap();
            assert!(timing.chunks > 1, "Expected multiple audio chunks");
            assert!(
                timing.first_playback_ms.unwrap() >= timing.first_chunk_received_ms.unwrap()
                    && timing.startup_audio_ms > 0.0,
                "Playback must honor the measured startup reserve"
            );
        }
        runtime.shutdown();
        let pid = model.status().pid;
        model.select("windows")?;
        println!(
            "Switched to Windows; previous PID={pid:?}, state={}, voices={}",
            model.status().phase,
            model.voices()?.len()
        );
        Ok(())
    })();
    model.shutdown();
    let _ = std::fs::remove_file(directory.join("audio-model.json"));
    let _ = std::fs::remove_dir(directory);
    result
}
