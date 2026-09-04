use std::{collections::HashSet, sync::Arc};

use parking_lot::RwLock;
use uuid::Uuid;

use crate::ai::provider::ChatMessage;

use super::{
    model::{
        ApprovedMemoryEvent, LocalMemoryExtractionInput, MemoryCommitReport, MemoryContext,
        MemoryContextBudget, MemoryExport, MemoryKind, MemoryListFilter, MemoryMutationReport,
        MemoryPrivacyMode, MemoryQuery, MemoryRecord, MemoryScope, MemoryStatus,
        MemoryWriteProposal, ProviderHealth,
    },
    policy::MemoryPolicy,
    provider::{
        LocalMemoryInferenceProvider, MemoryError, MemoryProvider, MemoryResult, SessionStore,
    },
};

pub struct MemoryManager {
    store: Arc<dyn SessionStore>,
    policy: MemoryPolicy,
    provider: RwLock<Option<Arc<dyn MemoryProvider>>>,
    inference: RwLock<Option<Arc<dyn LocalMemoryInferenceProvider>>>,
    privacy_mode: RwLock<MemoryPrivacyMode>,
}

impl MemoryManager {
    pub fn new(store: Arc<dyn SessionStore>) -> Self {
        Self {
            store,
            policy: MemoryPolicy::default(),
            provider: RwLock::new(None),
            inference: RwLock::new(None),
            privacy_mode: RwLock::new(MemoryPrivacyMode::LocalOnly),
        }
    }

    pub fn set_provider(&self, provider: Option<Arc<dyn MemoryProvider>>) {
        *self.provider.write() = provider;
    }

    pub fn set_inference_provider(&self, provider: Option<Arc<dyn LocalMemoryInferenceProvider>>) {
        *self.inference.write() = provider;
    }

    pub fn set_privacy_mode(&self, mode: MemoryPrivacyMode) {
        *self.privacy_mode.write() = mode;
    }

    pub async fn append_turn(
        &self,
        scope: &MemoryScope,
        messages: Vec<ChatMessage>,
    ) -> MemoryResult<()> {
        self.require_enabled()?;
        self.store.append_turn(scope, messages).await
    }

    pub async fn build_context(
        &self,
        scope: &MemoryScope,
        query_text: impl Into<String>,
        budget: MemoryContextBudget,
    ) -> MemoryResult<MemoryContext> {
        self.require_enabled()?;
        scope.validate().map_err(MemoryError::Provider)?;
        let mut context = MemoryContext {
            recent_messages: self
                .store
                .get_recent(scope, budget.recent_message_limit.min(500))
                .await?,
            ..MemoryContext::default()
        };

        let provider = { self.provider.read().clone() };
        if let Some(provider) = provider {
            let query = MemoryQuery {
                scope: scope.clone(),
                text: query_text.into(),
                kinds: vec![MemoryKind::Semantic, MemoryKind::Episodic],
                top_k: budget.long_term_record_limit.min(100),
                time_range: None,
            };
            match provider.search(query).await {
                Ok(records) => {
                    let records = deduplicate_records(records);
                    for record in records {
                        match record.entry.kind {
                            MemoryKind::Semantic => context.semantic.push(record),
                            MemoryKind::Episodic => context.episodic.push(record),
                        }
                    }
                }
                Err(error) => context
                    .degraded_reasons
                    .push(format!("long-term memory unavailable: {error}")),
            }
        } else {
            context
                .degraded_reasons
                .push("long-term memory provider is not configured".into());
        }

        trim_context(&mut context, budget.max_characters.max(256));
        Ok(context)
    }

    pub async fn extract(
        &self,
        input: LocalMemoryExtractionInput,
    ) -> MemoryResult<Vec<MemoryWriteProposal>> {
        self.require_enabled()?;
        let provider = self
            .inference
            .read()
            .clone()
            .ok_or(MemoryError::NotConfigured)?;
        provider.extract(input).await
    }

    pub async fn commit(
        &self,
        scope: &MemoryScope,
        proposal: MemoryWriteProposal,
    ) -> MemoryResult<MemoryCommitReport> {
        self.require_enabled()?;
        let mut entry = self.policy.approve(scope, proposal)?;
        let existing = self
            .store
            .list_approved(
                scope,
                &MemoryListFilter {
                    kinds: vec![entry.kind],
                    limit: 500,
                },
            )
            .await?;
        if let Some(existing) = existing
            .into_iter()
            .find(|event| event.entry.content.eq_ignore_ascii_case(&entry.content))
        {
            return Ok(MemoryCommitReport {
                memory_id: existing.id,
                pending_provider_sync: false,
                warnings: vec!["an equivalent approved memory already exists".into()],
            });
        }

        let local_id = Uuid::new_v4().to_string();
        entry.metadata["desktopCompanionMemoryId"] = serde_json::Value::String(local_id.clone());
        let event = ApprovedMemoryEvent {
            id: local_id.clone(),
            entry: entry.clone(),
        };
        self.store.save_approved(event).await?;

        let provider = { self.provider.read().clone() };
        let Some(provider) = provider else {
            return Ok(MemoryCommitReport {
                memory_id: local_id,
                pending_provider_sync: true,
                warnings: vec![
                    "Mem0 is not configured; the local fact source retained the memory".into(),
                ],
            });
        };

        match provider.remember(entry).await {
            Ok(provider_memory_id) => {
                let mut warnings = Vec::new();
                if let Err(error) = self
                    .store
                    .save_provider_mapping(&local_id, provider.id(), &provider_memory_id)
                    .await
                {
                    warnings.push(format!("provider ID mapping could not be saved: {error}"));
                }
                Ok(MemoryCommitReport {
                    memory_id: local_id,
                    pending_provider_sync: !warnings.is_empty(),
                    warnings,
                })
            }
            Err(error) => Ok(MemoryCommitReport {
                memory_id: local_id,
                pending_provider_sync: true,
                warnings: vec![format!(
                    "Mem0 write failed; the local fact source retained the memory: {error}"
                )],
            }),
        }
    }

    pub async fn list(
        &self,
        scope: &MemoryScope,
        filter: &MemoryListFilter,
    ) -> MemoryResult<Vec<ApprovedMemoryEvent>> {
        self.store.list_approved(scope, filter).await
    }

    pub async fn forget(
        &self,
        scope: &MemoryScope,
        memory_id: &str,
    ) -> MemoryResult<MemoryMutationReport> {
        let local_memory_id = memory_id.to_string();
        let provider = self.provider.read().clone();
        let mut warnings = Vec::new();
        let mut provider_updated = false;
        if let Some(provider) = provider {
            match self
                .store
                .provider_mapping(&local_memory_id, provider.id())
                .await
            {
                Ok(Some(provider_memory_id)) => {
                    match provider.forget(scope, &provider_memory_id).await {
                        Ok(()) => provider_updated = true,
                        Err(error) => warnings.push(format!("provider delete failed: {error}")),
                    }
                }
                Ok(None) => {}
                Err(error) => warnings.push(format!("provider mapping lookup failed: {error}")),
            }
        }
        let local_source_updated = self.store.remove_approved(scope, &local_memory_id).await?;
        Ok(MemoryMutationReport {
            local_source_updated,
            provider_updated,
            warnings,
        })
    }

    pub async fn clear_scope(&self, scope: &MemoryScope) -> MemoryResult<MemoryMutationReport> {
        let mut warnings = Vec::new();
        let mut provider_updated = false;
        let provider = { self.provider.read().clone() };
        if let Some(provider) = provider {
            match provider.clear_scope(scope).await {
                Ok(()) => provider_updated = true,
                Err(error) => warnings.push(format!("provider clear failed: {error}")),
            }
        }
        self.store.clear_scope(scope).await?;
        Ok(MemoryMutationReport {
            local_source_updated: true,
            provider_updated,
            warnings,
        })
    }

    pub async fn export_scope(&self, scope: &MemoryScope) -> MemoryResult<MemoryExport> {
        self.store.export_scope(scope).await
    }

    pub async fn status(&self) -> MemoryStatus {
        let provider = { self.provider.read().clone() };
        let inference = { self.inference.read().clone() };
        let memory_provider = health_or_unavailable(provider, "mem0-local-sidecar").await;
        let inference_provider =
            inference_health_or_unavailable(inference, "local-openai-compatible").await;
        MemoryStatus {
            privacy_mode: *self.privacy_mode.read(),
            memory_provider,
            inference_provider,
            temporal_graph_phase: "phaseII-not-configured".into(),
        }
    }

    pub async fn test_memory_provider(&self) -> ProviderHealth {
        let provider = { self.provider.read().clone() };
        health_or_unavailable(provider, "mem0-local-sidecar").await
    }

    pub async fn test_inference_provider(&self) -> ProviderHealth {
        let inference = { self.inference.read().clone() };
        inference_health_or_unavailable(inference, "local-openai-compatible").await
    }

    fn require_enabled(&self) -> MemoryResult<()> {
        if *self.privacy_mode.read() == MemoryPrivacyMode::Disabled {
            Err(MemoryError::Disabled)
        } else {
            Ok(())
        }
    }
}

fn deduplicate_records(mut records: Vec<MemoryRecord>) -> Vec<MemoryRecord> {
    records.sort_by(|left, right| right.score.total_cmp(&left.score));
    let mut seen = HashSet::new();
    records.retain(|record| seen.insert(record.entry.content.trim().to_lowercase()));
    records
}

fn trim_context(context: &mut MemoryContext, max_characters: usize) {
    while context_characters(context) > max_characters {
        if context.episodic.pop().is_some() {
            continue;
        }
        if context.semantic.pop().is_some() {
            continue;
        }
        if !context.recent_messages.is_empty() {
            context.recent_messages.remove(0);
            continue;
        }
        break;
    }
}

fn context_characters(context: &MemoryContext) -> usize {
    context
        .recent_messages
        .iter()
        .map(|message| message.content.chars().count())
        .chain(
            context
                .semantic
                .iter()
                .map(|record| record.entry.content.chars().count()),
        )
        .chain(
            context
                .episodic
                .iter()
                .map(|record| record.entry.content.chars().count()),
        )
        .sum()
}

async fn health_or_unavailable(
    provider: Option<Arc<dyn MemoryProvider>>,
    id: &str,
) -> ProviderHealth {
    match provider {
        Some(provider) => provider
            .health()
            .await
            .unwrap_or_else(|error| ProviderHealth::unavailable(id, error.to_string())),
        None => ProviderHealth::unavailable(id, "not configured"),
    }
}

async fn inference_health_or_unavailable(
    provider: Option<Arc<dyn LocalMemoryInferenceProvider>>,
    id: &str,
) -> ProviderHealth {
    match provider {
        Some(provider) => provider
            .health()
            .await
            .unwrap_or_else(|error| ProviderHealth::unavailable(id, error.to_string())),
        None => ProviderHealth::unavailable(id, "not configured"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::session_store::SqliteSessionStore;

    fn manager() -> MemoryManager {
        let path = std::env::temp_dir().join(format!(
            "desktop-companion-manager-{}.sqlite3",
            Uuid::new_v4()
        ));
        MemoryManager::new(Arc::new(SqliteSessionStore::open(path).unwrap()))
    }

    fn scope() -> MemoryScope {
        MemoryScope {
            user_id: "user".into(),
            character_id: "character".into(),
            session_id: Some("session".into()),
        }
    }

    #[test]
    fn missing_mem0_degrades_to_local_fact_source() {
        tauri::async_runtime::block_on(async {
            let manager = manager();
            let report = manager
                .commit(
                    &scope(),
                    MemoryWriteProposal {
                        content: "The user prefers dark themes".into(),
                        suggested_kind: Some(MemoryKind::Semantic),
                        importance: Some(0.8),
                        tags: vec!["preference".into()],
                        occurred_at_unix_ms: None,
                        source_event_ids: vec!["event-1".into()],
                    },
                )
                .await
                .unwrap();
            assert!(report.pending_provider_sync);
            let stored = manager
                .list(&scope(), &MemoryListFilter::default())
                .await
                .unwrap();
            assert_eq!(stored.len(), 1);
            assert_eq!(stored[0].id, report.memory_id);
        });
    }

    #[test]
    fn missing_mem0_does_not_hide_session_memory() {
        tauri::async_runtime::block_on(async {
            let manager = manager();
            manager
                .append_turn(
                    &scope(),
                    vec![
                        ChatMessage::new("user", "hello"),
                        ChatMessage::new("assistant", "welcome"),
                    ],
                )
                .await
                .unwrap();
            let context = manager
                .build_context(&scope(), "hello", MemoryContextBudget::default())
                .await
                .unwrap();
            assert_eq!(context.recent_messages.len(), 2);
            assert!(!context.degraded_reasons.is_empty());
        });
    }
}
