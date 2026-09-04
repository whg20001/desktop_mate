use std::sync::Arc;

use tauri::State;
use uuid::Uuid;

use crate::brain::{
    model::{
        BrainMemory, BrainSettings, BrainStatus, CharacterResponse, ConversationPayload,
        ConversationRequest, ConversationScope,
    },
    BrainSupervisor,
};

type CommandResult<T> = Result<T, String>;

#[tauri::command]
pub fn get_brain_status(state: State<'_, Arc<BrainSupervisor>>) -> BrainStatus {
    state.status()
}

#[tauri::command]
pub fn get_brain_settings(state: State<'_, Arc<BrainSupervisor>>) -> BrainSettings {
    state.settings()
}

#[tauri::command]
pub fn configure_brain(
    settings: BrainSettings,
    state: State<'_, Arc<BrainSupervisor>>,
) -> CommandResult<BrainSettings> {
    state.configure(settings)
}

#[tauri::command]
pub async fn converse(
    request: ConversationRequest,
    state: State<'_, Arc<BrainSupervisor>>,
) -> CommandResult<CharacterResponse> {
    request.scope.validate()?;
    let user_input = request.user_input.trim();
    if user_input.is_empty() || user_input.len() > 16_000 {
        return Err("userInput must contain 1 to 16000 characters".into());
    }
    if request.available_actions.len() > 32 {
        return Err("availableActions cannot contain more than 32 entries".into());
    }
    let turn_id = request
        .turn_id
        .unwrap_or_else(|| Uuid::new_v4().to_string());
    if turn_id.trim().is_empty() || turn_id.len() > 128 || turn_id.chars().any(char::is_control) {
        return Err("turnId must contain 1 to 128 printable characters".into());
    }
    let payload = ConversationPayload {
        turn_id,
        scope: request.scope,
        user_input: user_input.to_string(),
        available_actions: request.available_actions,
        desktop_context: request.desktop_context,
    };
    state.ready_client()?.converse(&payload).await
}

#[tauri::command]
pub async fn list_brain_memories(
    scope: ConversationScope,
    state: State<'_, Arc<BrainSupervisor>>,
) -> CommandResult<Vec<BrainMemory>> {
    scope.validate()?;
    state.ready_client()?.list_memories(&scope).await
}

#[tauri::command]
pub async fn update_brain_memory(
    scope: ConversationScope,
    memory_id: String,
    content: String,
    state: State<'_, Arc<BrainSupervisor>>,
) -> CommandResult<()> {
    scope.validate()?;
    validate_memory_id(&memory_id)?;
    let content = content.trim();
    if content.is_empty() || content.len() > 2_000 {
        return Err("memory content must contain 1 to 2000 characters".into());
    }
    state
        .ready_client()?
        .update_memory(&scope, &memory_id, content)
        .await
}

#[tauri::command]
pub async fn delete_brain_memory(
    scope: ConversationScope,
    memory_id: String,
    state: State<'_, Arc<BrainSupervisor>>,
) -> CommandResult<()> {
    scope.validate()?;
    validate_memory_id(&memory_id)?;
    state
        .ready_client()?
        .delete_memory(&scope, &memory_id)
        .await
}

fn validate_memory_id(memory_id: &str) -> CommandResult<()> {
    if memory_id.trim().is_empty()
        || memory_id.len() > 256
        || memory_id.chars().any(char::is_control)
    {
        Err("memoryId is invalid".into())
    } else {
        Ok(())
    }
}
