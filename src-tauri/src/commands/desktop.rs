use std::sync::Arc;

use tauri::State;

use crate::{desktop::world::DesktopWorld, runtime::RuntimeState};

#[tauri::command]
pub fn get_desktop_world(state: State<'_, Arc<RuntimeState>>) -> DesktopWorld {
    state.desktop.read().clone()
}
