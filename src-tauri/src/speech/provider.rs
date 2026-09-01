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

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SpeechViseme {
    A,
    I,
    U,
    E,
    O,
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
pub struct VisemeCue {
    pub offset_ms: u64,
    pub duration_ms: u64,
    pub viseme: SpeechViseme,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SynthesizedSpeech {
    pub audio: SpeechAudio,
    pub visemes: Vec<VisemeCue>,
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

#[async_trait]
pub trait TtsProvider: Send + Sync {
    fn id(&self) -> &str;
    async fn synthesize(
        &self,
        request: SpeechSynthesisRequest,
    ) -> Result<SynthesizedSpeech, SpeechError>;
}

#[async_trait]
pub trait SttProvider: Send + Sync {
    fn id(&self) -> &str;
    async fn transcribe(
        &self,
        request: SpeechRecognitionRequest,
    ) -> Result<SpeechRecognitionResult, SpeechError>;
}
