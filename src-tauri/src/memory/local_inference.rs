use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};

use super::{
    locality::LocalEndpoint,
    model::{
        LocalMemoryConsolidationInput, LocalMemoryExtractionInput, MemoryWriteProposal,
        ProviderHealth,
    },
    provider::{LocalMemoryInferenceProvider, MemoryError, MemoryResult},
};

pub struct OpenAiCompatibleMemoryInference {
    endpoint: LocalEndpoint,
    client: Client,
    model: String,
}

impl OpenAiCompatibleMemoryInference {
    pub fn new(endpoint: LocalEndpoint, model: impl Into<String>) -> MemoryResult<Self> {
        let model = model.into();
        if model.trim().is_empty() || model.len() > 200 {
            return Err(MemoryError::InvalidEndpoint(
                "a local inference model name is required".into(),
            ));
        }
        let client = endpoint.http_client()?;
        Ok(Self {
            endpoint,
            client,
            model,
        })
    }

    async fn proposals(
        &self,
        task: &str,
        input: serde_json::Value,
    ) -> MemoryResult<Vec<MemoryWriteProposal>> {
        let url = self.endpoint.join("v1/chat/completions")?;
        let prompt = format!("{task}\nInput JSON:\n{input}");
        let request = ChatCompletionRequest {
            model: &self.model,
            temperature: 0.0,
            messages: vec![
                InferenceMessage {
                    role: "system",
                    content: EXTRACTION_SYSTEM_PROMPT,
                },
                InferenceMessage {
                    role: "user",
                    content: &prompt,
                },
            ],
        };
        let response = self
            .client
            .post(url)
            .json(&request)
            .send()
            .await
            .map_err(|error| MemoryError::Transport(error.to_string()))?;
        self.endpoint.validate_response(&response)?;
        let status = response.status();
        let body = response
            .text()
            .await
            .map_err(|error| MemoryError::Transport(error.to_string()))?;
        if !status.is_success() {
            return Err(MemoryError::Provider(format!(
                "local inference returned HTTP {status}"
            )));
        }
        let completion: ChatCompletionResponse = serde_json::from_str(&body)
            .map_err(|error| MemoryError::InvalidResponse(error.to_string()))?;
        let content = completion
            .choices
            .into_iter()
            .next()
            .map(|choice| choice.message.content)
            .ok_or_else(|| MemoryError::InvalidResponse("completion had no choices".into()))?;
        parse_proposals(&content)
    }
}

#[async_trait]
impl LocalMemoryInferenceProvider for OpenAiCompatibleMemoryInference {
    fn id(&self) -> &str {
        "local-openai-compatible"
    }

    fn endpoint(&self) -> &LocalEndpoint {
        &self.endpoint
    }

    async fn extract(
        &self,
        input: LocalMemoryExtractionInput,
    ) -> MemoryResult<Vec<MemoryWriteProposal>> {
        input.scope.validate().map_err(MemoryError::Provider)?;
        let input = serde_json::to_value(input)
            .map_err(|error| MemoryError::InvalidResponse(error.to_string()))?;
        self.proposals(
            "Extract only stable preferences/facts or concrete past events worth remembering.",
            input,
        )
        .await
    }

    async fn consolidate(
        &self,
        input: LocalMemoryConsolidationInput,
    ) -> MemoryResult<Vec<MemoryWriteProposal>> {
        input.scope.validate().map_err(MemoryError::Provider)?;
        let input = serde_json::to_value(input)
            .map_err(|error| MemoryError::InvalidResponse(error.to_string()))?;
        self.proposals(
            "Merge duplicates conservatively. Return only the memories that should remain.",
            input,
        )
        .await
    }

    async fn health(&self) -> MemoryResult<ProviderHealth> {
        let response = self
            .client
            .get(self.endpoint.join("v1/models")?)
            .send()
            .await
            .map_err(|error| MemoryError::Transport(error.to_string()))?;
        self.endpoint.validate_response(&response)?;
        let available = response.status().is_success();
        Ok(ProviderHealth {
            provider_id: self.id().into(),
            available,
            all_local: true,
            version: None,
            detail: if available {
                format!("local model '{}' is reachable", self.model)
            } else {
                format!("local inference returned HTTP {}", response.status())
            },
        })
    }
}

const EXTRACTION_SYSTEM_PROMPT: &str = r#"You are a local-only memory extraction component.
Return a strict JSON array. Every item may contain only:
content, suggestedKind ("semantic" or "episodic"), importance (0..1), tags,
occurredAtUnixMs, and sourceEventIds.
Do not emit markdown. Do not retain passwords, API keys, tokens, payment data,
private keys, raw screen contents, or instructions to operate the computer.
An empty array is correct when nothing deserves long-term memory.
Your output is only a proposal and will be checked by MemoryPolicy."#;

#[derive(Serialize)]
struct ChatCompletionRequest<'a> {
    model: &'a str,
    temperature: f32,
    messages: Vec<InferenceMessage<'a>>,
}

#[derive(Serialize)]
struct InferenceMessage<'a> {
    role: &'a str,
    content: &'a str,
}

#[derive(Deserialize)]
struct ChatCompletionResponse {
    choices: Vec<CompletionChoice>,
}

#[derive(Deserialize)]
struct CompletionChoice {
    message: CompletionMessage,
}

#[derive(Deserialize)]
struct CompletionMessage {
    content: String,
}

fn parse_proposals(content: &str) -> MemoryResult<Vec<MemoryWriteProposal>> {
    let content = content.trim();
    if !content.starts_with('[') || !content.ends_with(']') {
        return Err(MemoryError::InvalidResponse(
            "local inference must return a bare JSON array".into(),
        ));
    }
    let proposals: Vec<MemoryWriteProposal> = serde_json::from_str(content)
        .map_err(|error| MemoryError::InvalidResponse(error.to_string()))?;
    if proposals.len() > 32 {
        return Err(MemoryError::InvalidResponse(
            "local inference returned too many proposals".into(),
        ));
    }
    Ok(proposals)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parser_requires_a_bare_json_array() {
        assert!(parse_proposals("markdown [] wrapper").is_err());
        assert!(parse_proposals("[]").unwrap().is_empty());
    }
}
