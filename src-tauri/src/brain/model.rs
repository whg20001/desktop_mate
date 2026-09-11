use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum BrainPhase {
    Stopped,
    Starting,
    Ready,
    Degraded,
    Restarting,
    Failed,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BrainStatus {
    pub phase: BrainPhase,
    pub ready: bool,
    pub pid: Option<u32>,
    pub restart_count: u32,
    pub detail: String,
}

impl BrainStatus {
    pub fn unavailable(detail: impl Into<String>) -> Self {
        Self {
            phase: BrainPhase::Failed,
            ready: false,
            pid: None,
            restart_count: 0,
            detail: detail.into(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BrainSettings {
    #[serde(default)]
    pub llm_base_url: String,
    #[serde(default)]
    pub llm_model: String,
    #[serde(default)]
    pub embedding_base_url: String,
    #[serde(default)]
    pub embedding_model: String,
    #[serde(default = "default_embedding_dimensions")]
    pub embedding_dimensions: usize,
    #[serde(default = "enabled")]
    pub memory_enabled: bool,
    #[serde(default = "enabled")]
    pub recall_enabled: bool,
    #[serde(default = "enabled")]
    pub memory_write_enabled: bool,
    #[serde(default = "default_recall_limit")]
    pub recall_limit: usize,
    #[serde(default = "default_timeout")]
    pub request_timeout_seconds: u64,
    #[serde(default)]
    pub graphiti_enabled: bool,
    #[serde(default = "default_graphiti_uri")]
    pub graphiti_uri: String,
    #[serde(default = "default_graphiti_database")]
    pub graphiti_database: String,
    #[serde(default = "default_graphiti_user")]
    pub graphiti_user: String,
    #[serde(default)]
    pub memory_approval_required: bool,
    #[serde(default = "default_memory_minimum_importance")]
    pub memory_minimum_importance: f32,
    #[serde(default = "default_memory_retention_days")]
    pub memory_retention_days: usize,
}

impl Default for BrainSettings {
    fn default() -> Self {
        Self {
            llm_base_url: String::new(),
            llm_model: String::new(),
            embedding_base_url: String::new(),
            embedding_model: String::new(),
            embedding_dimensions: default_embedding_dimensions(),
            memory_enabled: true,
            recall_enabled: true,
            memory_write_enabled: true,
            recall_limit: default_recall_limit(),
            request_timeout_seconds: default_timeout(),
            graphiti_enabled: false,
            graphiti_uri: default_graphiti_uri(),
            graphiti_database: default_graphiti_database(),
            graphiti_user: default_graphiti_user(),
            memory_approval_required: false,
            memory_minimum_importance: default_memory_minimum_importance(),
            memory_retention_days: default_memory_retention_days(),
        }
    }
}

impl BrainSettings {
    pub fn normalized(mut self) -> Self {
        self.llm_base_url = self.llm_base_url.trim().to_string();
        self.llm_model = self.llm_model.trim().to_string();
        self.embedding_base_url = self.embedding_base_url.trim().to_string();
        self.embedding_model = self.embedding_model.trim().to_string();
        self.graphiti_uri = self.graphiti_uri.trim().to_string();
        self.graphiti_database = self.graphiti_database.trim().to_string();
        self.graphiti_user = self.graphiti_user.trim().to_string();
        self.embedding_dimensions = self.embedding_dimensions.clamp(64, 8192);
        self.recall_limit = self.recall_limit.clamp(1, 20);
        self.request_timeout_seconds = self.request_timeout_seconds.clamp(2, 120);
        self.memory_minimum_importance = self.memory_minimum_importance.clamp(0.0, 1.0);
        self.memory_retention_days = self.memory_retention_days.clamp(1, 3650);
        self
    }
}

const fn enabled() -> bool {
    true
}

const fn default_recall_limit() -> usize {
    6
}

const fn default_embedding_dimensions() -> usize {
    768
}

const fn default_timeout() -> u64 {
    30
}

fn default_graphiti_uri() -> String {
    "bolt://127.0.0.1:7687".into()
}

fn default_graphiti_database() -> String {
    "neo4j".into()
}

fn default_graphiti_user() -> String {
    "neo4j".into()
}

const fn default_memory_minimum_importance() -> f32 {
    0.55
}

const fn default_memory_retention_days() -> usize {
    365
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConversationScope {
    pub user_id: String,
    pub character_id: String,
    pub session_id: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AvailableAction {
    pub id: String,
    pub description: String,
    #[serde(default)]
    pub scenes: Vec<String>,
}

impl ConversationScope {
    pub fn validate(&self) -> Result<(), String> {
        for (name, value) in [
            ("userId", &self.user_id),
            ("characterId", &self.character_id),
            ("sessionId", &self.session_id),
        ] {
            if value.trim().is_empty() || value.len() > 128 {
                return Err(format!("{name} must contain 1 to 128 characters"));
            }
            if value.chars().any(char::is_control) {
                return Err(format!("{name} cannot contain control characters"));
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConversationRequest {
    pub turn_id: Option<String>,
    pub scope: ConversationScope,
    pub user_input: String,
    #[serde(default)]
    pub available_actions: Vec<AvailableAction>,
    pub desktop_context: Option<serde_json::Value>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConversationPayload {
    pub turn_id: String,
    pub scope: ConversationScope,
    pub user_input: String,
    pub available_actions: Vec<AvailableAction>,
    pub desktop_context: Option<serde_json::Value>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CharacterResponse {
    pub turn_id: String,
    pub text: String,
    pub emotion: Option<CharacterEmotion>,
    pub action_intent: Option<CharacterActionIntent>,
    pub speech: Option<CharacterSpeech>,
    #[serde(default)]
    pub degraded_reasons: Vec<String>,
}

impl CharacterResponse {
    pub fn validate(&self, available_actions: &[AvailableAction]) -> Result<(), String> {
        if self.turn_id.trim().is_empty() || self.turn_id.len() > 128 {
            return Err("brain response has an invalid turnId".into());
        }
        if self.text.trim().is_empty() || self.text.len() > 8_000 {
            return Err("brain response text is empty or too long".into());
        }
        if let Some(emotion) = &self.emotion {
            if !matches!(
                emotion.kind.as_str(),
                "neutral" | "happy" | "concerned" | "curious" | "surprised" | "sad"
            ) || !emotion.intensity.is_finite()
                || !(0.0..=1.0).contains(&emotion.intensity)
                || !(250..=10_000).contains(&emotion.duration_ms)
            {
                return Err("brain response has an invalid emotion".into());
            }
        }
        if let Some(intent) = &self.action_intent {
            if !available_actions
                .iter()
                .any(|action| action.id == intent.id)
            {
                return Err("brain response action is outside the allow-list".into());
            }
            if !intent.intensity.is_finite() || !(0.0..=1.0).contains(&intent.intensity) {
                return Err("brain response has an invalid action intensity".into());
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CharacterEmotion {
    #[serde(rename = "type")]
    pub kind: String,
    pub intensity: f32,
    pub duration_ms: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CharacterActionIntent {
    pub id: String,
    pub intensity: f32,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CharacterSpeech {
    pub text: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BrainMemory {
    pub id: String,
    pub content: String,
    pub score: f32,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub importance: f32,
    #[serde(default)]
    pub sources: Vec<String>,
    #[serde(default)]
    pub providers: serde_json::Value,
    #[serde(default)]
    pub metadata: serde_json::Value,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryProviderStatus {
    pub id: String,
    pub ready: bool,
    pub detail: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryManagerStatus {
    pub enabled: bool,
    pub ready: bool,
    #[serde(default)]
    pub approval_required: bool,
    #[serde(default)]
    pub inference_ready: bool,
    #[serde(default)]
    pub worker_alive: bool,
    #[serde(default)]
    pub worker_error: Option<String>,
    #[serde(default)]
    pub extraction_queue: serde_json::Value,
    #[serde(default)]
    pub index_queue: serde_json::Value,
    #[serde(default)]
    pub providers: Vec<MemoryProviderStatus>,
    #[serde(default)]
    pub events: serde_json::Value,
    #[serde(default)]
    pub deliveries: serde_json::Value,
}
