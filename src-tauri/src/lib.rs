pub mod ai;
pub mod brain;
mod character;
mod commands;
mod desktop;
pub mod error;
pub mod memory;
mod runtime;
pub mod speech;
pub mod vision;
mod windows;

use std::sync::Arc;
use tauri::Manager;

use commands::{
    brain::{
        configure_brain, converse, delete_brain_memory, get_brain_settings, get_brain_status,
        list_brain_memories, restart_brain, update_brain_memory,
    },
    character::{begin_drag, end_drag, update_hit_regions},
    desktop::get_desktop_world,
    memory::{
        clear_memory_scope, export_memory_scope, forget_memory, get_memory_status, list_memories,
        set_local_memory_endpoint, set_memory_privacy_mode, test_local_memory_inference,
        test_memory_provider,
    },
};

pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let state = runtime::initialize(app)?;
            app.manage(state);
            let memory = memory::initialize(app)?;
            app.manage(memory);
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
            get_memory_status,
            test_memory_provider,
            test_local_memory_inference,
            list_memories,
            forget_memory,
            clear_memory_scope,
            export_memory_scope,
            set_memory_privacy_mode,
            set_local_memory_endpoint,
            get_brain_status,
            get_brain_settings,
            configure_brain,
            restart_brain,
            converse,
            list_brain_memories,
            update_brain_memory,
            delete_brain_memory,
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
