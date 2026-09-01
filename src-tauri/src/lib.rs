pub mod ai;
mod character;
mod commands;
mod desktop;
pub mod error;
pub mod memory;
mod runtime;
pub mod speech;
pub mod vision;
mod windows;

use tauri::Manager;

use commands::{
    character::{begin_drag, end_drag, update_hit_regions},
    desktop::get_desktop_world,
};

pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let state = runtime::initialize(app)?;
            app.manage(state);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            begin_drag,
            end_drag,
            update_hit_regions,
            get_desktop_world,
        ])
        .run(tauri::generate_context!())
        .expect("desktop companion failed to run");
}
