//! Remembered voices: an opt-in, local JSON file of named voice signatures.
//!
//! A profile is created only when the user names a speaker. From then on a
//! matching voice in any meeting is labelled with that name automatically.
//! The file lives in the app data directory (`speaker_profiles.json`) and
//! never leaves the machine; profiles can be deleted from Settings.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};

pub const PROFILES_FILE: &str = "speaker_profiles.json";
const FILE_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoiceProfile {
    pub id: String,
    pub name: String,
    /// L2-normalised mean embedding of every clip merged into this profile.
    pub embedding: Vec<f32>,
    /// How many clips contributed (weights later merges).
    pub samples: u32,
    /// This is the user's own voice: labelled "You" wherever it is heard.
    #[serde(default)]
    pub is_me: bool,
    pub created_at: String,
    pub updated_at: String,
}

/// What the UI gets (no embedding).
#[derive(Debug, Clone, Serialize)]
pub struct VoiceProfileSummary {
    pub id: String,
    pub name: String,
    pub samples: u32,
    pub is_me: bool,
    pub updated_at: String,
}

/// Names that mean "this speaker is me" when typed into the rename box.
const ME_ALIASES: &[&str] = &["me", "you", "myself", "אני", "זה אני"];

pub fn is_me_alias(name: &str) -> bool {
    let name = name.trim().trim_end_matches('.').to_lowercase();
    ME_ALIASES.contains(&name.as_str())
}

#[derive(Debug, Serialize, Deserialize)]
struct ProfilesFile {
    version: u32,
    profiles: Vec<VoiceProfile>,
}

#[derive(Debug)]
pub struct ProfileStore {
    path: PathBuf,
    profiles: Vec<VoiceProfile>,
}

fn now() -> String {
    chrono::Utc::now().to_rfc3339()
}

fn normalise(v: &mut [f32]) {
    let norm = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm > 0.0 {
        for x in v.iter_mut() {
            *x /= norm;
        }
    }
}

fn cosine(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() {
        return -1.0;
    }
    a.iter().zip(b.iter()).map(|(x, y)| x * y).sum()
}

impl ProfileStore {
    pub fn load(path: PathBuf) -> Self {
        let profiles = match std::fs::read_to_string(&path) {
            Ok(text) => match serde_json::from_str::<ProfilesFile>(&text) {
                Ok(file) => file.profiles,
                Err(e) => {
                    log::error!("Voice profiles file {} is unreadable ({}); starting empty", path.display(), e);
                    Vec::new()
                }
            },
            Err(_) => Vec::new(),
        };
        if !profiles.is_empty() {
            log::info!("Loaded {} remembered voice(s) from {}", profiles.len(), path.display());
        }
        Self { path, profiles }
    }

    fn save(&self) -> Result<()> {
        if let Some(dir) = self.path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let tmp = self.path.with_extension("json.tmp");
        let file = ProfilesFile { version: FILE_VERSION, profiles: self.profiles.clone() };
        std::fs::write(&tmp, serde_json::to_string_pretty(&file)?)?;
        std::fs::rename(&tmp, &self.path)?;
        Ok(())
    }

    pub fn list(&self) -> Vec<VoiceProfileSummary> {
        self.profiles
            .iter()
            .map(|p| VoiceProfileSummary {
                id: p.id.clone(),
                name: p.name.clone(),
                samples: p.samples,
                is_me: p.is_me,
                updated_at: p.updated_at.clone(),
            })
            .collect()
    }

    pub fn is_empty(&self) -> bool {
        self.profiles.is_empty()
    }

    pub fn has_me(&self) -> bool {
        self.profiles.iter().any(|p| p.is_me)
    }

    pub fn get(&self, id: &str) -> Option<&VoiceProfile> {
        self.profiles.iter().find(|p| p.id == id)
    }

    /// Mark one remembered voice as the user's own (clearing any other).
    pub fn set_me(&mut self, id: &str, is_me: bool) -> Result<VoiceProfile> {
        if !self.profiles.iter().any(|p| p.id == id) {
            return Err(anyhow!("Voice profile not found"));
        }
        for p in self.profiles.iter_mut() {
            let value = if p.id == id { is_me } else if is_me { false } else { p.is_me };
            if p.is_me != value {
                p.is_me = value;
                p.updated_at = now();
            }
        }
        self.save()?;
        Ok(self.get(id).cloned().expect("checked above"))
    }

    /// Merge this voice into the user's own profile, creating it when needed.
    pub fn upsert_me(&mut self, embedding: &[f32], samples: u32) -> Result<VoiceProfile> {
        let samples = samples.max(1);
        if let Some(existing) = self.profiles.iter_mut().find(|p| p.is_me && p.embedding.len() == embedding.len()) {
            let total = existing.samples as f32 + samples as f32;
            for (c, e) in existing.embedding.iter_mut().zip(embedding.iter()) {
                *c = (*c * existing.samples as f32 + e * samples as f32) / total;
            }
            normalise(&mut existing.embedding);
            existing.samples += samples;
            existing.updated_at = now();
            let profile = existing.clone();
            self.save()?;
            return Ok(profile);
        }
        let mut vec = embedding.to_vec();
        normalise(&mut vec);
        let profile = VoiceProfile {
            id: uuid::Uuid::new_v4().to_string(),
            name: "Me".to_string(),
            embedding: vec,
            samples,
            is_me: true,
            created_at: now(),
            updated_at: now(),
        };
        self.profiles.push(profile.clone());
        self.save()?;
        Ok(profile)
    }

    /// Closest remembered voice and its similarity.
    pub fn best_match(&self, embedding: &[f32]) -> Option<(VoiceProfile, f32)> {
        self.profiles
            .iter()
            .map(|p| (p, cosine(embedding, &p.embedding)))
            .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
            .map(|(p, s)| (p.clone(), s))
    }

    /// Create or merge a profile with this name. Returns the stored profile.
    pub fn upsert_named(&mut self, name: &str, embedding: &[f32], samples: u32) -> Result<VoiceProfile> {
        let name = name.trim();
        if name.is_empty() {
            return Err(anyhow!("Name must not be empty"));
        }
        let samples = samples.max(1);
        let profile = if let Some(existing) = self
            .profiles
            .iter_mut()
            .find(|p| p.name.eq_ignore_ascii_case(name) && p.embedding.len() == embedding.len())
        {
            let total = existing.samples as f32 + samples as f32;
            for (c, e) in existing.embedding.iter_mut().zip(embedding.iter()) {
                *c = (*c * existing.samples as f32 + e * samples as f32) / total;
            }
            normalise(&mut existing.embedding);
            existing.samples += samples;
            existing.updated_at = now();
            existing.clone()
        } else {
            let mut vec = embedding.to_vec();
            normalise(&mut vec);
            let profile = VoiceProfile {
                id: uuid::Uuid::new_v4().to_string(),
                name: name.to_string(),
                embedding: vec,
                samples,
                is_me: false,
                created_at: now(),
                updated_at: now(),
            };
            self.profiles.push(profile.clone());
            profile
        };
        self.save()?;
        Ok(profile)
    }

    pub fn rename(&mut self, id: &str, name: &str) -> Result<()> {
        let name = name.trim();
        if name.is_empty() {
            return Err(anyhow!("Name must not be empty"));
        }
        let profile = self
            .profiles
            .iter_mut()
            .find(|p| p.id == id)
            .ok_or_else(|| anyhow!("Voice profile not found"))?;
        profile.name = name.to_string();
        profile.updated_at = now();
        self.save()
    }

    pub fn delete(&mut self, id: &str) -> Result<()> {
        let before = self.profiles.len();
        self.profiles.retain(|p| p.id != id);
        if self.profiles.len() == before {
            return Err(anyhow!("Voice profile not found"));
        }
        self.save()
    }

    pub fn clear(&mut self) -> Result<()> {
        self.profiles.clear();
        if self.path.exists() {
            std::fs::remove_file(&self.path)?;
        }
        Ok(())
    }
}

static STORE: Mutex<Option<ProfileStore>> = Mutex::new(None);

pub fn init(app_data_dir: &Path) {
    let store = ProfileStore::load(app_data_dir.join(PROFILES_FILE));
    if let Ok(mut guard) = STORE.lock() {
        *guard = Some(store);
    }
}

pub fn with<R>(f: impl FnOnce(&ProfileStore) -> R) -> Option<R> {
    let guard = STORE.lock().ok()?;
    guard.as_ref().map(f)
}

pub fn with_mut<R>(f: impl FnOnce(&mut ProfileStore) -> Result<R>) -> Result<R> {
    let mut guard = STORE.lock().map_err(|_| anyhow!("Voice profile store poisoned"))?;
    match guard.as_mut() {
        Some(store) => f(store),
        None => Err(anyhow!("Voice profiles not initialised")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn upsert_merges_by_name_and_persists() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(PROFILES_FILE);
        let mut store = ProfileStore::load(path.clone());
        assert!(store.is_empty());

        let a = store.upsert_named("Dana", &[1.0, 0.0, 0.0], 3).unwrap();
        let b = store.upsert_named("dana", &[0.0, 1.0, 0.0], 3).unwrap();
        assert_eq!(a.id, b.id, "same name merges");
        assert_eq!(b.samples, 6);
        let norm: f32 = b.embedding.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!((norm - 1.0).abs() < 1e-5);

        let reloaded = ProfileStore::load(path.clone());
        assert_eq!(reloaded.list().len(), 1);
        assert_eq!(reloaded.list()[0].name, "Dana");

        let (m, sim) = reloaded.best_match(&[0.7, 0.7, 0.0]).unwrap();
        assert_eq!(m.id, a.id);
        assert!(sim > 0.9);

        let mut reloaded = reloaded;
        reloaded.rename(&a.id, "Dana Levi").unwrap();
        assert_eq!(reloaded.list()[0].name, "Dana Levi");
        reloaded.delete(&a.id).unwrap();
        assert!(reloaded.is_empty());
        assert!(reloaded.delete("nope").is_err());
        reloaded.clear().unwrap();
        assert!(!path.exists());
    }

    #[test]
    fn me_profile_is_unique_and_merges() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = ProfileStore::load(dir.path().join(PROFILES_FILE));
        let dana = store.upsert_named("Dana", &[0.0, 1.0], 1).unwrap();
        let me = store.upsert_me(&[1.0, 0.0], 2).unwrap();
        assert!(store.has_me());
        let me2 = store.upsert_me(&[1.0, 0.1], 1).unwrap();
        assert_eq!(me.id, me2.id);
        assert_eq!(me2.samples, 3);
        // Marking Dana as me clears the previous one
        store.set_me(&dana.id, true).unwrap();
        assert!(store.get(&dana.id).unwrap().is_me);
        assert!(!store.get(&me.id).unwrap().is_me);
        store.set_me(&dana.id, false).unwrap();
        assert!(!store.has_me());
        assert!(store.set_me("nope", true).is_err());
    }

    #[test]
    fn empty_name_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = ProfileStore::load(dir.path().join(PROFILES_FILE));
        assert!(store.upsert_named("   ", &[1.0], 1).is_err());
    }
}
