//! Model registry: a single source of truth for every local model the app
//! can use, whether it is a Whisper GGML file for transcription or a GGUF
//! file for built-in summaries.
//!
//! Entries come from three places, merged in this order:
//!
//! 1. **Built-in** entries compiled into the app (the upstream catalogs plus
//!    the Hebrew ivrit.ai models).
//! 2. **User** entries persisted in `<models dir>/registry.json`, added from
//!    a download URL or by importing a local file.
//! 3. **Discovered** files found on disk that no other entry claims, so a
//!    file dropped into the models folder by hand just shows up.
//!
//! The engines (`whisper_engine`, `summary_engine`) read from this module
//! instead of their former hardcoded catalogs.

pub mod commands;

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, RwLock};

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};

const REGISTRY_FILE: &str = "registry.json";
const REGISTRY_VERSION: u32 = 1;

/// Which engine consumes a model file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelKind {
    /// whisper.cpp GGML/GGUF model (`ggml-*.bin`) used for transcription.
    WhisperGgml,
    /// GGUF LLM used by the built-in (llama.cpp) summary engine.
    Gguf,
}

impl ModelKind {
    /// Sub-directory under the models directory where files of this kind live.
    /// Whisper files sit directly in the models directory (upstream layout);
    /// GGUF files live in `summary/`.
    pub fn subdir(self) -> &'static str {
        match self {
            ModelKind::WhisperGgml => "",
            ModelKind::Gguf => "summary",
        }
    }

    pub fn file_matches(self, file_name: &str) -> bool {
        let lower = file_name.to_ascii_lowercase();
        match self {
            ModelKind::WhisperGgml => lower.ends_with(".bin"),
            ModelKind::Gguf => lower.ends_with(".gguf"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelOrigin {
    BuiltIn,
    User,
    Discovered,
}

fn default_origin() -> ModelOrigin {
    ModelOrigin::User
}

/// One registered model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistryEntry {
    /// Stable identifier used by settings and the engines
    /// (e.g. `large-v3-turbo`, `ivrit-large-v3-turbo`, `qwen3.5:4b`).
    pub id: String,
    pub kind: ModelKind,
    pub display_name: String,
    /// File name inside the kind's directory.
    pub file_name: String,
    #[serde(default)]
    pub download_url: Option<String>,
    /// Expected size in MiB when known. `None` means "accept any plausible file".
    #[serde(default)]
    pub size_mb: Option<u64>,
    #[serde(default)]
    pub sha256: Option<String>,
    /// BCP-47 language hints (`["he"]`). Empty means multilingual / unknown.
    #[serde(default)]
    pub languages: Vec<String>,
    #[serde(default)]
    pub description: String,
    #[serde(default = "default_origin")]
    pub origin: ModelOrigin,

    // Whisper-only metadata (display purposes).
    #[serde(default)]
    pub accuracy: Option<String>,
    #[serde(default)]
    pub speed: Option<String>,

    // GGUF-only metadata.
    /// Prompt template name. `None` or `"auto"` means use the chat template
    /// embedded in the GGUF file.
    #[serde(default)]
    pub template: Option<String>,
    #[serde(default)]
    pub context_size: Option<u32>,
    #[serde(default)]
    pub layer_count: Option<u32>,
    /// Sampling preset name (`tight_structured`, `qwen35_summary`, `gemma3_instruct`).
    #[serde(default)]
    pub sampling: Option<String>,
}

impl RegistryEntry {
    pub fn is_builtin(&self) -> bool {
        self.origin == ModelOrigin::BuiltIn
    }

    pub fn supports_language(&self, code: &str) -> bool {
        self.languages.is_empty()
            || self
                .languages
                .iter()
                .any(|l| l.eq_ignore_ascii_case(code))
    }
}

/// Fields a caller may supply when adding or editing an entry.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EntryOverrides {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub file_name: Option<String>,
    #[serde(default)]
    pub languages: Option<Vec<String>>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub size_mb: Option<u64>,
    #[serde(default)]
    pub sha256: Option<String>,
    #[serde(default)]
    pub template: Option<String>,
    #[serde(default)]
    pub context_size: Option<u32>,
    #[serde(default)]
    pub layer_count: Option<u32>,
    #[serde(default)]
    pub sampling: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct RegistryFile {
    version: u32,
    models: Vec<RegistryEntry>,
}

/// On-disk + in-memory registry.
#[derive(Debug)]
pub struct Registry {
    base_dir: PathBuf,
    user_entries: Vec<RegistryEntry>,
}

impl Registry {
    /// Load the registry from `<base_dir>/registry.json` (missing file = empty).
    pub fn load(base_dir: PathBuf) -> Self {
        let path = base_dir.join(REGISTRY_FILE);
        let user_entries = match std::fs::read_to_string(&path) {
            Ok(text) => match serde_json::from_str::<RegistryFile>(&text) {
                Ok(file) => file
                    .models
                    .into_iter()
                    .map(|mut e| {
                        e.origin = ModelOrigin::User;
                        e
                    })
                    .collect(),
                Err(e) => {
                    log::error!("Model registry at {} is unreadable ({}); ignoring it", path.display(), e);
                    Vec::new()
                }
            },
            Err(_) => Vec::new(),
        };
        log::info!(
            "Model registry loaded from {} ({} user entries)",
            base_dir.display(),
            user_entries.len()
        );
        Self { base_dir, user_entries }
    }

    pub fn base_dir(&self) -> &Path {
        &self.base_dir
    }

    pub fn dir_for(&self, kind: ModelKind) -> PathBuf {
        let sub = kind.subdir();
        if sub.is_empty() {
            self.base_dir.clone()
        } else {
            self.base_dir.join(sub)
        }
    }

    pub fn path_for(&self, entry: &RegistryEntry) -> PathBuf {
        self.dir_for(entry.kind).join(&entry.file_name)
    }

    fn save(&self) -> Result<()> {
        std::fs::create_dir_all(&self.base_dir)?;
        let path = self.base_dir.join(REGISTRY_FILE);
        let tmp = self.base_dir.join(format!("{}.tmp", REGISTRY_FILE));
        let file = RegistryFile {
            version: REGISTRY_VERSION,
            models: self.user_entries.clone(),
        };
        std::fs::write(&tmp, serde_json::to_string_pretty(&file)?)?;
        std::fs::rename(&tmp, &path)?;
        Ok(())
    }

    /// All entries of a kind: built-in, then user, then discovered files.
    pub fn entries(&self, kind: ModelKind) -> Vec<RegistryEntry> {
        let mut out: Vec<RegistryEntry> = builtin_entries(kind);
        let mut ids: HashSet<String> = out.iter().map(|e| e.id.clone()).collect();
        let mut files: HashSet<String> = out.iter().map(|e| e.file_name.clone()).collect();

        for entry in self.user_entries.iter().filter(|e| e.kind == kind) {
            if ids.contains(&entry.id) || files.contains(&entry.file_name) {
                // A user entry shadowing a built-in one is a leftover; skip it.
                continue;
            }
            ids.insert(entry.id.clone());
            files.insert(entry.file_name.clone());
            out.push(entry.clone());
        }

        for entry in discover_files(&self.dir_for(kind), kind) {
            if files.contains(&entry.file_name) {
                continue;
            }
            let mut entry = entry;
            entry.id = unique_id(&entry.id, &ids);
            ids.insert(entry.id.clone());
            files.insert(entry.file_name.clone());
            out.push(entry);
        }
        out
    }

    pub fn find(&self, kind: ModelKind, id: &str) -> Option<RegistryEntry> {
        self.entries(kind).into_iter().find(|e| e.id == id)
    }

    /// Register a model that will be downloaded from `url`.
    pub fn add_from_url(
        &mut self,
        kind: ModelKind,
        url: &str,
        overrides: EntryOverrides,
    ) -> Result<RegistryEntry> {
        let url = url.trim();
        if !(url.starts_with("https://") || url.starts_with("http://")) {
            return Err(anyhow!("Model URL must start with http:// or https://"));
        }
        let basename = url
            .split('?')
            .next()
            .unwrap_or(url)
            .rsplit('/')
            .next()
            .unwrap_or("")
            .to_string();
        if basename.is_empty() {
            return Err(anyhow!("Could not determine a file name from the URL"));
        }
        if !kind.file_matches(&basename) && overrides.file_name.is_none() {
            return Err(anyhow!(
                "URL does not point at a {} file: {}",
                kind_label(kind),
                basename
            ));
        }

        let existing = self.entries(kind);
        let file_name = match overrides.file_name.clone() {
            Some(name) => sanitize_file_name(&name),
            None => derive_file_name(kind, url, &basename, &existing),
        };
        if !kind.file_matches(&file_name) {
            return Err(anyhow!("File name must end with {}", match kind {
                ModelKind::WhisperGgml => ".bin",
                ModelKind::Gguf => ".gguf",
            }));
        }
        if existing.iter().any(|e| e.file_name == file_name) {
            return Err(anyhow!("A model with file name '{}' already exists", file_name));
        }

        let ids: HashSet<String> = existing.iter().map(|e| e.id.clone()).collect();
        let id = match overrides.id.clone() {
            Some(id) => {
                let id = slugify(&id);
                if ids.contains(&id) {
                    return Err(anyhow!("A model with id '{}' already exists", id));
                }
                id
            }
            None => unique_id(&id_from_file_name(kind, &file_name), &ids),
        };

        let entry = RegistryEntry {
            id,
            kind,
            display_name: overrides
                .display_name
                .clone()
                .filter(|s| !s.trim().is_empty())
                .unwrap_or_else(|| display_name_from_file(kind, &file_name)),
            file_name,
            download_url: Some(url.to_string()),
            size_mb: overrides.size_mb,
            sha256: overrides.sha256.clone(),
            languages: overrides.languages.clone().unwrap_or_default(),
            description: overrides
                .description
                .clone()
                .unwrap_or_else(|| format!("Added from {}", url)),
            origin: ModelOrigin::User,
            accuracy: None,
            speed: None,
            template: overrides.template.clone(),
            context_size: overrides.context_size,
            layer_count: overrides.layer_count,
            sampling: overrides.sampling.clone(),
        };
        self.user_entries.push(entry.clone());
        self.save()?;
        Ok(entry)
    }

    /// Copy a local file into the models directory and register it.
    pub fn import_local(
        &mut self,
        kind: ModelKind,
        source: &Path,
        overrides: EntryOverrides,
    ) -> Result<RegistryEntry> {
        if !source.is_file() {
            return Err(anyhow!("{} is not a file", source.display()));
        }
        let basename = source
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(|| anyhow!("Invalid file name"))?
            .to_string();
        let file_name = match overrides.file_name.clone() {
            Some(name) => sanitize_file_name(&name),
            None => sanitize_file_name(&basename),
        };
        if !kind.file_matches(&file_name) {
            return Err(anyhow!("{} is not a {} file", basename, kind_label(kind)));
        }

        let dir = self.dir_for(kind);
        std::fs::create_dir_all(&dir)?;
        let target = dir.join(&file_name);
        let already_in_place = source
            .canonicalize()
            .ok()
            .zip(target.canonicalize().ok())
            .map(|(a, b)| a == b)
            .unwrap_or(false);

        let existing = self.entries(kind);
        if !already_in_place {
            if target.exists() {
                return Err(anyhow!("A file named '{}' already exists in the models folder", file_name));
            }
            // Prefer a rename (instant on the same volume), fall back to a copy.
            if std::fs::rename(source, &target).is_err() {
                std::fs::copy(source, &target)?;
            }
        } else if existing.iter().any(|e| e.file_name == file_name && e.origin != ModelOrigin::Discovered) {
            return Err(anyhow!("'{}' is already registered", file_name));
        }

        let ids: HashSet<String> = existing
            .iter()
            .filter(|e| e.file_name != file_name)
            .map(|e| e.id.clone())
            .collect();
        let id = match overrides.id.clone() {
            Some(id) => {
                let id = slugify(&id);
                if ids.contains(&id) {
                    return Err(anyhow!("A model with id '{}' already exists", id));
                }
                id
            }
            None => unique_id(&id_from_file_name(kind, &file_name), &ids),
        };
        let size_mb = std::fs::metadata(&target).ok().map(|m| m.len() / (1024 * 1024));

        let entry = RegistryEntry {
            id,
            kind,
            display_name: overrides
                .display_name
                .clone()
                .filter(|s| !s.trim().is_empty())
                .unwrap_or_else(|| display_name_from_file(kind, &file_name)),
            file_name,
            download_url: None,
            size_mb: overrides.size_mb.or(size_mb),
            sha256: overrides.sha256.clone(),
            languages: overrides.languages.clone().unwrap_or_default(),
            description: overrides
                .description
                .clone()
                .unwrap_or_else(|| "Imported local model".to_string()),
            origin: ModelOrigin::User,
            accuracy: None,
            speed: None,
            template: overrides.template.clone(),
            context_size: overrides.context_size,
            layer_count: overrides.layer_count,
            sampling: overrides.sampling.clone(),
        };
        self.user_entries.retain(|e| !(e.kind == kind && e.file_name == entry.file_name));
        self.user_entries.push(entry.clone());
        self.save()?;
        Ok(entry)
    }

    /// Edit metadata of a user (or discovered) entry. Discovered entries are
    /// promoted to user entries so the edit persists.
    pub fn update(&mut self, kind: ModelKind, id: &str, overrides: EntryOverrides) -> Result<RegistryEntry> {
        let current = self
            .find(kind, id)
            .ok_or_else(|| anyhow!("Model '{}' not found", id))?;
        if current.is_builtin() {
            return Err(anyhow!("Built-in models cannot be edited"));
        }
        let mut updated = current.clone();
        updated.origin = ModelOrigin::User;
        if let Some(v) = overrides.display_name {
            if !v.trim().is_empty() {
                updated.display_name = v;
            }
        }
        if let Some(v) = overrides.languages {
            updated.languages = v;
        }
        if let Some(v) = overrides.description {
            updated.description = v;
        }
        if overrides.size_mb.is_some() {
            updated.size_mb = overrides.size_mb;
        }
        if overrides.sha256.is_some() {
            updated.sha256 = overrides.sha256;
        }
        if overrides.template.is_some() {
            updated.template = overrides.template;
        }
        if overrides.context_size.is_some() {
            updated.context_size = overrides.context_size;
        }
        if overrides.layer_count.is_some() {
            updated.layer_count = overrides.layer_count;
        }
        if overrides.sampling.is_some() {
            updated.sampling = overrides.sampling;
        }
        self.user_entries
            .retain(|e| !(e.kind == kind && (e.id == id || e.file_name == updated.file_name)));
        self.user_entries.push(updated.clone());
        self.save()?;
        Ok(updated)
    }

    /// Remove a user/discovered entry, optionally deleting its file.
    pub fn remove(&mut self, kind: ModelKind, id: &str, delete_file: bool) -> Result<()> {
        let entry = self
            .find(kind, id)
            .ok_or_else(|| anyhow!("Model '{}' not found", id))?;
        if entry.is_builtin() {
            return Err(anyhow!("Built-in models cannot be removed from the registry; delete the file instead"));
        }
        if delete_file || entry.origin == ModelOrigin::Discovered {
            let path = self.path_for(&entry);
            if path.exists() {
                std::fs::remove_file(&path)?;
            }
        }
        self.user_entries.retain(|e| !(e.kind == kind && e.id == id));
        self.save()?;
        Ok(())
    }
}

// ============================================================================
// Global instance
// ============================================================================

static REGISTRY: LazyLock<RwLock<Option<Registry>>> = LazyLock::new(|| RwLock::new(None));

/// Initialise the global registry. Called once at app start with the models directory.
pub fn init(models_dir: PathBuf) {
    let registry = Registry::load(models_dir);
    let mut guard = REGISTRY.write().unwrap_or_else(|p| p.into_inner());
    *guard = Some(registry);
}

pub fn is_initialized() -> bool {
    REGISTRY
        .read()
        .map(|g| g.is_some())
        .unwrap_or(false)
}

pub fn with<R>(f: impl FnOnce(&Registry) -> R) -> Result<R> {
    let guard = REGISTRY.read().unwrap_or_else(|p| p.into_inner());
    match guard.as_ref() {
        Some(reg) => Ok(f(reg)),
        None => Err(anyhow!("Model registry not initialised")),
    }
}

pub fn with_mut<R>(f: impl FnOnce(&mut Registry) -> Result<R>) -> Result<R> {
    let mut guard = REGISTRY.write().unwrap_or_else(|p| p.into_inner());
    match guard.as_mut() {
        Some(reg) => f(reg),
        None => Err(anyhow!("Model registry not initialised")),
    }
}

/// Entries of a kind. Before `init` runs (unit tests, early startup) only the
/// built-in entries are returned, so callers never need to special-case it.
pub fn entries(kind: ModelKind) -> Vec<RegistryEntry> {
    with(|r| r.entries(kind)).unwrap_or_else(|_| builtin_entries(kind))
}

pub fn find(kind: ModelKind, id: &str) -> Option<RegistryEntry> {
    entries(kind).into_iter().find(|e| e.id == id)
}

// ============================================================================
// Built-in catalogs
// ============================================================================

/// Hebrew whisper.cpp models published by ivrit.ai (official GGML conversions).
const IVRIT_WHISPER_MODELS: &[(&str, &str, &str, &str, u64, &str)] = &[
    (
        "ivrit-large-v3-turbo",
        "ivrit.ai Whisper Large v3 Turbo (Hebrew)",
        "ggml-ivrit-large-v3-turbo.bin",
        "https://huggingface.co/ivrit-ai/whisper-large-v3-turbo-ggml/resolve/main/ggml-model.bin",
        1549,
        "Hebrew fine-tune of Whisper large-v3-turbo by ivrit.ai. Best choice for Hebrew and mixed Hebrew/English meetings.",
    ),
    (
        "ivrit-large-v3",
        "ivrit.ai Whisper Large v3 (Hebrew)",
        "ggml-ivrit-large-v3.bin",
        "https://huggingface.co/ivrit-ai/whisper-large-v3-ggml/resolve/main/ggml-model.bin",
        2952,
        "Hebrew fine-tune of Whisper large-v3 by ivrit.ai. Highest Hebrew accuracy, slower than turbo.",
    ),
];

pub fn builtin_entries(kind: ModelKind) -> Vec<RegistryEntry> {
    match kind {
        ModelKind::WhisperGgml => {
            let mut out: Vec<RegistryEntry> = crate::config::WHISPER_MODEL_CATALOG
                .iter()
                .map(|&(name, filename, size_mb, accuracy, speed, description)| RegistryEntry {
                    id: name.to_string(),
                    kind,
                    display_name: name.to_string(),
                    file_name: filename.to_string(),
                    download_url: Some(format!(
                        "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/{}",
                        filename
                    )),
                    size_mb: Some(size_mb as u64),
                    sha256: None,
                    languages: Vec::new(),
                    description: description.to_string(),
                    origin: ModelOrigin::BuiltIn,
                    accuracy: Some(accuracy.to_string()),
                    speed: Some(speed.to_string()),
                    template: None,
                    context_size: None,
                    layer_count: None,
                    sampling: None,
                })
                .collect();
            out.extend(IVRIT_WHISPER_MODELS.iter().map(
                |&(id, display_name, file_name, url, size_mb, description)| RegistryEntry {
                    id: id.to_string(),
                    kind,
                    display_name: display_name.to_string(),
                    file_name: file_name.to_string(),
                    download_url: Some(url.to_string()),
                    size_mb: Some(size_mb),
                    sha256: None,
                    languages: vec!["he".to_string()],
                    description: description.to_string(),
                    origin: ModelOrigin::BuiltIn,
                    accuracy: Some("High".to_string()),
                    speed: Some(if id.ends_with("turbo") { "Medium" } else { "Slow" }.to_string()),
                    template: None,
                    context_size: None,
                    layer_count: None,
                    sampling: None,
                },
            ));
            out
        }
        ModelKind::Gguf => crate::summary::summary_engine::models::builtin_registry_entries(),
    }
}

// ============================================================================
// Helpers
// ============================================================================

fn kind_label(kind: ModelKind) -> &'static str {
    match kind {
        ModelKind::WhisperGgml => "whisper.cpp GGML (.bin)",
        ModelKind::Gguf => "GGUF (.gguf)",
    }
}

fn discover_files(dir: &Path, kind: ModelKind) -> Vec<RegistryEntry> {
    let Ok(read_dir) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for entry in read_dir.flatten() {
        let Ok(file_type) = entry.file_type() else { continue };
        if !file_type.is_file() {
            continue;
        }
        let Some(file_name) = entry.file_name().to_str().map(|s| s.to_string()) else {
            continue;
        };
        if !kind.file_matches(&file_name) || file_name.starts_with('.') {
            continue;
        }
        let size_mb = entry.metadata().ok().map(|m| m.len() / (1024 * 1024));
        out.push(RegistryEntry {
            id: id_from_file_name(kind, &file_name),
            kind,
            display_name: display_name_from_file(kind, &file_name),
            file_name,
            download_url: None,
            size_mb,
            sha256: None,
            languages: Vec::new(),
            description: "Found in the models folder".to_string(),
            origin: ModelOrigin::Discovered,
            accuracy: None,
            speed: None,
            template: None,
            context_size: None,
            layer_count: None,
            sampling: None,
        });
    }
    out.sort_by(|a, b| a.file_name.cmp(&b.file_name));
    out
}

pub fn slugify(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut last_dash = false;
    for ch in input.trim().chars() {
        let c = ch.to_ascii_lowercase();
        if c.is_ascii_alphanumeric() || c == '.' || c == ':' {
            out.push(c);
            last_dash = false;
        } else if !last_dash && !out.is_empty() {
            out.push('-');
            last_dash = true;
        }
    }
    let trimmed = out.trim_matches('-').to_string();
    if trimmed.is_empty() {
        "model".to_string()
    } else {
        trimmed
    }
}

fn file_stem(file_name: &str) -> &str {
    file_name.rsplit_once('.').map(|(stem, _)| stem).unwrap_or(file_name)
}

fn id_from_file_name(kind: ModelKind, file_name: &str) -> String {
    let stem = file_stem(file_name);
    let stem = match kind {
        ModelKind::WhisperGgml => stem.strip_prefix("ggml-").unwrap_or(stem),
        ModelKind::Gguf => stem,
    };
    slugify(stem)
}

fn display_name_from_file(kind: ModelKind, file_name: &str) -> String {
    let stem = file_stem(file_name);
    match kind {
        ModelKind::WhisperGgml => stem.strip_prefix("ggml-").unwrap_or(stem).to_string(),
        ModelKind::Gguf => stem.to_string(),
    }
}

fn unique_id(base: &str, taken: &HashSet<String>) -> String {
    if !taken.contains(base) {
        return base.to_string();
    }
    let mut n = 2;
    loop {
        let candidate = format!("{}-{}", base, n);
        if !taken.contains(&candidate) {
            return candidate;
        }
        n += 1;
    }
}

fn sanitize_file_name(name: &str) -> String {
    let cleaned: String = name
        .trim()
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_') {
                c
            } else {
                '_'
            }
        })
        .collect();
    cleaned.trim_matches('.').to_string()
}

/// Pick a file name for a URL download. Hugging Face repos often use a
/// generic `ggml-model.bin` / `model.gguf`, so fold the repo name in when the
/// basename is generic or already taken.
fn derive_file_name(kind: ModelKind, url: &str, basename: &str, existing: &[RegistryEntry]) -> String {
    let basename = sanitize_file_name(basename);
    let generic = matches!(
        basename.to_ascii_lowercase().as_str(),
        "ggml-model.bin" | "model.bin" | "model.gguf" | "ggml.bin"
    );
    let taken = |name: &str| existing.iter().any(|e| e.file_name == name);

    if !generic && !taken(&basename) {
        return basename;
    }

    // Try "<repo>-<basename>" using the Hugging Face path (owner/repo/resolve/...)
    let repo = url
        .split("://")
        .nth(1)
        .and_then(|rest| {
            let mut parts = rest.split('/').skip(1); // skip host
            let owner = parts.next()?;
            let repo = parts.next()?;
            Some(format!("{}-{}", slugify(owner), slugify(repo)))
        })
        .unwrap_or_else(|| "custom".to_string());

    let (stem, ext) = basename
        .rsplit_once('.')
        .map(|(s, e)| (s.to_string(), e.to_string()))
        .unwrap_or((basename.clone(), String::new()));
    let prefix = match kind {
        ModelKind::WhisperGgml => "ggml-",
        ModelKind::Gguf => "",
    };
    let stem = stem.strip_prefix("ggml-").unwrap_or(&stem);
    let mut candidate = format!("{}{}-{}.{}", prefix, repo, stem, ext);
    let mut n = 2;
    while taken(&candidate) {
        candidate = format!("{}{}-{}-{}.{}", prefix, repo, stem, n, ext);
        n += 1;
    }
    candidate
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_whisper_entries_include_upstream_and_ivrit_models() {
        let entries = builtin_entries(ModelKind::WhisperGgml);
        assert!(entries.iter().any(|e| e.id == "tiny"));
        assert!(entries.iter().any(|e| e.id == "large-v3-turbo"));
        let ivrit = entries
            .iter()
            .find(|e| e.id == "ivrit-large-v3-turbo")
            .expect("ivrit entry");
        assert_eq!(ivrit.languages, vec!["he"]);
        assert!(ivrit.supports_language("he"));
        assert!(!ivrit.supports_language("en"));
        assert!(entries.iter().find(|e| e.id == "tiny").unwrap().supports_language("he"));
    }

    #[test]
    fn add_from_url_derives_unique_names_for_generic_basenames() {
        let dir = tempfile::tempdir().unwrap();
        let mut reg = Registry::load(dir.path().to_path_buf());
        let entry = reg
            .add_from_url(
                ModelKind::WhisperGgml,
                "https://huggingface.co/ivrit-ai/yi-whisper-large-v3-turbo-ggml/resolve/main/ggml-model.bin",
                EntryOverrides { languages: Some(vec!["he".into()]), ..Default::default() },
            )
            .unwrap();
        assert_eq!(entry.file_name, "ggml-ivrit-ai-yi-whisper-large-v3-turbo-ggml-model.bin");
        assert_eq!(entry.id, "ivrit-ai-yi-whisper-large-v3-turbo-ggml-model");
        assert_eq!(entry.origin, ModelOrigin::User);

        // Persisted and reloaded
        let reloaded = Registry::load(dir.path().to_path_buf());
        assert!(reloaded.find(ModelKind::WhisperGgml, &entry.id).is_some());

        // A second URL with the same basename gets a distinct file name automatically
        let second = reg
            .add_from_url(
                ModelKind::WhisperGgml,
                "https://example.com/x/ggml-ivrit-ai-yi-whisper-large-v3-turbo-ggml-model.bin",
                EntryOverrides::default(),
            )
            .unwrap();
        assert_ne!(second.file_name, entry.file_name);
        assert_ne!(second.id, entry.id);

        // An explicit file name that is already taken is rejected
        let dup = reg.add_from_url(
            ModelKind::WhisperGgml,
            "https://example.com/y/ggml-other.bin",
            EntryOverrides { file_name: Some(entry.file_name.clone()), ..Default::default() },
        );
        assert!(dup.is_err());
    }

    #[test]
    fn add_from_url_rejects_wrong_extension_and_scheme() {
        let dir = tempfile::tempdir().unwrap();
        let mut reg = Registry::load(dir.path().to_path_buf());
        assert!(reg
            .add_from_url(ModelKind::Gguf, "https://example.com/model.bin", EntryOverrides::default())
            .is_err());
        assert!(reg
            .add_from_url(ModelKind::Gguf, "ftp://example.com/model.gguf", EntryOverrides::default())
            .is_err());
    }

    #[test]
    fn discovered_files_show_up_and_can_be_removed() {
        let dir = tempfile::tempdir().unwrap();
        let summary = dir.path().join("summary");
        std::fs::create_dir_all(&summary).unwrap();
        std::fs::write(summary.join("DictaLM-2.0-Q4_K_M.gguf"), [0u8; 16]).unwrap();
        std::fs::write(dir.path().join("ggml-custom.bin"), [0u8; 16]).unwrap();

        let mut reg = Registry::load(dir.path().to_path_buf());
        let gguf = reg.entries(ModelKind::Gguf);
        let found = gguf.iter().find(|e| e.file_name == "DictaLM-2.0-Q4_K_M.gguf").unwrap();
        assert_eq!(found.origin, ModelOrigin::Discovered);
        assert_eq!(found.id, "dictalm-2.0-q4-k-m");

        let whisper = reg.entries(ModelKind::WhisperGgml);
        let custom = whisper.iter().find(|e| e.file_name == "ggml-custom.bin").unwrap();
        assert_eq!(custom.id, "custom");

        // Editing a discovered entry promotes it to a user entry
        let updated = reg
            .update(
                ModelKind::Gguf,
                "dictalm-2.0-q4-k-m",
                EntryOverrides { languages: Some(vec!["he".into()]), context_size: Some(4096), ..Default::default() },
            )
            .unwrap();
        assert_eq!(updated.origin, ModelOrigin::User);
        assert_eq!(updated.context_size, Some(4096));

        reg.remove(ModelKind::Gguf, "dictalm-2.0-q4-k-m", true).unwrap();
        assert!(!summary.join("DictaLM-2.0-Q4_K_M.gguf").exists());
        assert!(reg.find(ModelKind::Gguf, "dictalm-2.0-q4-k-m").is_none());
    }

    #[test]
    fn import_local_moves_file_into_models_dir() {
        let dir = tempfile::tempdir().unwrap();
        let src_dir = tempfile::tempdir().unwrap();
        let src = src_dir.path().join("ggml-hebrew.bin");
        std::fs::write(&src, [1u8; 2048]).unwrap();

        let mut reg = Registry::load(dir.path().to_path_buf());
        let entry = reg
            .import_local(ModelKind::WhisperGgml, &src, EntryOverrides::default())
            .unwrap();
        assert_eq!(entry.id, "hebrew");
        assert!(dir.path().join("ggml-hebrew.bin").exists());
        assert!(reg.find(ModelKind::WhisperGgml, "hebrew").is_some());

        assert!(reg.remove(ModelKind::WhisperGgml, "tiny", false).is_err(), "built-ins are protected");
    }

    #[test]
    fn slugify_normalises() {
        assert_eq!(slugify("Qwen3.5-4B Q4_K_M"), "qwen3.5-4b-q4-k-m");
        assert_eq!(slugify("  --weird__name-- "), "weird-name");
        assert_eq!(slugify(""), "model");
    }
}
