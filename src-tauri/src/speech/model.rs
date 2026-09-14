use super::provider::SpeechSynthesisRequest;
use serde::{Deserialize, Serialize};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

pub const MAX_AUDIO_BYTES: usize = 64 * 1024 * 1024;
pub const MAX_DURATION_SECONDS: f64 = 120.0;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioError {
    pub code: &'static str,
    pub message: String,
}
impl AudioError {
    pub fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
    pub fn cancelled() -> Self {
        Self::new("cancelled", "语音已取消")
    }
}
impl std::fmt::Display for AudioError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}
impl std::error::Error for AudioError {}

#[derive(Clone, Default)]
pub struct Cancellation(Arc<AtomicBool>);
impl Cancellation {
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Release);
    }
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
    pub fn check(&self) -> Result<(), AudioError> {
        if self.is_cancelled() {
            Err(AudioError::cancelled())
        } else {
            Ok(())
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SessionKey {
    pub session_id: String,
    pub generation: u64,
    pub sequence: u64,
}
impl SessionKey {
    pub fn validate(&self) -> Result<(), AudioError> {
        if uuid::Uuid::parse_str(&self.session_id).is_err()
            || self.generation == 0
            || self.sequence == 0
            || self.sequence > 9_007_199_254_740_991
        {
            return Err(AudioError::new("invalid_request", "无效的音频会话"));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AudioStart {
    pub key: SessionKey,
    pub speech: SpeechSynthesisRequest,
    #[serde(default = "default_timeout")]
    pub timeout_seconds: u64,
}
fn default_timeout() -> u64 {
    30
}
impl AudioStart {
    pub fn validate(&self) -> Result<(), AudioError> {
        self.key.validate()?;
        let s = &self.speech;
        if s.text.trim().is_empty()
            || s.text.chars().count() > 8000
            || s.text.contains('\0')
            || !(2..=180).contains(&self.timeout_seconds)
        {
            return Err(AudioError::new("invalid_request", "语音文本或超时时间无效"));
        }
        for (value, min, max) in [
            (s.rate, 0.5, 2.0),
            (s.pitch, 0.0, 2.0),
            (s.volume, 0.0, 1.0),
        ] {
            if value.is_some_and(|v| !v.is_finite() || v < min || v > max) {
                return Err(AudioError::new("invalid_request", "语音参数超出允许范围"));
            }
        }
        if s.voice_id
            .as_ref()
            .is_some_and(|v| v.len() > 1024 || v.contains('\0'))
            || s.language
                .as_ref()
                .is_some_and(|v| v.len() > 35 || v.contains('\0'))
        {
            return Err(AudioError::new("invalid_request", "音色或语言参数无效"));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VoiceInfo {
    pub id: String,
    pub name: String,
    pub language: String,
    pub is_default: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioEvent {
    #[serde(flatten)]
    pub key: SessionKey,
    pub event_sequence: u64,
    #[serde(rename = "type")]
    pub kind: String,
    pub position_ms: u64,
    pub level: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<AudioError>,
}
#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioStatus {
    pub timings: Option<AudioTimings>,
    pub generation: u64,
    pub last_event: Option<AudioEvent>,
    pub worker_alive: bool,
}

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioTimings {
    pub session_id: String,
    pub queue_ms: f64,
    #[serde(flatten)]
    pub provider: super::provider::ProviderTimings,
    pub wav_decode_ms: f64,
    pub first_chunk_received_ms: Option<f64>,
    pub first_playback_ms: Option<f64>,
    pub synthesis_done_ms: Option<f64>,
    pub total_ms: Option<f64>,
    pub chunks: u64,
    pub underrun_ms: f64,
    pub startup_buffer_ms: f64,
    pub startup_audio_ms: f64,
    pub buffer_target_ms: f64,
    pub generation_rtf: Option<f64>,
    pub rebuffer_count: u64,
}
