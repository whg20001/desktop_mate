use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::memory::model::MemoryContext;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

impl ChatMessage {
    pub fn new(role: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            role: role.into(),
            content: content.into(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AvailableAction {
    pub id: String,
    pub description: String,
    #[serde(default)]
    pub scenes: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentRequest {
    pub system_prompt: String,
    pub memory_context: Option<MemoryContext>,
    pub desktop_context: Option<serde_json::Value>,
    pub vision_context: Option<String>,
    #[serde(default)]
    pub available_actions: Vec<AvailableAction>,
    pub user_input: String,
}

impl AgentRequest {
    pub fn validate(&self) -> Result<(), AgentError> {
        if self.memory_context.is_none() {
            Err(AgentError::SecurityBoundary(
                "local memory-aware requests require a MemoryContext".into(),
            ))
        } else {
            Ok(())
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EmotionProposal {
    pub kind: String,
    pub intensity: Option<f32>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BehaviorProposal {
    pub action_id: String,
    pub intensity: Option<f32>,
    pub reason: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentResponse {
    pub speech: String,
    pub emotion: Option<EmotionProposal>,
    pub behavior: Option<BehaviorProposal>,
}

impl AgentResponse {
    pub fn validate_against(&self, request: &AgentRequest) -> Result<(), AgentError> {
        if let Some(behavior) = &self.behavior {
            if !request
                .available_actions
                .iter()
                .any(|action| action.id == behavior.action_id)
            {
                return Err(AgentError::SecurityBoundary(format!(
                    "action '{}' is not in the current MotionCatalog allow-list",
                    behavior.action_id
                )));
            }
            if behavior
                .intensity
                .is_some_and(|value| !value.is_finite() || !(0.0..=1.0).contains(&value))
            {
                return Err(AgentError::SecurityBoundary(
                    "behavior intensity must be between zero and one".into(),
                ));
            }
        }
        Ok(())
    }
}

#[derive(Debug, Error)]
pub enum AgentError {
    #[error("AI provider is not configured")]
    NotConfigured,
    #[error("AI provider request failed: {0}")]
    Provider(String),
    #[error("AI privacy boundary rejected the request: {0}")]
    SecurityBoundary(String),
}

#[async_trait]
pub trait AgentProvider: Send + Sync {
    fn id(&self) -> &str;
    fn supports_vision(&self) -> bool {
        false
    }
    async fn chat(&self, request: AgentRequest) -> Result<AgentResponse, AgentError>;
    async fn chat_stream(
        &self,
        request: AgentRequest,
        on_delta: Box<dyn Fn(String) + Send>,
    ) -> Result<AgentResponse, AgentError> {
        let response = self.chat(request).await?;
        on_delta(response.speech.clone());
        Ok(response)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_request_requires_memory_context() {
        let mut request = AgentRequest {
            system_prompt: "system".into(),
            memory_context: None,
            desktop_context: None,
            vision_context: None,
            available_actions: Vec::new(),
            user_input: "hello".into(),
        };
        assert!(request.validate().is_err());

        request.memory_context = Some(MemoryContext::default());
        assert!(request.validate().is_ok());
    }

    #[test]
    fn behavior_must_use_the_current_action_allow_list() {
        let request = AgentRequest {
            system_prompt: "system".into(),
            memory_context: None,
            desktop_context: None,
            vision_context: None,
            available_actions: vec![AvailableAction {
                id: "greeting".into(),
                description: "Wave to the user".into(),
                scenes: vec!["user-click".into()],
            }],
            user_input: "hello".into(),
        };
        let response = AgentResponse {
            speech: "hello".into(),
            emotion: None,
            behavior: Some(BehaviorProposal {
                action_id: "raw-bone-write".into(),
                intensity: Some(1.0),
                reason: None,
            }),
        };
        assert!(response.validate_against(&request).is_err());
    }
}
