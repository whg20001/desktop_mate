use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopRect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

#[derive(Clone, Debug)]
pub enum ImageInput {
    ScreenRegion(DesktopRect),
    StaticImageBytes(Vec<u8>),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DetectedRegion {
    pub label: String,
    pub bounds: DesktopRect,
    pub confidence: f32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VisionResult {
    pub description: String,
    pub tags: Vec<String>,
    pub detections: Vec<DetectedRegion>,
}

#[derive(Debug, Error)]
pub enum VisionError {
    #[error("视觉能力默认关闭")]
    Disabled,
    #[error("视觉 provider 请求失败: {0}")]
    Provider(String),
}

#[async_trait]
pub trait VisionProvider: Send + Sync {
    fn id(&self) -> &str;
    async fn describe(
        &self,
        image: ImageInput,
        prompt: Option<String>,
    ) -> Result<VisionResult, VisionError>;
}
