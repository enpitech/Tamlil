//! Data inventory, reset and uninstall (fork-specific).
//!
//! The app stores state in several places that an ordinary "drag to Trash"
//! never touches: the Tauri app data dir (database, models, onboarding store,
//! voice profiles), the web view's storage, caches and saved window state.
//! This module lists them, wipes them (optionally keeping the models) and,
//! for a full uninstall, also removes the application itself.

use std::path::{Path, PathBuf};

use serde::Serialize;
use tauri::{AppHandle, Manager, Runtime};

const BUNDLE_ID: &str = "com.enpitech.tamlil";

#[derive(Debug, Clone, Serialize)]
pub struct DataLocation {
    pub path: String,
    /// `data` (database, settings, voices), `models`, `webview`, `cache`, `state`.
    pub kind: String,
    pub exists: bool,
    pub size_bytes: u64,
}

fn dir_size(path: &Path) -> u64 {
    let mut total = 0;
    if let Ok(entries) = std::fs::read_dir(path) {
        for entry in entries.flatten() {
            let p = entry.path();
            match entry.metadata() {
                Ok(m) if m.is_dir() => total += dir_size(&p),
                Ok(m) => total += m.len(),
                Err(_) => {}
            }
        }
    } else if let Ok(m) = std::fs::metadata(path) {
        total = m.len();
    }
    total
}

fn home() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
}

/// Every location the app may have written to, deduplicated.
pub fn locations<R: Runtime>(app: &AppHandle<R>) -> Vec<(PathBuf, &'static str)> {
    let mut out: Vec<(PathBuf, &'static str)> = Vec::new();
    let mut push = |p: Option<PathBuf>, kind: &'static str| {
        if let Some(p) = p {
            if !out.iter().any(|(q, _)| q == &p) {
                out.push((p, kind));
            }
        }
    };
    let data = app.path().app_data_dir().ok();
    push(data.as_ref().map(|d| d.join("models")), "models");
    push(data.clone(), "data");
    push(app.path().app_config_dir().ok(), "data");
    push(app.path().app_local_data_dir().ok(), "data");
    push(app.path().app_cache_dir().ok(), "cache");

    if let Some(home) = home() {
        #[cfg(target_os = "macos")]
        {
            let lib = home.join("Library");
            push(Some(lib.join("WebKit").join(BUNDLE_ID)), "webview");
            push(Some(lib.join("Caches").join(BUNDLE_ID)), "cache");
            push(Some(lib.join("HTTPStorages").join(BUNDLE_ID)), "cache");
            push(Some(lib.join("Saved Application State").join(format!("{}.savedState", BUNDLE_ID))), "state");
            push(Some(lib.join("Preferences").join(format!("{}.plist", BUNDLE_ID))), "state");
            push(Some(lib.join("Logs").join("Tamlil")), "state");
        }
        #[cfg(target_os = "windows")]
        {
            if let Some(local) = std::env::var_os("LOCALAPPDATA").map(PathBuf::from) {
                push(Some(local.join(BUNDLE_ID)), "data");
                push(Some(local.join("Tamlil")), "data");
                push(Some(local.join(BUNDLE_ID).join("EBWebView")), "webview");
            }
            if let Some(roaming) = std::env::var_os("APPDATA").map(PathBuf::from) {
                push(Some(roaming.join(BUNDLE_ID)), "data");
            }
            let _ = &home;
        }
        #[cfg(target_os = "linux")]
        {
            push(Some(home.join(".local/share").join(BUNDLE_ID)), "data");
            push(Some(home.join(".config").join(BUNDLE_ID)), "data");
            push(Some(home.join(".cache").join(BUNDLE_ID)), "cache");
        }
    }
    out
}

/// Where the running application lives (the `.app` bundle on macOS, the
/// install directory on Windows, the AppImage or binary on Linux).
pub fn app_location() -> Option<PathBuf> {
    #[cfg(target_os = "linux")]
    if let Some(p) = std::env::var_os("APPIMAGE") {
        return Some(PathBuf::from(p));
    }
    let exe = std::env::current_exe().ok()?;
    #[cfg(target_os = "macos")]
    {
        // .../Tamlil.app/Contents/MacOS/Tamlil
        let bundle = exe.ancestors().find(|p| p.extension().map(|e| e == "app").unwrap_or(false))?;
        return Some(bundle.to_path_buf());
    }
    #[cfg(target_os = "windows")]
    {
        return exe.parent().map(|p| p.to_path_buf());
    }
    #[allow(unreachable_code)]
    Some(exe)
}

#[derive(Debug, Clone, Serialize)]
pub struct DataReport {
    pub locations: Vec<DataLocation>,
    pub app_path: Option<String>,
    pub total_bytes: u64,
    pub models_bytes: u64,
}

#[tauri::command]
pub async fn maintenance_data_report<R: Runtime>(app: AppHandle<R>) -> Result<DataReport, String> {
    let mut locations = Vec::new();
    let (mut total, mut models) = (0u64, 0u64);
    let all = self::locations(&app);
    // Sizes: a nested path (models inside data) must not be counted twice.
    for (path, kind) in &all {
        let exists = path.exists();
        let mut size = if exists { dir_size(path) } else { 0 };
        if *kind == "data" {
            for (other, other_kind) in &all {
                if *other_kind == "models" && other.starts_with(path) && other.exists() {
                    size = size.saturating_sub(dir_size(other));
                }
            }
        }
        if *kind == "models" {
            models += size;
        }
        total += size;
        locations.push(DataLocation { path: path.to_string_lossy().to_string(), kind: kind.to_string(), exists, size_bytes: size });
    }
    Ok(DataReport { locations, app_path: app_location().map(|p| p.to_string_lossy().to_string()), total_bytes: total, models_bytes: models })
}

fn remove_path(path: &Path) -> Result<(), String> {
    if !path.exists() {
        return Ok(());
    }
    let result = if path.is_dir() { std::fs::remove_dir_all(path) } else { std::fs::remove_file(path) };
    result.map_err(|e| format!("{}: {}", path.display(), e))
}

/// Delete the app's data. With `keep_models`, the models directory survives
/// (and so does its parent); everything else goes. The process exits afterwards
/// so nothing is written back; the next launch starts with onboarding.
#[tauri::command]
pub async fn maintenance_reset<R: Runtime>(app: AppHandle<R>, keep_models: bool) -> Result<(), String> {
    let all = locations(&app);
    let models_dirs: Vec<PathBuf> = all.iter().filter(|(_, k)| *k == "models").map(|(p, _)| p.clone()).collect();
    let mut errors = Vec::new();
    for (path, kind) in &all {
        if *kind == "models" {
            if !keep_models {
                if let Err(e) = remove_path(path) { errors.push(e); }
            }
            continue;
        }
        let contains_kept_models = keep_models && models_dirs.iter().any(|m| m.starts_with(path));
        if contains_kept_models {
            // Remove the children except the models directory
            if let Ok(entries) = std::fs::read_dir(path) {
                for entry in entries.flatten() {
                    let p = entry.path();
                    if models_dirs.iter().any(|m| m == &p || m.starts_with(&p)) {
                        continue;
                    }
                    if let Err(e) = remove_path(&p) { errors.push(e); }
                }
            }
        } else if let Err(e) = remove_path(path) {
            errors.push(e);
        }
    }
    if !errors.is_empty() {
        log::warn!("Reset finished with errors: {:?}", errors);
    }
    log::info!("App data reset (keep_models = {}); exiting", keep_models);
    // Give the frontend a moment to show its confirmation, then leave without
    // running shutdown hooks that would recreate files.
    tokio::time::sleep(std::time::Duration::from_millis(400)).await;
    std::process::exit(0);
}

/// Full uninstall: wipe all data, then remove the application itself.
/// macOS: the .app bundle is moved to the Trash through Finder. Windows: the
/// bundled uninstaller is launched (or Apps & features opened). Linux: the
/// AppImage is deleted; a package install prints the command to run.
#[tauri::command]
pub async fn maintenance_uninstall<R: Runtime>(app: AppHandle<R>) -> Result<String, String> {
    let app_path = app_location();
    let all = locations(&app);
    let mut errors = Vec::new();
    for (path, _) in &all {
        if let Err(e) = remove_path(path) { errors.push(e); }
    }
    let mut message = String::from("Data removed.");
    if let Some(bundle) = app_path {
        #[cfg(target_os = "macos")]
        {
            let script = format!("tell application \"Finder\" to delete POSIX file \"{}\"", bundle.display());
            match std::process::Command::new("osascript").arg("-e").arg(&script).output() {
                Ok(o) if o.status.success() => message.push_str(" Tamlil was moved to the Trash."),
                Ok(o) => message.push_str(&format!(" Could not move the app to the Trash ({}); delete {} by hand.", String::from_utf8_lossy(&o.stderr).trim(), bundle.display())),
                Err(e) => message.push_str(&format!(" Could not run Finder ({}); delete {} by hand.", e, bundle.display())),
            }
            let _ = std::process::Command::new("defaults").arg("delete").arg(BUNDLE_ID).output();
        }
        #[cfg(target_os = "windows")]
        {
            let uninstaller = bundle.join("uninstall.exe");
            if uninstaller.exists() {
                let _ = std::process::Command::new(&uninstaller).spawn();
                message.push_str(" The uninstaller was started.");
            } else {
                let _ = std::process::Command::new("cmd").args(["/C", "start", "ms-settings:appsfeatures"]).spawn();
                message.push_str(" Remove Tamlil from Apps & features to finish.");
            }
        }
        #[cfg(target_os = "linux")]
        {
            if bundle.extension().map(|e| e.eq_ignore_ascii_case("AppImage")).unwrap_or(false) {
                match std::fs::remove_file(&bundle) {
                    Ok(()) => message.push_str(" The AppImage was deleted."),
                    Err(e) => message.push_str(&format!(" Could not delete {} ({}).", bundle.display(), e)),
                }
            } else {
                message.push_str(" Installed from a package: run `sudo apt remove tamlil` (or your package manager) to finish.");
            }
        }
    }
    if !errors.is_empty() {
        message.push_str(&format!(" Some files could not be removed: {}", errors.join("; ")));
    }
    log::info!("Uninstall: {}", message);
    let app_handle = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(1500)).await;
        let _ = &app_handle;
        std::process::exit(0);
    });
    Ok(message)
}
