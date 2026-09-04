use std::sync::Arc;

use tauri::State;

use crate::memory::{
    local_inference::OpenAiCompatibleMemoryInference,
    locality::LocalEndpoint,
    manager::MemoryManager,
    model::{
        ApprovedMemoryEvent, LocalMemoryEndpointConfig, LocalMemoryService, MemoryExport,
        MemoryListFilter, MemoryMutationReport, MemoryPrivacyMode, MemoryScope, MemoryStatus,
        ProviderHealth,
    },
    provider::MemoryError,
    providers::mem0::Mem0Provider,
};

type CommandResult<T> = Result<T, String>;
const MEM0_SESSION_TOKEN_ENV: &str = "DESKTOP_COMPANION_MEM0_SESSION_TOKEN";

#[tauri::command]
pub async fn get_memory_status(
    state: State<'_, Arc<MemoryManager>>,
) -> CommandResult<MemoryStatus> {
    Ok(state.status().await)
}

#[tauri::command]
pub async fn test_memory_provider(
    state: State<'_, Arc<MemoryManager>>,
) -> CommandResult<ProviderHealth> {
    Ok(state.test_memory_provider().await)
}

#[tauri::command]
pub async fn test_local_memory_inference(
    state: State<'_, Arc<MemoryManager>>,
) -> CommandResult<ProviderHealth> {
    Ok(state.test_inference_provider().await)
}

#[tauri::command]
pub async fn list_memories(
    scope: MemoryScope,
    filter: Option<MemoryListFilter>,
    state: State<'_, Arc<MemoryManager>>,
) -> CommandResult<Vec<ApprovedMemoryEvent>> {
    state
        .list(&scope, &filter.unwrap_or_default())
        .await
        .map_err(command_error)
}

#[tauri::command]
pub async fn forget_memory(
    scope: MemoryScope,
    memory_id: String,
    state: State<'_, Arc<MemoryManager>>,
) -> CommandResult<MemoryMutationReport> {
    state
        .forget(&scope, &memory_id)
        .await
        .map_err(command_error)
}

#[tauri::command]
pub async fn clear_memory_scope(
    scope: MemoryScope,
    state: State<'_, Arc<MemoryManager>>,
) -> CommandResult<MemoryMutationReport> {
    state.clear_scope(&scope).await.map_err(command_error)
}

#[tauri::command]
pub async fn export_memory_scope(
    scope: MemoryScope,
    state: State<'_, Arc<MemoryManager>>,
) -> CommandResult<MemoryExport> {
    state.export_scope(&scope).await.map_err(command_error)
}

#[tauri::command]
pub fn set_memory_privacy_mode(mode: MemoryPrivacyMode, state: State<'_, Arc<MemoryManager>>) {
    state.set_privacy_mode(mode);
}

#[tauri::command]
pub fn set_local_memory_endpoint(
    config: LocalMemoryEndpointConfig,
    state: State<'_, Arc<MemoryManager>>,
) -> CommandResult<()> {
    let endpoint = LocalEndpoint::parse(&config.endpoint).map_err(command_error)?;
    match config.service {
        LocalMemoryService::Mem0 => {
            let session_token = std::env::var(MEM0_SESSION_TOKEN_ENV).map_err(|_| {
                format!(
                    "{MEM0_SESSION_TOKEN_ENV} is required in debug mode; the sidecar launcher +                     must inject this random credential without exposing it to the WebView"
                )
            })?;
            let provider = Mem0Provider::new(endpoint, &session_token).map_err(command_error)?;
            state.set_provider(Some(Arc::new(provider)));
        }
        LocalMemoryService::Inference => {
            let model = config
                .model
                .ok_or_else(|| "a local inference model name is required".to_string())?;
            let provider =
                OpenAiCompatibleMemoryInference::new(endpoint, model).map_err(command_error)?;
            state.set_inference_provider(Some(Arc::new(provider)));
        }
    }
    Ok(())
}

fn command_error(error: MemoryError) -> String {
    error.to_string()
}
