//! Tauri commands for language routes.

use tauri::State;

use super::{LanguageRoute, LanguageRouteRepository};
use crate::state::AppState;

#[tauri::command]
pub async fn language_routes_list(state: State<'_, AppState>) -> Result<Vec<LanguageRoute>, String> {
    LanguageRouteRepository::list(state.db_manager.pool())
        .await
        .map_err(|e| format!("Failed to list language routes: {}", e))
}

#[tauri::command]
pub async fn language_routes_upsert(
    state: State<'_, AppState>,
    route: LanguageRoute,
) -> Result<LanguageRoute, String> {
    LanguageRouteRepository::upsert(state.db_manager.pool(), route)
        .await
        .map_err(|e| format!("Failed to save language route: {}", e))
}

#[tauri::command]
pub async fn language_routes_delete(state: State<'_, AppState>, language: String) -> Result<(), String> {
    LanguageRouteRepository::delete(state.db_manager.pool(), &language)
        .await
        .map_err(|e| format!("Failed to delete language route: {}", e))
}

/// Language currently pinned for the active recording (auto mode), if any.
#[tauri::command]
pub async fn language_routes_detected_language() -> Result<Option<String>, String> {
    Ok(super::session::detected())
}
