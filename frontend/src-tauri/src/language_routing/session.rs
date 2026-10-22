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

/// Language identification independent of the transcription model.
///
/// Order of preference: the loaded transcription model when it is a general
/// whisper model; otherwise the smallest general whisper model on disk, loaded
/// once into a dedicated CPU context (`ggml-base.bin` is downloaded for this).
/// A language-specific model (ivrit.ai) is used only as a last resort, with a
/// warning, because it reports its own language for any input.
pub async fn identify_language(
    whisper: Option<&std::sync::Arc<crate::whisper_engine::WhisperEngine>>,
    audio: &[f32],
    preferred_models: &[String],
) -> Result<(String, f32), String> {
    use crate::whisper_engine::{LanguageIdentifier, ModelStatus};
    static IDENTIFIER: std::sync::Mutex<Option<std::sync::Arc<LanguageIdentifier>>> = std::sync::Mutex::new(None);

    crate::whisper_engine::commands::whisper_init().await?;
    let engine = {
        let guard = crate::whisper_engine::commands::WHISPER_ENGINE
            .lock()
            .map_err(|_| "Whisper engine lock poisoned".to_string())?;
        guard.as_ref().cloned().ok_or_else(|| "Whisper engine not initialized".to_string())?
    };
    let models = engine.discover_models().await.map_err(|e| e.to_string())?;
    let is_general = |name: &str| models.iter().any(|m| m.name == name && m.languages.is_empty());

    // 1. The loaded transcription model, when general
    if let Some(w) = whisper {
        if let Some(current) = w.get_current_model().await {
            if is_general(&current) {
                return w.detect_language(audio).await.map_err(|e| e.to_string());
            }
        }
    }

    // 2. A dedicated small general model
    let general_small = models
        .iter()
        .filter(|m| matches!(m.status, ModelStatus::Available) && m.languages.is_empty())
        .min_by_key(|m| m.size_mb);
    if let Some(model) = general_small {
        let cached = IDENTIFIER.lock().ok().and_then(|g| g.clone()).filter(|i| i.model_name == model.name);
        let identifier = match cached {
            Some(i) => i,
            None => {
                let path = model.path.clone();
                let name = model.name.clone();
                let loaded = tokio::task::spawn_blocking(move || LanguageIdentifier::load(&name, &path))
                    .await
                    .map_err(|e| e.to_string())?
                    .map_err(|e| e.to_string())?;
                let loaded = std::sync::Arc::new(loaded);
                if let Ok(mut g) = IDENTIFIER.lock() {
                    *g = Some(loaded.clone());
                }
                loaded
            }
        };
        let window: Vec<f32> = audio.to_vec();
        return tokio::task::spawn_blocking(move || identifier.detect(&window))
            .await
            .map_err(|e| e.to_string())?
            .map_err(|e| e.to_string());
    }

    // 3. Last resort: a language-specific model (unreliable)
    log::warn!("🌐 No general whisper model on disk; language identification with a language-specific model is unreliable");
    if let Some(w) = whisper {
        if w.is_model_loaded().await {
            return w.detect_language(audio).await.map_err(|e| e.to_string());
        }
    }
    if !engine.is_model_loaded().await {
        let available: Vec<_> = models.iter().filter(|m| matches!(m.status, ModelStatus::Available)).collect();
        let candidate = available
            .iter()
            .find(|m| preferred_models.iter().any(|p| p == &m.name))
            .or_else(|| available.iter().min_by_key(|m| m.size_mb))
            .ok_or_else(|| "No whisper model available for language detection".to_string())?;
        engine.load_model(&candidate.name).await.map_err(|e| e.to_string())?;
    }
    engine.detect_language(audio).await.map_err(|e| e.to_string())
}

/// Whisper models that enabled language routes point at (candidates for the
/// language-identification pass, so the later switch needs no second load).
pub async fn routed_whisper_models<R: Runtime>(app: &AppHandle<R>) -> Vec<String> {
    let state = app.state::<AppState>();
    match LanguageRouteRepository::list(state.db_manager.pool()).await {
        Ok(routes) => routes
            .into_iter()
            .filter(|r| r.enabled && r.stt_provider.as_deref() == Some("localWhisper"))
            .filter_map(|r| r.stt_model)
            .collect(),
        Err(e) => {
            log::warn!("Could not read language routes: {}", e);
            Vec::new()
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
