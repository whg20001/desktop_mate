use std::time::Duration;

use reqwest::{header::HeaderValue, Client, Method};
use serde::{de::DeserializeOwned, Serialize};

use super::{
    locality::LocalEndpoint,
    model::{
        BrainMemory, CharacterResponse, ConversationPayload, ConversationScope, MemoryManagerStatus,
    },
};

pub struct BrainClient {
    endpoint: LocalEndpoint,
    client: Client,
    token: HeaderValue,
}

impl BrainClient {
    pub fn new(endpoint: LocalEndpoint, token: &str) -> Result<Self, String> {
        let token = HeaderValue::from_str(token).map_err(|error| error.to_string())?;
        let client = endpoint
            .http_client_with_timeout(Duration::from_secs(125))
            .map_err(|error| error.to_string())?;
        Ok(Self {
            endpoint,
            client,
            token,
        })
    }

    pub async fn converse(
        &self,
        payload: &ConversationPayload,
    ) -> Result<CharacterResponse, String> {
        let response: CharacterResponse = self
            .request(Method::POST, "v1/conversation", Some(payload))
            .await?;
        response.validate(&payload.available_actions)?;
        if response.turn_id != payload.turn_id {
            return Err("brain response turnId does not match the request".into());
        }
        Ok(response)
    }

    pub async fn list_memories(
        &self,
        scope: &ConversationScope,
    ) -> Result<Vec<BrainMemory>, String> {
        #[derive(serde::Deserialize)]
        struct Response {
            records: Vec<BrainMemory>,
        }
        #[derive(serde::Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Request<'a> {
            scope: &'a ConversationScope,
        }
        Ok(self
            .request::<_, Response>(Method::POST, "v1/memories/list", Some(&Request { scope }))
            .await?
            .records)
    }

    pub async fn update_memory(
        &self,
        scope: &ConversationScope,
        memory_id: &str,
        content: &str,
    ) -> Result<(), String> {
        #[derive(serde::Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Request<'a> {
            scope: &'a ConversationScope,
            content: &'a str,
        }
        let _: serde_json::Value = self
            .request(
                Method::PATCH,
                &format!("v1/memories/{}", encode_path(memory_id)),
                Some(&Request { scope, content }),
            )
            .await?;
        Ok(())
    }

    pub async fn delete_memory(
        &self,
        scope: &ConversationScope,
        memory_id: &str,
    ) -> Result<(), String> {
        #[derive(serde::Serialize)]
        struct Request<'a> {
            scope: &'a ConversationScope,
        }
        let _: serde_json::Value = self
            .request(
                Method::DELETE,
                &format!("v1/memories/{}", encode_path(memory_id)),
                Some(&Request { scope }),
            )
            .await?;
        Ok(())
    }

    pub async fn memory_status(&self) -> Result<MemoryManagerStatus, String> {
        self.request(
            Method::POST,
            "v1/memories/status",
            Some(&serde_json::json!({})),
        )
        .await
    }

    pub async fn decide_memory(
        &self,
        scope: &ConversationScope,
        memory_id: &str,
        approve: bool,
    ) -> Result<(), String> {
        #[derive(serde::Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Request<'a> {
            scope: &'a ConversationScope,
            memory_id: &'a str,
        }
        let action = if approve { "approve" } else { "reject" };
        let _: serde_json::Value = self
            .request(
                Method::POST,
                &format!("v1/memories/{action}"),
                Some(&Request { scope, memory_id }),
            )
            .await?;
        Ok(())
    }

    pub async fn rebuild_memory(
        &self,
        scope: &ConversationScope,
        provider: &str,
    ) -> Result<usize, String> {
        #[derive(serde::Serialize)]
        struct Request<'a> {
            scope: &'a ConversationScope,
            provider: &'a str,
        }
        #[derive(serde::Deserialize)]
        struct Response {
            count: usize,
        }
        Ok(self
            .request::<_, Response>(
                Method::POST,
                "v1/memories/rebuild",
                Some(&Request { scope, provider }),
            )
            .await?
            .count)
    }

    async fn request<B: Serialize + ?Sized, T: DeserializeOwned>(
        &self,
        method: Method,
        path: &str,
        body: Option<&B>,
    ) -> Result<T, String> {
        let url = self
            .endpoint
            .join(path)
            .map_err(|error| error.to_string())?;
        let mut request = self
            .client
            .request(method, url)
            .header("x-desktop-companion-token", self.token.clone());
        if let Some(body) = body {
            request = request.json(body);
        }
        let response = request
            .send()
            .await
            .map_err(|_| "local brain sidecar is unreachable".to_string())?;
        self.endpoint
            .validate_response(&response)
            .map_err(|error| error.to_string())?;
        let status = response.status();
        if !status.is_success() {
            return Err(format!("local brain sidecar returned HTTP {status}"));
        }
        response
            .json()
            .await
            .map_err(|_| "local brain sidecar returned invalid JSON".to_string())
    }
}

fn encode_path(value: &str) -> String {
    url::form_urlencoded::byte_serialize(value.as_bytes()).collect()
}
