use std::sync::Arc;

use tauri::State;

use crate::{
    error::AppResult,
    runtime::{HitRegionPayload, RuntimeState},
};

#[tauri::command]
pub fn update_hit_regions(
    payload: HitRegionPayload,
    state: State<'_, Arc<RuntimeState>>,
) -> AppResult<()> {
    state.update_hit_regions(payload)
}

#[tauri::command]
pub fn begin_drag(state: State<'_, Arc<RuntimeState>>) -> AppResult<()> {
    state.begin_drag()
}

#[tauri::command]
pub fn end_drag(state: State<'_, Arc<RuntimeState>>) {
    state.end_drag();
}
