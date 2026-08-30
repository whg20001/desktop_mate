use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentRequest {
    pub system_prompt: String,
    pub history: Vec<ChatMessage>,
    pub retrieved_memory: Vec<String>,
    pub desktop_context: Option<serde_json::Value>,
    pub vision_context: Option<String>,
    pub user_input: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentResponse {
    pub speech: String,
    pub emotion: Option<serde_json::Value>,
    pub action: Option<serde_json::Value>,
    pub memory_write: Vec<String>,
}

#[derive(Debug, Error)]
pub enum AgentError {
    #[error("AI provider 尚未配置")]
    NotConfigured,
    #[error("AI provider 请求失败: {0}")]
    Provider(String),
}

#[async_trait]
pub trait AgentProvider: Send + Sync {
    fn id(&self) -> &str;
    fn supports_vision(&self) -> bool {
        false
    }
    async fn chat(&self, request: AgentRequest) -> Result<AgentResponse, AgentError>;
}
