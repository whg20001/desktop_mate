pub mod local_inference;
pub mod locality;
pub mod manager;
pub mod model;
pub mod policy;
pub mod provider;
pub mod providers;
pub mod session_store;

use std::sync::Arc;

use tauri::Manager;

use manager::MemoryManager;
use provider::{MemoryError, MemoryResult};
use session_store::SqliteSessionStore;

pub fn initialize(app: &tauri::App) -> MemoryResult<Arc<MemoryManager>> {
    let app_local_data_dir = app
        .path()
        .app_local_data_dir()
        .map_err(|error| MemoryError::Storage(error.to_string()))?;
    let store = SqliteSessionStore::open(app_local_data_dir.join("memory").join("memory.sqlite3"))?;
    Ok(Arc::new(MemoryManager::new(Arc::new(store))))
}
