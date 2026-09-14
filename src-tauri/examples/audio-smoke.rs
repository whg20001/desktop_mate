//! Explicit native-device smoke check: cargo run --example audio-smoke [-- --play]
use desktop_companion_lib::speech::{
    model::{AudioStart, Cancellation, SessionKey},
    playback,
    provider::{SpeechSynthesisRequest, TtsProvider},
    runtime::AudioRuntime,
    windows_tts::WindowsTtsProvider,
};
use std::{
    sync::{Arc, Mutex},
    thread,
    time::{Duration, Instant},
};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let voices = WindowsTtsProvider::voices()?;
    println!("Installed voices: {}", voices.len());
    for voice in &voices {
        println!("{} ({})", voice.name, voice.language);
    }
    let mut chosen = None;
    for language in ["zh-CN", "en-US"] {
        let Some(voice) = voices.iter().find(|v| v.language == language) else {
            println!("SKIP {language}: no installed voice");
            continue;
        };
        let request = SpeechSynthesisRequest {
            text: if language == "zh-CN" {
                "你好，原生语音测试。".into()
            } else {
                "Hello, native audio test.".into()
            },
            voice_id: Some(voice.id.clone()),
            language: Some(language.into()),
            rate: Some(1.0),
            pitch: Some(1.0),
            volume: Some(0.35),
        };
        let start = Instant::now();
        let wav = WindowsTtsProvider.synthesize(
            &request,
            &Cancellation::default(),
            Instant::now() + Duration::from_secs(30),
        )?;
        let pcm = playback::decode_wav(wav, &Cancellation::default())?;
        println!(
            "{language}: synthesis+decode={}ms, rate={}, channels={}, duration={:.2}s",
            start.elapsed().as_millis(),
            pcm.sample_rate,
            pcm.channels,
            pcm.duration_seconds()
        );
        if chosen.is_none() {
            chosen = Some(request);
        }
    }
    if std::env::args().any(|a| a == "--play") {
        let request = chosen.ok_or("No installed test language")?;
        let events = Arc::new(Mutex::new(Vec::new()));
        let sink = events.clone();
        let runtime = AudioRuntime::new(move |e| {
            if e.kind != "frame" {
                println!(
                    "event {} seq={} error={:?}",
                    e.kind, e.key.sequence, e.error
                );
            }
            sink.lock().unwrap().push(e);
        })?;
        let generation = runtime.open()?;
        let key = SessionKey {
            session_id: uuid::Uuid::new_v4().to_string(),
            generation,
            sequence: 1,
        };
        runtime.start(AudioStart {
            key: key.clone(),
            speech: request.clone(),
            timeout_seconds: 30,
        })?;
        wait(
            || {
                events
                    .lock()
                    .unwrap()
                    .iter()
                    .any(|e| matches!(e.kind.as_str(), "completed" | "failed"))
            },
            35,
        )?;
        {
            let events = events.lock().unwrap();
            let last = events.last().unwrap();
            println!(
                "Playback: {}, frames={}, peak={:.3}",
                last.kind,
                events.iter().filter(|e| e.kind == "frame").count(),
                events.iter().map(|e| e.level).fold(0_f32, f32::max)
            );
            if let Some(error) = &last.error {
                return Err(error.clone().into());
            }
        }
        let key = SessionKey {
            session_id: uuid::Uuid::new_v4().to_string(),
            generation,
            sequence: 2,
        };
        runtime.start(AudioStart {
            key: key.clone(),
            speech: request,
            timeout_seconds: 30,
        })?;
        wait(
            || {
                events.lock().unwrap().iter().any(|e| {
                    e.key == key && matches!(e.kind.as_str(), "started" | "failed" | "completed")
                })
            },
            35,
        )?;
        if let Some(error) = runtime.status().last_event.and_then(|e| e.error) {
            return Err(error.into());
        }
        let before = Instant::now();
        runtime.cancel(&key)?;
        wait(
            || {
                events
                    .lock()
                    .unwrap()
                    .iter()
                    .any(|e| e.key == key && e.kind == "cancelled")
            },
            5,
        )?;
        println!(
            "Cancellation acknowledged after stream cleanup: {}ms (not an acoustic measurement)",
            before.elapsed().as_millis()
        );
        runtime.shutdown();
        println!("Worker stopped: {}", !runtime.status().worker_alive);
    }
    Ok(())
}
fn wait(test: impl Fn() -> bool, seconds: u64) -> Result<(), Box<dyn std::error::Error>> {
    let deadline = Instant::now() + Duration::from_secs(seconds);
    while !test() {
        if Instant::now() >= deadline {
            return Err("Smoke check timed out".into());
        }
        thread::sleep(Duration::from_millis(10));
    }
    Ok(())
}
