pub mod client;
pub mod model;
pub mod supervisor;

use std::sync::Arc;

pub use supervisor::BrainSupervisor;

pub fn initialize(app: &tauri::App) -> Result<Arc<BrainSupervisor>, String> {
    BrainSupervisor::initialize(app)
}
