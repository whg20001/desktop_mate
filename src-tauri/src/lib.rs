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
use tauri::{Emitter, Manager};

use commands::{
    audio::{
        cancel_audio, get_audio_models, get_audio_status, list_audio_voices, open_audio_session,
        select_audio_model, start_audio,
    },
    brain::{
        approve_brain_memory, configure_brain, converse, delete_brain_memory, get_brain_settings,
        get_brain_status, get_memory_status, list_brain_memories, rebuild_brain_memory,
        reject_brain_memory, retry_brain_memory, update_brain_memory,
    },
    character::{begin_drag, end_drag, update_hit_regions},
    desktop::get_desktop_world,
};

pub fn run() {
    tauri::Builder::default()
        .on_page_load(|webview, payload| {
            if webview.label() == "character"
                && payload.event() == tauri::webview::PageLoadEvent::Started
            {
                if let Some(audio) = webview.try_state::<Arc<speech::runtime::AudioRuntime>>() {
                    audio.invalidate();
                }
            }
        })
        .setup(|app| {
            let state = runtime::initialize(app)?;
            app.manage(state);
            let audio_handle = app.handle().clone();
            let models = speech::service::ModelService::new(&app.path().app_local_data_dir()?)?;
            let audio = speech::runtime::AudioRuntime::with_provider(
                move |event| {
                    let _ = audio_handle.emit_to("character", "audio://event", event);
                },
                models.clone(),
            )?;
            app.manage(models);
            app.manage(audio);
            let brain = brain::initialize(app)
                .unwrap_or_else(|error| brain::BrainSupervisor::unavailable(error));
            app.manage(brain);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            list_audio_voices,
            get_audio_models,
            select_audio_model,
            open_audio_session,
            start_audio,
            cancel_audio,
            get_audio_status,
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
            retry_brain_memory,
            approve_brain_memory,
            reject_brain_memory,
            rebuild_brain_memory,
        ])
        .build(tauri::generate_context!())
        .expect("desktop companion failed to build")
        .run(|app, event| {
            if let tauri::RunEvent::WindowEvent {
                label,
                event: tauri::WindowEvent::Destroyed,
                ..
            } = &event
            {
                if label == "character" {
                    if let Some(audio) = app.try_state::<Arc<speech::runtime::AudioRuntime>>() {
                        audio.invalidate();
                    }
                    // The hidden settings window must not keep audio/Brain workers alive
                    // after the owning character window is closed.
                    app.exit(0);
                }
            }
            if let tauri::RunEvent::Exit = event {
                if let Some(audio) = app.try_state::<Arc<speech::runtime::AudioRuntime>>() {
                    audio.shutdown();
                }
                if let Some(models) = app.try_state::<Arc<speech::service::ModelService>>() {
                    models.shutdown();
                }
                if let Some(brain) = app.try_state::<Arc<brain::BrainSupervisor>>() {
                    brain.shutdown();
                }
            }
        });
}
