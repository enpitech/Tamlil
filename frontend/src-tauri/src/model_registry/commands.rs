//! Tauri commands for the model registry (add / import / edit / remove models).

use std::path::PathBuf;

use serde::Deserialize;
use tauri::{AppHandle, Emitter, Runtime};
use tauri_plugin_dialog::DialogExt;

use super::{EntryOverrides, ModelKind, RegistryEntry};

#[derive(Debug, Deserialize)]
pub struct AddModelFromUrlArgs {
    pub kind: ModelKind,
    pub url: String,
    #[serde(default)]
    pub overrides: EntryOverrides,
}

#[derive(Debug, Deserialize)]
pub struct ImportLocalModelArgs {
    pub kind: ModelKind,
    pub path: String,
    #[serde(default)]
    pub overrides: EntryOverrides,
}

/// Notify engines and UI that the set of registered models changed.
async fn broadcast_change<R: Runtime>(app: &AppHandle<R>, kind: ModelKind) {
    if kind == ModelKind::Gguf {
        // The built-in summary model manager caches its scan; refresh it.
        if let Err(e) = crate::summary::summary_engine::commands::rescan_models(app).await {
            log::warn!("Failed to rescan built-in summary models after registry change: {}", e);
        }
    }
    let _ = app.emit("model-registry-changed", serde_json::json!({ "kind": kind }));
}

#[tauri::command]
pub async fn registry_list_models(kind: ModelKind) -> Result<Vec<RegistryEntry>, String> {
    Ok(super::entries(kind))
}

#[tauri::command]
pub async fn registry_get_models_directory(kind: ModelKind) -> Result<String, String> {
    super::with(|r| r.dir_for(kind).to_string_lossy().to_string()).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn registry_add_model_from_url<R: Runtime>(
    app: AppHandle<R>,
    args: AddModelFromUrlArgs,
) -> Result<RegistryEntry, String> {
    let kind = args.kind;
    let entry = super::with_mut(|r| r.add_from_url(kind, &args.url, args.overrides))
        .map_err(|e| e.to_string())?;
    log::info!("Registered {:?} model '{}' from URL", kind, entry.id);
    broadcast_change(&app, kind).await;
    Ok(entry)
}

#[tauri::command]
pub async fn registry_import_local_model<R: Runtime>(
    app: AppHandle<R>,
    args: ImportLocalModelArgs,
) -> Result<RegistryEntry, String> {
    let kind = args.kind;
    let path = PathBuf::from(args.path);
    let overrides = args.overrides;
    // File copy can be slow for multi-GB models; keep it off the async runtime.
    let entry = tokio::task::spawn_blocking(move || {
        super::with_mut(|r| r.import_local(kind, &path, overrides))
    })
    .await
    .map_err(|e| format!("Import task failed: {}", e))?
    .map_err(|e| e.to_string())?;
    log::info!("Imported {:?} model '{}' from local file", kind, entry.id);
    broadcast_change(&app, kind).await;
    Ok(entry)
}

#[tauri::command]
pub async fn registry_update_model<R: Runtime>(
    app: AppHandle<R>,
    kind: ModelKind,
    id: String,
    overrides: EntryOverrides,
) -> Result<RegistryEntry, String> {
    let entry = super::with_mut(|r| r.update(kind, &id, overrides)).map_err(|e| e.to_string())?;
    broadcast_change(&app, kind).await;
    Ok(entry)
}

#[tauri::command]
pub async fn registry_remove_model<R: Runtime>(
    app: AppHandle<R>,
    kind: ModelKind,
    id: String,
    delete_file: bool,
) -> Result<(), String> {
    super::with_mut(|r| r.remove(kind, &id, delete_file)).map_err(|e| e.to_string())?;
    log::info!("Removed {:?} model '{}' (delete_file={})", kind, id, delete_file);
    broadcast_change(&app, kind).await;
    Ok(())
}

/// Open a native file picker filtered to the kind's extension and return the chosen path.
#[tauri::command]
pub async fn registry_pick_model_file<R: Runtime>(
    app: AppHandle<R>,
    kind: ModelKind,
) -> Result<Option<String>, String> {
    let (label, extensions): (&str, &[&str]) = match kind {
        ModelKind::WhisperGgml => ("Whisper GGML model", &["bin"]),
        ModelKind::Gguf => ("GGUF model", &["gguf"]),
    };
    let app_clone = app.clone();
    let picked = tokio::task::spawn_blocking(move || {
        app_clone
            .dialog()
            .file()
            .add_filter(label, extensions)
            .blocking_pick_file()
    })
    .await
    .map_err(|e| format!("File dialog task failed: {}", e))?;
    Ok(picked.map(|p| p.to_string()))
}
