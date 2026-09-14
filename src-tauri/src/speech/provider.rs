use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AudioEncoding {
    PcmS16Le,
    Wav,
    Mp3,
    OggOpus,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpeechAudio {
    pub data: Vec<u8>,
    pub encoding: AudioEncoding,
    pub sample_rate_hz: u32,
    pub channels: u16,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpeechSynthesisRequest {
    pub text: String,
    pub voice_id: Option<String>,
    pub language: Option<String>,
    pub rate: Option<f32>,
    pub pitch: Option<f32>,
    pub volume: Option<f32>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpeechRecognitionRequest {
    pub audio: SpeechAudio,
    pub language: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpeechRecognitionResult {
    pub text: String,
    pub confidence: Option<f32>,
}

#[derive(Debug, Error)]
pub enum SpeechError {
    #[error("语音 provider 尚未配置")]
    NotConfigured,
    #[error("语音请求无效: {0}")]
    InvalidRequest(String),
    #[error("语音 provider 请求失败: {0}")]
    Provider(String),
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderTimings {
    #[serde(default)]
    pub service_queue_ms: f64,
    #[serde(default)]
    pub model_ms: f64,
    #[serde(default)]
    pub codec_decode_ms: f64,
    pub first_chunk_ms: Option<f64>,
}
pub struct AudioChunk {
    pub wav: Vec<u8>,
    pub timings: ProviderTimings,
}

pub trait TtsProvider: Send + Sync {
    /// Returns complete WAV bytes; playback reads format metadata from the header.
    fn synthesize(
        &self,
        request: &SpeechSynthesisRequest,
        cancel: &super::model::Cancellation,
        deadline: std::time::Instant,
    ) -> Result<Vec<u8>, super::model::AudioError>;

    fn stream(
        &self,
        request: &SpeechSynthesisRequest,
        cancel: &super::model::Cancellation,
        deadline: std::time::Instant,
        chunk: &mut dyn FnMut(AudioChunk) -> Result<(), super::model::AudioError>,
    ) -> Result<ProviderTimings, super::model::AudioError> {
        let start = std::time::Instant::now();
        let wav = self.synthesize(request, cancel, deadline)?;
        let timings = ProviderTimings {
            model_ms: start.elapsed().as_secs_f64() * 1000.0,
            ..Default::default()
        };
        chunk(AudioChunk {
            wav,
            timings: timings.clone(),
        })?;
        Ok(timings)
    }
}

#[async_trait]
pub trait SttProvider: Send + Sync {
    fn id(&self) -> &str;
    async fn transcribe(
        &self,
        request: SpeechRecognitionRequest,
    ) -> Result<SpeechRecognitionResult, SpeechError>;
}
