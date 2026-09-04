use async_trait::async_trait;
use reqwest::{header::HeaderValue, Client};
use serde::{Deserialize, Serialize};

use crate::memory::{
    locality::LocalEndpoint,
    model::{MemoryEntry, MemoryId, MemoryQuery, MemoryRecord, MemoryScope, ProviderHealth},
    provider::{MemoryError, MemoryProvider, MemoryResult},
};

pub struct Mem0Provider {
    endpoint: LocalEndpoint,
    client: Client,
    session_token: HeaderValue,
}

impl Mem0Provider {
    pub fn new(endpoint: LocalEndpoint, session_token: &str) -> MemoryResult<Self> {
        if session_token.len() < 16 || session_token.len() > 256 {
            return Err(MemoryError::InvalidEndpoint(
                "Mem0 sidecar session token must contain 16 to 256 characters".into(),
            ));
        }
        let session_token = HeaderValue::from_str(session_token)
            .map_err(|error| MemoryError::InvalidEndpoint(error.to_string()))?;
        let client = endpoint.http_client()?;
        Ok(Self {
            endpoint,
            client,
            session_token,
        })
    }

    fn authenticated(&self, request: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        request.header("x-desktop-companion-token", self.session_token.clone())
    }

    async fn response_json<T: for<'de> Deserialize<'de>>(
        &self,
        response: reqwest::Response,
    ) -> MemoryResult<T> {
        self.endpoint.validate_response(&response)?;
        let status = response.status();
        if !status.is_success() {
            return Err(MemoryError::Provider(format!(
                "local Mem0 sidecar returned HTTP {status}"
            )));
        }
        response
            .json()
            .await
            .map_err(|error| MemoryError::InvalidResponse(error.to_string()))
    }

    async fn locality_health(&self) -> MemoryResult<Mem0Health> {
        let response = self
            .authenticated(self.client.get(self.endpoint.join("health")?))
            .send()
            .await
            .map_err(|error| MemoryError::Transport(error.to_string()))?;
        self.response_json(response).await
    }

    async fn require_local_dependencies(&self) -> MemoryResult<Mem0Health> {
        let health = self.locality_health().await?;
        if health.status == "ok" && health.locality.all_local() {
            Ok(health)
        } else {
            Err(MemoryError::LocalityViolation(
                "Mem0 did not attest that its LLM, embedding, vector and metadata stores are local"
                    .into(),
            ))
        }
    }
}

#[async_trait]
impl MemoryProvider for Mem0Provider {
    fn id(&self) -> &str {
        "mem0-local-sidecar"
    }

    fn endpoint(&self) -> Option<&LocalEndpoint> {
        Some(&self.endpoint)
    }

    async fn remember(&self, entry: MemoryEntry) -> MemoryResult<MemoryId> {
        self.require_local_dependencies().await?;
        let response = self
            .authenticated(self.client.post(self.endpoint.join("v1/memories")?).json(
                &RememberRequest {
                    entry,
                    infer: false,
                },
            ))
            .send()
            .await
            .map_err(|error| MemoryError::Transport(error.to_string()))?;
        let response: RememberResponse = self.response_json(response).await?;
        if response.id.trim().is_empty() {
            return Err(MemoryError::InvalidResponse(
                "Mem0 returned an empty memory id".into(),
            ));
        }
        Ok(response.id)
    }

    async fn search(&self, query: MemoryQuery) -> MemoryResult<Vec<MemoryRecord>> {
        query.scope.validate().map_err(MemoryError::Provider)?;
        self.require_local_dependencies().await?;
        let expected_scope = query.scope.clone();
        let allowed_kinds = query.kinds.clone();
        let result_limit = query.top_k.min(100);
        let response = self
            .authenticated(
                self.client
                    .post(self.endpoint.join("v1/memories/search")?)
                    .json(&query),
            )
            .send()
            .await
            .map_err(|error| MemoryError::Transport(error.to_string()))?;
        let response: SearchResponse = self.response_json(response).await?;
        let mut records: Vec<MemoryRecord> = response
            .records
            .into_iter()
            .map(|record| {
                if !scope_allows(&expected_scope, &record.entry.scope) {
                    return Err(MemoryError::InvalidResponse(
                        "Mem0 returned a record from another memory scope".into(),
                    ));
                }
                if !allowed_kinds.is_empty() && !allowed_kinds.contains(&record.entry.kind) {
                    return Err(MemoryError::InvalidResponse(
                        "Mem0 returned a memory kind excluded by the query".into(),
                    ));
                }
                if !record.score.is_finite() {
                    return Err(MemoryError::InvalidResponse(
                        "Mem0 returned a non-finite relevance score".into(),
                    ));
                }
                let stable_id = record
                    .entry
                    .metadata
                    .get("desktopCompanionMemoryId")
                    .and_then(serde_json::Value::as_str)
                    .filter(|id| !id.is_empty())
                    .ok_or_else(|| {
                        MemoryError::InvalidResponse(
                            "Mem0 result is missing its project-owned memory id".into(),
                        )
                    })?;
                Ok(MemoryRecord {
                    id: stable_id.to_string(),
                    entry: record.entry,
                    score: record.score.clamp(0.0, 1.0),
                    provider_id: self.id().into(),
                })
            })
            .collect::<MemoryResult<_>>()?;
        records.sort_by(|left, right| right.score.total_cmp(&left.score));
        records.truncate(result_limit);
        Ok(records)
    }

    async fn forget(&self, scope: &MemoryScope, provider_memory_id: &MemoryId) -> MemoryResult<()> {
        self.require_local_dependencies().await?;
        let encoded_id: String =
            url::form_urlencoded::byte_serialize(provider_memory_id.as_bytes()).collect();
        let response = self
            .authenticated(
                self.client
                    .delete(self.endpoint.join(&format!("v1/memories/{encoded_id}"))?)
                    .json(scope),
            )
            .send()
            .await
            .map_err(|error| MemoryError::Transport(error.to_string()))?;
        self.endpoint.validate_response(&response)?;
        if response.status().is_success() || response.status() == reqwest::StatusCode::NOT_FOUND {
            Ok(())
        } else {
            Err(MemoryError::Provider(format!(
                "local Mem0 delete returned HTTP {}",
                response.status()
            )))
        }
    }

    async fn clear_scope(&self, scope: &MemoryScope) -> MemoryResult<()> {
        self.require_local_dependencies().await?;
        let response = self
            .authenticated(
                self.client
                    .post(self.endpoint.join("v1/memories/clear")?)
                    .json(scope),
            )
            .send()
            .await
            .map_err(|error| MemoryError::Transport(error.to_string()))?;
        self.endpoint.validate_response(&response)?;
        if response.status().is_success() {
            Ok(())
        } else {
            Err(MemoryError::Provider(format!(
                "local Mem0 clear returned HTTP {}",
                response.status()
            )))
        }
    }

    async fn health(&self) -> MemoryResult<ProviderHealth> {
        let health = self.locality_health().await?;
        let all_local = health.locality.all_local();
        Ok(ProviderHealth {
            provider_id: self.id().into(),
            available: health.status == "ok" && all_local,
            all_local,
            version: health.version,
            detail: if all_local {
                "Mem0 sidecar and all of its dependencies report local-only operation".into()
            } else {
                "Mem0 locality attestation failed; provider is disabled".into()
            },
        })
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RememberRequest {
    entry: MemoryEntry,
    infer: bool,
}

#[derive(Deserialize)]
struct RememberResponse {
    id: MemoryId,
}

#[derive(Deserialize)]
struct SearchResponse {
    records: Vec<Mem0Record>,
}

#[derive(Deserialize)]
struct Mem0Record {
    entry: MemoryEntry,
    score: f32,
}

#[derive(Deserialize)]
struct Mem0Health {
    status: String,
    version: Option<String>,
    locality: Mem0Locality,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Mem0Locality {
    service_loopback: bool,
    llm_local: bool,
    embedding_local: bool,
    vector_store_local: bool,
    metadata_store_local: bool,
}

impl Mem0Locality {
    fn all_local(&self) -> bool {
        self.service_loopback
            && self.llm_local
            && self.embedding_local
            && self.vector_store_local
            && self.metadata_store_local
    }
}

fn scope_allows(query: &MemoryScope, record: &MemoryScope) -> bool {
    query.user_id == record.user_id
        && query.character_id == record.character_id
        && match (&query.session_id, &record.session_id) {
            (Some(query_session), Some(record_session)) => query_session == record_session,
            (None, _) | (Some(_), None) => true,
        }
}
