//! Per-language model routing.
//!
//! A *language route* maps a language code to the transcription model and the
//! summary model that work best for it (for example Hebrew → ivrit.ai whisper
//! + a Hebrew-capable GGUF, summarised directly in Hebrew).
//!
//! Routes are applied in two places:
//!
//! * **Transcription** – `session::resolve_transcript_config` overrides the
//!   saved transcript provider/model when the meeting language is known. The
//!   language is either the user's fixed language preference or, in auto mode,
//!   the language detected from the first seconds of speech
//!   (`detect_and_route` in the transcription worker).
//! * **Summaries** – `summary_route_for` picks the summary provider/model and
//!   whether to write the summary directly in the language instead of the
//!   upstream English-then-translate pipeline.

pub mod commands;
pub mod session;

use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

pub const SUMMARY_MODE_DIRECT: &str = "direct";
pub const SUMMARY_MODE_TRANSLATE: &str = "translate";

/// Hebrew whisper model (ivrit.ai); the high-accuracy alternative for the `he` route.
pub const HEBREW_WHISPER_MODEL: &str = "ivrit-large-v3-turbo";
/// Default engine of the `he` route: ivrit.ai's streaming Zipformer (69 MB, ~25x
/// cheaper than whisper turbo, close accuracy on read speech; see docs/QA.md).
pub const HEBREW_DEFAULT_PROVIDER: &str = "parakeet";
pub const HEBREW_DEFAULT_MODEL: &str = crate::parakeet_engine::IVRIT_ZIPFORMER_MODEL;
pub const ENGLISH_DEFAULT_PROVIDER: &str = "parakeet";
pub const ENGLISH_DEFAULT_MODEL: &str = crate::config::DEFAULT_PARAKEET_MODEL;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, PartialEq, Eq)]
pub struct LanguageRoute {
    /// BCP-47 / ISO-639-1 code, lower case (`he`, `en`, `pt`).
    pub language: String,
    /// `localWhisper` or `parakeet`; `None` keeps the global transcription setting.
    pub stt_provider: Option<String>,
    pub stt_model: Option<String>,
    /// Summary provider id (`builtin-ai`, `ollama`, `claude`, ...); `None` keeps the global setting.
    pub summary_provider: Option<String>,
    pub summary_model: Option<String>,
    /// `direct` or `translate`.
    pub summary_mode: String,
    pub enabled: bool,
}

impl LanguageRoute {
    pub fn normalised(mut self) -> Self {
        self.language = normalise_language_code(&self.language);
        self.stt_provider = self.stt_provider.filter(|s| !s.trim().is_empty());
        self.stt_model = self.stt_model.filter(|s| !s.trim().is_empty());
        self.summary_provider = self.summary_provider.filter(|s| !s.trim().is_empty());
        self.summary_model = self.summary_model.filter(|s| !s.trim().is_empty());
        if self.summary_mode != SUMMARY_MODE_TRANSLATE {
            self.summary_mode = SUMMARY_MODE_DIRECT.to_string();
        }
        self
    }

    pub fn has_stt_override(&self) -> bool {
        self.stt_provider.is_some() && self.stt_model.is_some()
    }

    pub fn has_summary_override(&self) -> bool {
        self.summary_provider.is_some() && self.summary_model.is_some()
    }

    pub fn summarises_directly(&self) -> bool {
        self.summary_mode == SUMMARY_MODE_DIRECT
    }
}

/// `he-IL` → `he`, `EN` → `en`. Whisper's "auto" pseudo-codes map to `None`.
pub fn normalise_language_code(code: &str) -> String {
    let trimmed = code.trim().to_ascii_lowercase();
    trimmed
        .split(|c| c == '-' || c == '_')
        .next()
        .unwrap_or("")
        .to_string()
}

pub fn is_auto_language(code: Option<&str>) -> bool {
    match code.map(|c| c.trim().to_ascii_lowercase()) {
        None => true,
        Some(c) => c.is_empty() || c == "auto" || c == "auto-translate",
    }
}

// ============================================================================
// Repository
// ============================================================================

pub struct LanguageRouteRepository;

impl LanguageRouteRepository {
    pub async fn list(pool: &SqlitePool) -> Result<Vec<LanguageRoute>, sqlx::Error> {
        sqlx::query_as::<_, LanguageRoute>(
            "SELECT language, stt_provider, stt_model, summary_provider, summary_model, summary_mode, enabled \
             FROM language_routes ORDER BY language",
        )
        .fetch_all(pool)
        .await
    }

    pub async fn get(pool: &SqlitePool, language: &str) -> Result<Option<LanguageRoute>, sqlx::Error> {
        let language = normalise_language_code(language);
        if language.is_empty() {
            return Ok(None);
        }
        sqlx::query_as::<_, LanguageRoute>(
            "SELECT language, stt_provider, stt_model, summary_provider, summary_model, summary_mode, enabled \
             FROM language_routes WHERE language = $1",
        )
        .bind(language)
        .fetch_optional(pool)
        .await
    }

    /// Enabled route for a language, if any.
    pub async fn get_enabled(pool: &SqlitePool, language: &str) -> Result<Option<LanguageRoute>, sqlx::Error> {
        Ok(Self::get(pool, language).await?.filter(|r| r.enabled))
    }

    pub async fn any_enabled(pool: &SqlitePool) -> Result<bool, sqlx::Error> {
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM language_routes WHERE enabled = 1")
            .fetch_one(pool)
            .await?;
        Ok(count > 0)
    }

    /// Default routes: Hebrew on ivrit.ai's streaming model with summaries written
    /// directly in Hebrew, and English on Parakeet (the global default is Hebrew-only,
    /// so an English recording needs its own route). The Hebrew route is seeded once,
    /// when no route exists yet (onboarding, or the first start of an installation that
    /// predates language routes); the English route is added whenever it is missing and
    /// the user has not customised the table beyond the Hebrew default. Returns true
    /// when a route was created.
    pub async fn seed_hebrew_default(pool: &SqlitePool) -> Result<bool, sqlx::Error> {
        let languages: Vec<String> = sqlx::query_scalar("SELECT language FROM language_routes")
            .fetch_all(pool)
            .await?;
        let mut created = false;
        if languages.is_empty() {
            Self::upsert(
                pool,
                LanguageRoute {
                    language: "he".to_string(),
                    stt_provider: Some(HEBREW_DEFAULT_PROVIDER.to_string()),
                    stt_model: Some(HEBREW_DEFAULT_MODEL.to_string()),
                    summary_provider: None,
                    summary_model: None,
                    summary_mode: SUMMARY_MODE_DIRECT.to_string(),
                    enabled: true,
                },
            )
            .await?;
            created = true;
        }
        let untouched = languages.iter().all(|l| l == "he");
        if untouched && !languages.iter().any(|l| l == "en") {
            Self::upsert(
                pool,
                LanguageRoute {
                    language: "en".to_string(),
                    stt_provider: Some(ENGLISH_DEFAULT_PROVIDER.to_string()),
                    stt_model: Some(ENGLISH_DEFAULT_MODEL.to_string()),
                    summary_provider: None,
                    summary_model: None,
                    summary_mode: SUMMARY_MODE_DIRECT.to_string(),
                    enabled: true,
                },
            )
            .await?;
            created = true;
        }
        Ok(created)
    }

    pub async fn upsert(pool: &SqlitePool, route: LanguageRoute) -> Result<LanguageRoute, sqlx::Error> {
        let route = route.normalised();
        if route.language.is_empty() {
            return Err(sqlx::Error::Protocol("language code is required".into()));
        }
        sqlx::query(
            r#"
            INSERT INTO language_routes
                (language, stt_provider, stt_model, summary_provider, summary_model, summary_mode, enabled, updated_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
            ON CONFLICT(language) DO UPDATE SET
                stt_provider = excluded.stt_provider,
                stt_model = excluded.stt_model,
                summary_provider = excluded.summary_provider,
                summary_model = excluded.summary_model,
                summary_mode = excluded.summary_mode,
                enabled = excluded.enabled,
                updated_at = excluded.updated_at
            "#,
        )
        .bind(&route.language)
        .bind(&route.stt_provider)
        .bind(&route.stt_model)
        .bind(&route.summary_provider)
        .bind(&route.summary_model)
        .bind(&route.summary_mode)
        .bind(route.enabled)
        .execute(pool)
        .await?;
        Ok(route)
    }

    pub async fn delete(pool: &SqlitePool, language: &str) -> Result<(), sqlx::Error> {
        sqlx::query("DELETE FROM language_routes WHERE language = $1")
            .bind(normalise_language_code(language))
            .execute(pool)
            .await?;
        Ok(())
    }
}

/// Summary routing decision for a meeting.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SummaryRouting {
    pub provider: Option<String>,
    pub model: Option<String>,
    /// Language code to write the summary in directly (skips the English pass).
    pub direct_language: Option<String>,
}

/// Resolve how a summary should be produced for `language`.
pub async fn summary_route_for(pool: &SqlitePool, language: Option<&str>) -> SummaryRouting {
    let Some(code) = language else {
        return SummaryRouting::default();
    };
    if is_auto_language(Some(code)) {
        return SummaryRouting::default();
    }
    match LanguageRouteRepository::get_enabled(pool, code).await {
        Ok(Some(route)) => SummaryRouting {
            provider: route.summary_provider.clone().filter(|_| route.has_summary_override()),
            model: route.summary_model.clone().filter(|_| route.has_summary_override()),
            direct_language: if route.summarises_directly() {
                Some(route.language.clone())
            } else {
                None
            },
        },
        Ok(None) => SummaryRouting::default(),
        Err(e) => {
            log::warn!("Failed to read language route for '{}': {}", code, e);
            SummaryRouting::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn test_pool() -> SqlitePool {
        let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        pool
    }

    #[test]
    fn language_codes_are_normalised() {
        assert_eq!(normalise_language_code("he-IL"), "he");
        assert_eq!(normalise_language_code(" EN "), "en");
        assert_eq!(normalise_language_code("pt_BR"), "pt");
        assert!(is_auto_language(None));
        assert!(is_auto_language(Some("auto")));
        assert!(is_auto_language(Some("auto-translate")));
        assert!(!is_auto_language(Some("he")));
    }

    #[tokio::test]
    async fn routes_round_trip_and_resolve_summary_routing() {
        let pool = test_pool().await;
        assert!(!LanguageRouteRepository::any_enabled(&pool).await.unwrap());

        let saved = LanguageRouteRepository::upsert(
            &pool,
            LanguageRoute {
                language: "HE".to_string(),
                stt_provider: Some("localWhisper".to_string()),
                stt_model: Some("ivrit-large-v3-turbo".to_string()),
                summary_provider: Some("builtin-ai".to_string()),
                summary_model: Some("qwen3.5:4b".to_string()),
                summary_mode: "weird".to_string(),
                enabled: true,
            },
        )
        .await
        .unwrap();
        assert_eq!(saved.language, "he");
        assert_eq!(saved.summary_mode, SUMMARY_MODE_DIRECT);
        assert!(LanguageRouteRepository::any_enabled(&pool).await.unwrap());

        let routing = summary_route_for(&pool, Some("he-IL")).await;
        assert_eq!(routing.provider.as_deref(), Some("builtin-ai"));
        assert_eq!(routing.model.as_deref(), Some("qwen3.5:4b"));
        assert_eq!(routing.direct_language.as_deref(), Some("he"));

        assert_eq!(summary_route_for(&pool, Some("en")).await, SummaryRouting::default());
        assert_eq!(summary_route_for(&pool, Some("auto")).await, SummaryRouting::default());

        // Disabled routes are ignored
        LanguageRouteRepository::upsert(&pool, LanguageRoute { enabled: false, ..saved.clone() })
            .await
            .unwrap();
        assert!(LanguageRouteRepository::get_enabled(&pool, "he").await.unwrap().is_none());
        assert_eq!(LanguageRouteRepository::list(&pool).await.unwrap().len(), 1);

        LanguageRouteRepository::delete(&pool, "he").await.unwrap();
        assert!(LanguageRouteRepository::list(&pool).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn translate_mode_keeps_english_pipeline() {
        let pool = test_pool().await;
        LanguageRouteRepository::upsert(
            &pool,
            LanguageRoute {
                language: "fr".to_string(),
                stt_provider: None,
                stt_model: None,
                summary_provider: None,
                summary_model: None,
                summary_mode: SUMMARY_MODE_TRANSLATE.to_string(),
                enabled: true,
            },
        )
        .await
        .unwrap();
        let routing = summary_route_for(&pool, Some("fr")).await;
        assert!(routing.provider.is_none());
        assert!(routing.direct_language.is_none());
    }
}
