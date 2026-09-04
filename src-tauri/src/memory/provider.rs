use async_trait::async_trait;
use thiserror::Error;

use crate::ai::provider::ChatMessage;

use super::{
    locality::LocalEndpoint,
    model::{
        ApprovedMemoryEvent, LocalMemoryConsolidationInput, LocalMemoryExtractionInput,
        MemoryEntry, MemoryExport, MemoryId, MemoryListFilter, MemoryQuery, MemoryRecord,
        MemoryScope, MemoryWriteProposal, ProviderHealth, TemporalQuery, TemporalRelation,
    },
};

#[derive(Debug, Error)]
pub enum MemoryError {
    #[error("memory provider is not configured")]
    NotConfigured,
    #[error("memory is disabled by the privacy setting")]
    Disabled,
    #[error("invalid local endpoint: {0}")]
    InvalidEndpoint(String),
    #[error("memory locality boundary rejected the operation: {0}")]
    LocalityViolation(String),
    #[error("memory storage failed: {0}")]
    Storage(String),
    #[error("memory transport failed: {0}")]
    Transport(String),
    #[error("memory response was invalid: {0}")]
    InvalidResponse(String),
    #[error("memory policy rejected the proposal: {0}")]
    PolicyRejected(String),
    #[error("memory provider operation failed: {0}")]
    Provider(String),
}

pub type MemoryResult<T> = Result<T, MemoryError>;

#[async_trait]
pub trait SessionStore: Send + Sync {
    async fn append_turn(
        &self,
        scope: &MemoryScope,
        messages: Vec<ChatMessage>,
    ) -> MemoryResult<()>;
    async fn get_recent(&self, scope: &MemoryScope, limit: usize)
        -> MemoryResult<Vec<ChatMessage>>;
    async fn clear_scope(&self, scope: &MemoryScope) -> MemoryResult<()>;
    async fn export_scope(&self, scope: &MemoryScope) -> MemoryResult<MemoryExport>;
    async fn save_approved(&self, event: ApprovedMemoryEvent) -> MemoryResult<()>;
    async fn list_approved(
        &self,
        scope: &MemoryScope,
        filter: &MemoryListFilter,
    ) -> MemoryResult<Vec<ApprovedMemoryEvent>>;
    async fn remove_approved(&self, scope: &MemoryScope, id: &MemoryId) -> MemoryResult<bool>;
    async fn save_provider_mapping(
        &self,
        local_id: &MemoryId,
        provider_id: &str,
        provider_memory_id: &MemoryId,
    ) -> MemoryResult<()>;
    async fn provider_mapping(
        &self,
        local_id: &MemoryId,
        provider_id: &str,
    ) -> MemoryResult<Option<MemoryId>>;
}

#[async_trait]
pub trait MemoryProvider: Send + Sync {
    fn id(&self) -> &str;
    fn endpoint(&self) -> Option<&LocalEndpoint>;
    async fn remember(&self, entry: MemoryEntry) -> MemoryResult<MemoryId>;
    async fn search(&self, query: MemoryQuery) -> MemoryResult<Vec<MemoryRecord>>;
    async fn forget(&self, scope: &MemoryScope, id: &MemoryId) -> MemoryResult<()>;
    async fn clear_scope(&self, scope: &MemoryScope) -> MemoryResult<()>;
    async fn health(&self) -> MemoryResult<ProviderHealth>;
}

#[async_trait]
pub trait LocalMemoryInferenceProvider: Send + Sync {
    fn id(&self) -> &str;
    fn endpoint(&self) -> &LocalEndpoint;
    async fn extract(
        &self,
        input: LocalMemoryExtractionInput,
    ) -> MemoryResult<Vec<MemoryWriteProposal>>;
    async fn consolidate(
        &self,
        input: LocalMemoryConsolidationInput,
    ) -> MemoryResult<Vec<MemoryWriteProposal>>;
    async fn health(&self) -> MemoryResult<ProviderHealth>;
}

#[async_trait]
pub trait TemporalGraphProvider: Send + Sync {
    fn id(&self) -> &str;
    fn endpoint(&self) -> Option<&LocalEndpoint>;
    async fn index_event(&self, event: ApprovedMemoryEvent) -> MemoryResult<()>;
    async fn search_relations(&self, query: TemporalQuery) -> MemoryResult<Vec<TemporalRelation>>;
    async fn clear_scope(&self, scope: &MemoryScope) -> MemoryResult<()>;
}
