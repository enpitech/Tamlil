//! Per-recording routing state and transcript-config resolution.

use std::sync::RwLock;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, Runtime};

use super::{is_auto_language, normalise_language_code, LanguageRouteRepository};
use crate::api::api::TranscriptConfig;
use crate::state::AppState;

/// Language detected for the current recording (auto mode only).
static DETECTED_LANGUAGE: RwLock<Option<String>> = RwLock::new(None);

/// Minimum seconds of speech to collect before running language identification.
pub const DETECTION_WINDOW_SECS: f32 = 8.0;
/// Give up on detection after this much speech without a confident result.
pub const DETECTION_MAX_SECS: f32 = 30.0;
/// Minimum whisper language probability to act on.
pub const DETECTION_MIN_CONFIDENCE: f32 = 0.5;

pub fn reset() {
    if let Ok(mut guard) = DETECTED_LANGUAGE.write() {
        *guard = None;
    }
}

pub fn set_detected(language: &str) {
    let code = normalise_language_code(language);
    if let Ok(mut guard) = DETECTED_LANGUAGE.write() {
        *guard = if code.is_empty() { None } else { Some(code) };
    }
}

pub fn detected() -> Option<String> {
    DETECTED_LANGUAGE.read().ok().and_then(|g| g.clone())
}

/// The user's fixed language preference, if it is not an auto mode.
pub fn fixed_language() -> Option<String> {
    let pref = crate::get_language_preference_internal();
    if is_auto_language(pref.as_deref()) {
        None
    } else {
        pref.map(|p| normalise_language_code(&p)).filter(|p| !p.is_empty())
    }
}

/// Language the current recording is in, when known: fixed preference first,
/// then the detected language.
pub fn effective_language() -> Option<String> {
    fixed_language().or_else(detected)
}

/// Language to hand to whisper for the current chunk. In auto mode a detected
/// language is pinned so the routed model does not have to re-detect every chunk.
pub fn whisper_language() -> Option<String> {
    let pref = crate::get_language_preference_internal();
    match pref.as_deref() {
        Some("auto") | None => detected().or(pref),
        _ => pref,
    }
}

/// Whether the worker should run language identification for this recording.
pub async fn auto_detection_wanted<R: Runtime>(app: &AppHandle<R>) -> bool {
    let pref = crate::get_language_preference_internal();
    if !is_auto_language(pref.as_deref()) || pref.as_deref() == Some("auto-translate") {
        return false;
    }
    if detected().is_some() {
        return false;
    }
    let state = app.state::<AppState>();
    match LanguageRouteRepository::any_enabled(state.db_manager.pool()).await {
        Ok(any) => any,
        Err(e) => {
            log::warn!("Could not read language routes: {}", e);
            false
        }
    }
}

fn default_transcript_config() -> TranscriptConfig {
    TranscriptConfig {
        provider: "parakeet".to_string(),
        model: crate::config::DEFAULT_PARAKEET_MODEL.to_string(),
        api_key: None,
    }
}

/// The saved transcript config with the active language route applied.
pub async fn resolve_transcript_config<R: Runtime>(app: &AppHandle<R>) -> TranscriptConfig {
    let base = match crate::api::api::api_get_transcript_config(app.clone(), app.state(), None).await {
        Ok(Some(config)) => config,
        Ok(None) => {
            log::info!("📝 No transcript config found, defaulting to parakeet");
            default_transcript_config()
        }
        Err(e) => {
            log::warn!("⚠️ Failed to get transcript config: {}, defaulting to parakeet", e);
            default_transcript_config()
        }
    };

    let Some(language) = effective_language() else {
        return base;
    };
    let state = app.state::<AppState>();
    match LanguageRouteRepository::get_enabled(state.db_manager.pool(), &language).await {
        Ok(Some(route)) if route.has_stt_override() => {
            let provider = route.stt_provider.clone().unwrap_or_default();
            let model = route.stt_model.clone().unwrap_or_default();
            if provider == base.provider && model == base.model {
                return base;
            }
            log::info!(
                "🌐 Language route '{}' selects {}/{} for transcription (global: {}/{})",
                language, provider, model, base.provider, base.model
            );
            TranscriptConfig {
                provider,
                model,
                api_key: base.api_key,
            }
        }
        Ok(_) => base,
        Err(e) => {
            log::warn!("Failed to read language route for '{}': {}", language, e);
            base
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct RoutedLanguageEvent {
    pub language: String,
    pub confidence: f32,
    pub provider: String,
    pub model: String,
    pub switched: bool,
}

pub fn emit_routed<R: Runtime>(app: &AppHandle<R>, event: &RoutedLanguageEvent) {
    if let Err(e) = app.emit("transcription-language-routed", event) {
        log::warn!("Failed to emit transcription-language-routed: {}", e);
    }
}
