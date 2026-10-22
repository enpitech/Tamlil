//! Tauri commands for speaker identification (status, model download, toggle).

use std::io::Write;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Runtime};

#[derive(Debug, Serialize)]
pub struct SpeakerIdStatus {
    pub enabled: bool,
    pub model_available: bool,
    pub model_path: Option<String>,
    pub model_size_mb: u64,
}

#[tauri::command]
pub async fn speaker_id_get_status() -> Result<SpeakerIdStatus, String> {
    Ok(SpeakerIdStatus {
        enabled: super::is_enabled(),
        model_available: super::model_is_available(),
        model_path: super::model_path().map(|p| p.to_string_lossy().to_string()),
        model_size_mb: super::MODEL_BYTES / (1024 * 1024),
    })
}

#[tauri::command]
pub async fn speaker_id_set_enabled(enabled: bool) -> Result<(), String> {
    super::set_enabled(enabled);
    log::info!("Speaker identification {}", if enabled { "enabled" } else { "disabled" });
    Ok(())
}

/// Download the speaker embedding model, emitting `speaker-id-download-progress`
/// events with `{ progress }` (0-100).
#[tauri::command]
pub async fn speaker_id_download_model<R: Runtime>(app: AppHandle<R>) -> Result<(), String> {
    let path = super::model_path().ok_or("Speaker models directory not initialised")?;
    if super::model_is_available() {
        return Ok(());
    }
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let tmp = path.with_extension("part");

    let client = reqwest::Client::builder()
        .user_agent(concat!("Tamlil/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|e| e.to_string())?;
    let mut response = client
        .get(super::MODEL_URL)
        .send()
        .await
        .map_err(|e| format!("Download failed to start: {}", e))?;
    if !response.status().is_success() {
        return Err(format!("Download failed with status {}", response.status()));
    }
    let total = response.content_length().unwrap_or(super::MODEL_BYTES);
    let mut file = std::fs::File::create(&tmp).map_err(|e| e.to_string())?;
    let mut downloaded: u64 = 0;
    let mut last_percent: u64 = u64::MAX;
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|e| format!("Download interrupted: {}", e))?
    {
        file.write_all(&chunk).map_err(|e| e.to_string())?;
        downloaded += chunk.len() as u64;
        let percent = if total > 0 { downloaded * 100 / total } else { 0 };
        if percent != last_percent {
            last_percent = percent;
            let _ = app.emit("speaker-id-download-progress", serde_json::json!({ "progress": percent }));
        }
    }
    file.flush().map_err(|e| e.to_string())?;
    drop(file);

    if downloaded != super::MODEL_BYTES {
        let _ = std::fs::remove_file(&tmp);
        return Err(format!(
            "Downloaded file has unexpected size ({} bytes, expected {})",
            downloaded,
            super::MODEL_BYTES
        ));
    }
    std::fs::rename(&tmp, &path).map_err(|e| e.to_string())?;
    log::info!("Speaker model downloaded to {}", path.display());
    let _ = app.emit("speaker-id-download-progress", serde_json::json!({ "progress": 100, "done": true }));
    Ok(())
}

#[tauri::command]
pub async fn speaker_id_delete_model() -> Result<(), String> {
    super::unload();
    if let Some(path) = super::model_path() {
        if path.exists() {
            std::fs::remove_file(&path).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

/// Name a speaker of the current recording and remember the voice for future
/// meetings. Emits `speaker-renamed` so open views update earlier lines.
#[tauri::command]
pub async fn speaker_id_name_speaker<R: Runtime>(
    app: AppHandle<R>,
    index: u32,
    name: String,
) -> Result<super::SpeakerLabel, String> {
    let label = super::name_speaker(index, &name).map_err(|e| e.to_string())?;
    let _ = app.emit("speaker-renamed", &label);
    Ok(label)
}

#[tauri::command]
pub async fn speaker_profiles_list() -> Result<Vec<super::profiles::VoiceProfileSummary>, String> {
    Ok(super::profiles::with(|s| s.list()).unwrap_or_default())
}

#[tauri::command]
pub async fn speaker_profiles_rename(id: String, name: String) -> Result<(), String> {
    super::profiles::with_mut(|s| s.rename(&id, &name)).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn speaker_profiles_delete(id: String) -> Result<(), String> {
    super::profiles::with_mut(|s| s.delete(&id)).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn speaker_profiles_clear() -> Result<(), String> {
    super::profiles::with_mut(|s| s.clear()).map_err(|e| e.to_string())
}
