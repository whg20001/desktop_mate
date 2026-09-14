use crate::speech::{
    model::{AudioError, AudioStart, AudioStatus, SessionKey, VoiceInfo},
    runtime::AudioRuntime,
    service::{ModelService, ModelStatus},
};
use std::sync::Arc;
use tauri::{State, WebviewWindow};

fn allow(label: &str, playback: bool) -> Result<(), AudioError> {
    if label == "character" || (!playback && label == "settings") {
        Ok(())
    } else {
        Err(AudioError::new("forbidden", "此窗口不能访问音频服务"))
    }
}
#[tauri::command]
pub async fn list_audio_voices(
    window: WebviewWindow,
    models: State<'_, Arc<ModelService>>,
) -> Result<Vec<VoiceInfo>, AudioError> {
    allow(window.label(), false)?;
    let models = models.inner().clone();
    tauri::async_runtime::spawn_blocking(move || models.voices())
        .await
        .map_err(|_| AudioError::new("synthesis_unavailable", "无法读取音色列表"))?
}
#[tauri::command]
pub fn open_audio_session(
    window: WebviewWindow,
    state: State<'_, Arc<AudioRuntime>>,
) -> Result<u64, AudioError> {
    allow(window.label(), true)?;
    state.open()
}
#[tauri::command]
pub fn start_audio(
    window: WebviewWindow,
    state: State<'_, Arc<AudioRuntime>>,
    request: AudioStart,
) -> Result<(), AudioError> {
    allow(window.label(), true)?;
    state.start(request)
}
#[tauri::command]
pub fn cancel_audio(
    window: WebviewWindow,
    state: State<'_, Arc<AudioRuntime>>,
    key: SessionKey,
) -> Result<bool, AudioError> {
    allow(window.label(), true)?;
    state.cancel(&key)
}
#[tauri::command]
pub fn get_audio_status(
    window: WebviewWindow,
    state: State<'_, Arc<AudioRuntime>>,
) -> Result<AudioStatus, AudioError> {
    allow(window.label(), false)?;
    Ok(state.status())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_character_can_start_playback() {
        assert!(allow("character", true).is_ok());
        assert!(allow("settings", true).is_err());
        assert!(allow("settings", false).is_ok());
        assert!(allow("other", false).is_err());
    }
}

#[tauri::command]
pub async fn get_audio_models(
    window: WebviewWindow,
    models: State<'_, Arc<ModelService>>,
) -> Result<ModelStatus, AudioError> {
    allow(window.label(), false)?;
    let models = models.inner().clone();
    tauri::async_runtime::spawn_blocking(move || models.status())
        .await
        .map_err(|_| AudioError::new("model_service_failed", "无法读取模型状态"))
}
#[tauri::command]
pub async fn select_audio_model(
    window: WebviewWindow,
    models: State<'_, Arc<ModelService>>,
    audio: State<'_, Arc<AudioRuntime>>,
    model_id: String,
) -> Result<ModelStatus, AudioError> {
    if window.label() != "settings" {
        return Err(AudioError::new("forbidden", "只能在设置窗口切换模型"));
    }
    let models = models.inner().clone();
    audio.cancel_all();
    let status = tauri::async_runtime::spawn_blocking(move || {
        models.select(&model_id)?;
        Ok(models.status())
    })
    .await
    .map_err(|_| AudioError::new("model_service_failed", "模型切换失败"))?;
    if status.is_ok() {
        use tauri::Emitter;
        let _ = window.emit_to("character", "audio://model-changed", ());
    }
    status
}
