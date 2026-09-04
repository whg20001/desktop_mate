use serde::{Deserialize, Serialize};

use crate::ai::provider::ChatMessage;

pub type MemoryId = String;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum MemoryKind {
    Semantic,
    Episodic,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryScope {
    pub user_id: String,
    pub character_id: String,
    pub session_id: Option<String>,
}

impl MemoryScope {
    pub fn validate(&self) -> Result<(), String> {
        validate_scope_part("userId", &self.user_id)?;
        validate_scope_part("characterId", &self.character_id)?;
        if let Some(session_id) = &self.session_id {
            validate_scope_part("sessionId", session_id)?;
        }
        Ok(())
    }

    pub fn require_session_id(&self) -> Result<&str, String> {
        self.validate()?;
        self.session_id
            .as_deref()
            .ok_or_else(|| "sessionId is required for session memory".to_string())
    }
}

fn validate_scope_part(name: &str, value: &str) -> Result<(), String> {
    let value = value.trim();
    if value.is_empty() || value.len() > 128 {
        return Err(format!("{name} must contain 1 to 128 characters"));
    }
    if value.chars().any(char::is_control) {
        return Err(format!("{name} cannot contain control characters"));
    }
    Ok(())
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TimeRange {
    pub start_unix_ms: Option<i64>,
    pub end_unix_ms: Option<i64>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryWriteProposal {
    pub content: String,
    pub suggested_kind: Option<MemoryKind>,
    pub importance: Option<f32>,
    #[serde(default)]
    pub tags: Vec<String>,
    pub occurred_at_unix_ms: Option<i64>,
    #[serde(default)]
    pub source_event_ids: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryEntry {
    pub scope: MemoryScope,
    pub kind: MemoryKind,
    pub content: String,
    pub importance: f32,
    pub tags: Vec<String>,
    pub occurred_at_unix_ms: Option<i64>,
    pub created_at_unix_ms: i64,
    pub source_event_ids: Vec<String>,
    pub metadata: serde_json::Value,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryQuery {
    pub scope: MemoryScope,
    pub text: String,
    #[serde(default)]
    pub kinds: Vec<MemoryKind>,
    pub top_k: usize,
    pub time_range: Option<TimeRange>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryRecord {
    pub id: MemoryId,
    pub entry: MemoryEntry,
    pub score: f32,
    pub provider_id: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TemporalRelation {
    pub source: String,
    pub relation: String,
    pub target: String,
    pub occurred_at_unix_ms: Option<i64>,
    pub score: f32,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryContext {
    pub recent_messages: Vec<ChatMessage>,
    pub semantic: Vec<MemoryRecord>,
    pub episodic: Vec<MemoryRecord>,
    pub temporal_relations: Vec<TemporalRelation>,
    #[serde(default)]
    pub degraded_reasons: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApprovedMemoryEvent {
    pub id: MemoryId,
    pub entry: MemoryEntry,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConversationEvent {
    pub id: String,
    pub scope: MemoryScope,
    pub role: String,
    pub content: String,
    pub created_at_unix_ms: i64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryExport {
    pub scope: MemoryScope,
    pub conversation_events: Vec<ConversationEvent>,
    pub approved_memories: Vec<ApprovedMemoryEvent>,
    pub exported_at_unix_ms: i64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryListFilter {
    #[serde(default)]
    pub kinds: Vec<MemoryKind>,
    #[serde(default = "default_list_limit")]
    pub limit: usize,
}

fn default_list_limit() -> usize {
    100
}

impl Default for MemoryListFilter {
    fn default() -> Self {
        Self {
            kinds: Vec::new(),
            limit: default_list_limit(),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum MemoryPrivacyMode {
    Disabled,
    #[default]
    LocalOnly,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderHealth {
    pub provider_id: String,
    pub available: bool,
    pub all_local: bool,
    pub version: Option<String>,
    pub detail: String,
}

impl ProviderHealth {
    pub fn unavailable(provider_id: impl Into<String>, detail: impl Into<String>) -> Self {
        Self {
            provider_id: provider_id.into(),
            available: false,
            all_local: false,
            version: None,
            detail: detail.into(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryStatus {
    pub privacy_mode: MemoryPrivacyMode,
    pub memory_provider: ProviderHealth,
    pub inference_provider: ProviderHealth,
    pub temporal_graph_phase: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryMutationReport {
    pub local_source_updated: bool,
    pub provider_updated: bool,
    #[serde(default)]
    pub warnings: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryCommitReport {
    pub memory_id: MemoryId,
    pub pending_provider_sync: bool,
    #[serde(default)]
    pub warnings: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryContextBudget {
    #[serde(default = "default_recent_limit")]
    pub recent_message_limit: usize,
    #[serde(default = "default_record_limit")]
    pub long_term_record_limit: usize,
    #[serde(default = "default_context_chars")]
    pub max_characters: usize,
}

const fn default_recent_limit() -> usize {
    20
}

const fn default_record_limit() -> usize {
    8
}

const fn default_context_chars() -> usize {
    8_000
}

impl Default for MemoryContextBudget {
    fn default() -> Self {
        Self {
            recent_message_limit: default_recent_limit(),
            long_term_record_limit: default_record_limit(),
            max_characters: default_context_chars(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalMemoryExtractionInput {
    pub scope: MemoryScope,
    pub messages: Vec<ChatMessage>,
    #[serde(default)]
    pub source_event_ids: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalMemoryConsolidationInput {
    pub scope: MemoryScope,
    pub existing: Vec<MemoryRecord>,
    pub candidates: Vec<MemoryWriteProposal>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TemporalQuery {
    pub scope: MemoryScope,
    pub text: String,
    pub top_k: usize,
    pub time_range: Option<TimeRange>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum LocalMemoryService {
    Mem0,
    Inference,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalMemoryEndpointConfig {
    pub service: LocalMemoryService,
    pub endpoint: String,
    pub model: Option<String>,
}
