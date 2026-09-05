pub mod brain;
mod character;
mod commands;
mod desktop;
pub mod error;
mod runtime;
pub mod speech;
pub mod vision;
mod windows;

use std::sync::Arc;
use tauri::Manager;

use commands::{
    brain::{
        approve_brain_memory, configure_brain, converse, delete_brain_memory, get_brain_settings,
        get_brain_status, get_memory_status, list_brain_memories, rebuild_brain_memory,
        reject_brain_memory, update_brain_memory,
    },
    character::{begin_drag, end_drag, update_hit_regions},
    desktop::get_desktop_world,
};

pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let state = runtime::initialize(app)?;
            app.manage(state);
            let brain = brain::initialize(app)
                .unwrap_or_else(|error| brain::BrainSupervisor::unavailable(error));
            app.manage(brain);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            begin_drag,
            end_drag,
            update_hit_regions,
            get_desktop_world,
            get_brain_status,
            get_brain_settings,
            configure_brain,
            converse,
            list_brain_memories,
            update_brain_memory,
            delete_brain_memory,
            get_memory_status,
            approve_brain_memory,
            reject_brain_memory,
            rebuild_brain_memory,
        ])
        .build(tauri::generate_context!())
        .expect("desktop companion failed to build")
        .run(|app, event| {
            if let tauri::RunEvent::Exit = event {
                if let Some(brain) = app.try_state::<Arc<brain::BrainSupervisor>>() {
                    brain.shutdown();
                }
            }
        });
}
