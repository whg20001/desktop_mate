use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use thiserror::Error;

pub type MemoryId = String;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MemoryKind {
    ShortTerm,
    LongTerm,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryEntry {
    pub session_id: String,
    pub kind: MemoryKind,
    pub role: String,
    pub content: String,
    pub importance: f32,
    pub tags: Vec<String>,
    pub metadata: serde_json::Value,
    pub created_at_unix_ms: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryQuery {
    pub session_id: Option<String>,
    pub text: String,
    pub top_k: usize,
    pub filters: Vec<(String, String)>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryRecord {
    pub id: MemoryId,
    pub entry: MemoryEntry,
    pub score: f32,
}

#[derive(Debug, Error)]
pub enum MemoryError {
    #[error("记忆 provider 尚未配置")]
    NotConfigured,
    #[error("记忆操作失败: {0}")]
    Provider(String),
}

#[async_trait]
pub trait MemoryProvider: Send + Sync {
    fn id(&self) -> &str;
    async fn add(&self, entry: MemoryEntry) -> Result<MemoryId, MemoryError>;
    async fn search(&self, query: MemoryQuery) -> Result<Vec<MemoryRecord>, MemoryError>;
    async fn get_recent(
        &self,
        session_id: &str,
        limit: usize,
    ) -> Result<Vec<MemoryRecord>, MemoryError>;
    async fn forget(&self, id: MemoryId) -> Result<(), MemoryError>;
}
