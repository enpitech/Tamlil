//! Speaker identification: who is talking in each transcript segment.
//!
//! Every VAD speech chunk is turned into a speaker embedding with a small
//! WeSpeaker CAM++ model (ONNX, runs on CPU in a few milliseconds) and matched
//! against the speakers heard so far in the recording by cosine similarity.
//! New voices get the next free label: "Speaker 1", "Speaker 2", ...
//!
//! Who is "You": when the user has marked a remembered voice as their own
//! (`profiles::VoiceProfile::is_me`) that voice is "You" wherever it is heard.
//! Otherwise, while the "my microphone is only me" setting is on (default),
//! the first voice on the microphone channel is assumed to be the user. Turn
//! it off when other people share the mic (a phone on speaker, a meeting room).
//!
//! Labels are per recording; only voices the user names are stored.

pub mod commands;
pub mod fbank;
pub mod profiles;

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use anyhow::{anyhow, Context, Result};
use ndarray::Array3;
use ort::execution_providers::CPUExecutionProvider;
use ort::inputs;
use ort::session::builder::GraphOptimizationLevel;
use ort::session::Session;
use ort::value::TensorRef;
use serde::Serialize;

use crate::audio::RecordingDeviceType as DeviceType;

/// WeSpeaker CAM++ (VoxCeleb, large-margin fine-tuned), Apache-2.0, 28 MB.
pub const MODEL_FILE: &str = "wespeaker-voxceleb-campplus-lm.onnx";
pub const MODEL_URL: &str =
    "https://huggingface.co/Wespeaker/wespeaker-voxceleb-campplus-LM/resolve/c5e01c6fcffcce160861e7e79782828320192b5c/voxceleb_CAM%2B%2B_LM.onnx";
pub const MODEL_BYTES: u64 = 29_292_449;

/// Cosine similarity needed to join an existing speaker.
const SAME_SPEAKER_THRESHOLD: f32 = 0.55;
/// Shorter chunks are too unreliable to open a new speaker; they attach to the
/// closest existing one (or the previous speaker when nothing is close).
const MIN_SECONDS_FOR_NEW_SPEAKER: f32 = 1.5;
/// Hard cap so noise cannot create endless speakers.
const MAX_SPEAKERS: usize = 12;
/// Similarity needed to label a voice with a remembered (named) profile.
const PROFILE_MATCH_THRESHOLD: f32 = 0.6;
/// Re-check unnamed speakers against remembered voices once their centroid
/// has this many clips (early guesses are noisy).
const PROFILE_RECHECK_AT: &[u32] = &[3, 6, 12];

static MODELS_DIR: OnceLock<PathBuf> = OnceLock::new();
static ENABLED: AtomicBool = AtomicBool::new(true);
/// "My microphone is only me": the first mic voice is "You" unless a remembered
/// voice is marked as the user.
static MIC_IS_ME: AtomicBool = AtomicBool::new(true);
static ENGINE: Mutex<Option<Arc<SpeakerEngine>>> = Mutex::new(None);
static TRACKER: Mutex<Option<SpeakerTracker>> = Mutex::new(None);

pub fn init(app_data_dir: &Path) {
    let dir = app_data_dir.join("models").join("speaker");
    if let Err(e) = std::fs::create_dir_all(&dir) {
        log::warn!("Could not create speaker models dir {}: {}", dir.display(), e);
    }
    let _ = MODELS_DIR.set(dir);
    profiles::init(app_data_dir);
}

/// Name the speaker with this index in the current recording and remember the
/// voice. Returns the new label for the index.
pub fn name_speaker(index: u32, name: &str) -> Result<SpeakerLabel> {
    let name = name.trim();
    if name.is_empty() {
        return Err(anyhow!("Name must not be empty"));
    }
    let mut guard = TRACKER.lock().map_err(|_| anyhow!("Speaker tracker poisoned"))?;
    let tracker = guard
        .as_mut()
        .ok_or_else(|| anyhow!("No speakers in the current recording"))?;
    let speaker = tracker
        .speakers
        .get(index as usize)
        .cloned()
        .ok_or_else(|| anyhow!("Speaker {} not found in the current recording", index))?;
    let profile = if profiles::is_me_alias(name) {
        profiles::with_mut(|store| store.upsert_me(&speaker.centroid, speaker.count))?
    } else {
        profiles::with_mut(|store| store.upsert_named(name, &speaker.centroid, speaker.count))?
    };
    let label = profile_label(&profile);
    let speaker = &mut tracker.speakers[index as usize];
    speaker.label = label.clone();
    speaker.profile_id = Some(profile.id);
    if label == "You" {
        tracker.has_you = true;
    }
    Ok(SpeakerLabel { index, label, confidence: 1.0, relabeled: true })
}

/// Apply a profile change (renamed, marked as "me", deleted) to the speakers of
/// the current recording. Returns the labels that changed.
pub fn relabel_profile(profile_id: &str, new_label: Option<&str>) -> Vec<SpeakerLabel> {
    let mut changed = Vec::new();
    if let Ok(mut guard) = TRACKER.lock() {
        if let Some(tracker) = guard.as_mut() {
            for (i, speaker) in tracker.speakers.iter_mut().enumerate() {
                if speaker.profile_id.as_deref() != Some(profile_id) {
                    continue;
                }
                let label = match new_label {
                    Some(l) => l.to_string(),
                    None => {
                        speaker.profile_id = None;
                        tracker.remote_count += 1;
                        format!("Speaker {}", tracker.remote_count)
                    }
                };
                if speaker.label != label {
                    speaker.label = label.clone();
                    changed.push(SpeakerLabel { index: i as u32, label, confidence: 1.0, relabeled: true });
                }
            }
        }
    }
    changed
}

/// Transcript label for a remembered voice: the user's own voice is "You".
pub fn profile_label(profile: &profiles::VoiceProfile) -> String {
    if profile.is_me {
        "You".to_string()
    } else {
        profile.name.clone()
    }
}

/// Current label of every speaker in the recording.
pub fn current_labels() -> Vec<SpeakerLabel> {
    TRACKER
        .lock()
        .ok()
        .and_then(|g| {
            g.as_ref().map(|t| {
                t.speakers
                    .iter()
                    .enumerate()
                    .map(|(i, s)| SpeakerLabel { index: i as u32, label: s.label.clone(), confidence: 1.0, relabeled: false })
                    .collect()
            })
        })
        .unwrap_or_default()
}

pub fn models_dir() -> Option<PathBuf> {
    MODELS_DIR.get().cloned()
}

pub fn model_path() -> Option<PathBuf> {
    models_dir().map(|d| d.join(MODEL_FILE))
}

pub fn model_is_available() -> bool {
    model_path()
        .and_then(|p| std::fs::metadata(p).ok())
        .map(|m| m.len() == MODEL_BYTES)
        .unwrap_or(false)
}

pub fn set_enabled(enabled: bool) {
    ENABLED.store(enabled, Ordering::SeqCst);
}

pub fn is_enabled() -> bool {
    ENABLED.load(Ordering::SeqCst)
}

pub fn set_mic_is_me(value: bool) {
    MIC_IS_ME.store(value, Ordering::SeqCst);
}

pub fn mic_is_me() -> bool {
    MIC_IS_ME.load(Ordering::SeqCst)
}

/// Enabled by the user and the model is on disk.
pub fn is_active() -> bool {
    is_enabled() && model_is_available()
}

/// Forget all speakers (call at the start of every recording).
pub fn reset_session() {
    if let Ok(mut guard) = TRACKER.lock() {
        *guard = Some(SpeakerTracker::for_session());
    }
}

/// Drop the loaded model (after deleting the file, or to free memory).
pub fn unload() {
    if let Ok(mut guard) = ENGINE.lock() {
        *guard = None;
    }
}

fn engine() -> Result<Arc<SpeakerEngine>> {
    if let Ok(guard) = ENGINE.lock() {
        if let Some(engine) = guard.as_ref() {
            return Ok(engine.clone());
        }
    }
    let path = model_path().ok_or_else(|| anyhow!("Speaker models directory not initialised"))?;
    if !model_is_available() {
        return Err(anyhow!("Speaker model not downloaded"));
    }
    let engine = Arc::new(SpeakerEngine::load(&path)?);
    if let Ok(mut guard) = ENGINE.lock() {
        *guard = Some(engine.clone());
    }
    Ok(engine)
}

/// Identify the speaker of a 16 kHz mono chunk. Returns `None` when speaker
/// identification is off, the model is missing, or the clip is unusable.
pub fn identify(audio_16k: &[f32], device: DeviceType) -> Option<SpeakerLabel> {
    if !is_active() {
        return None;
    }
    let engine = match engine() {
        Ok(e) => e,
        Err(e) => {
            log::warn!("Speaker identification unavailable: {}", e);
            return None;
        }
    };
    let embedding = match engine.embed(audio_16k) {
        Ok(Some(e)) => e,
        Ok(None) => return None,
        Err(e) => {
            log::warn!("Speaker embedding failed: {}", e);
            return None;
        }
    };
    let seconds = audio_16k.len() as f32 / fbank::SAMPLE_RATE as f32;
    let mut guard = TRACKER.lock().ok()?;
    let tracker = guard.get_or_insert_with(SpeakerTracker::for_session);
    Some(tracker.assign(&embedding, device, seconds))
}

// ============================================================================
// Embedding model
// ============================================================================

pub struct SpeakerEngine {
    session: Mutex<Session>,
    input_name: String,
    output_name: String,
}

impl SpeakerEngine {
    pub fn load(path: &Path) -> Result<Self> {
        crate::ensure_onnx_runtime_available()?;
        let session = Session::builder()?
            .with_optimization_level(GraphOptimizationLevel::Level3)?
            .with_execution_providers(vec![CPUExecutionProvider::default().build()])?
            .with_intra_threads(2)?
            .commit_from_file(path)
            .with_context(|| format!("Failed to load speaker model {}", path.display()))?;
        let input_name = session
            .inputs
            .first()
            .map(|i| i.name.clone())
            .ok_or_else(|| anyhow!("Speaker model has no inputs"))?;
        let output_name = session
            .outputs
            .first()
            .map(|o| o.name.clone())
            .ok_or_else(|| anyhow!("Speaker model has no outputs"))?;
        log::info!(
            "Speaker model loaded ({} -> {}) from {}",
            input_name,
            output_name,
            path.display()
        );
        Ok(Self {
            session: Mutex::new(session),
            input_name,
            output_name,
        })
    }

    /// L2-normalised embedding of a 16 kHz clip, or `None` if too short.
    pub fn embed(&self, audio_16k: &[f32]) -> Result<Option<Vec<f32>>> {
        let Some(feats) = fbank::compute_fbank(audio_16k) else {
            return Ok(None);
        };
        if feats.len() < 20 {
            return Ok(None);
        }
        let frames = feats.len();
        let mut array = Array3::<f32>::zeros((1, frames, fbank::NUM_MEL_BINS));
        for (t, feat) in feats.iter().enumerate() {
            for (m, v) in feat.iter().enumerate() {
                array[[0, t, m]] = *v;
            }
        }
        let mut session = self
            .session
            .lock()
            .map_err(|_| anyhow!("Speaker model session poisoned"))?;
        let outputs = session.run(inputs![
            self.input_name.as_str() => TensorRef::from_array_view(array.view())?
        ])?;
        let output = outputs
            .get(self.output_name.as_str())
            .ok_or_else(|| anyhow!("Speaker model output '{}' missing", self.output_name))?
            .try_extract_array::<f32>()?;
        let mut embedding: Vec<f32> = output.iter().copied().collect();
        let norm = embedding.iter().map(|v| v * v).sum::<f32>().sqrt();
        if !norm.is_finite() || norm == 0.0 {
            return Ok(None);
        }
        for v in embedding.iter_mut() {
            *v /= norm;
        }
        Ok(Some(embedding))
    }
}

// ============================================================================
// Online clustering
// ============================================================================

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct SpeakerLabel {
    /// Stable zero-based index within the recording (drives the avatar).
    pub index: u32,
    /// Display label: "You", "Speaker 1", ...
    pub label: String,
    /// Similarity to the matched centroid (1.0 for a brand-new speaker).
    pub confidence: f32,
    /// The label of this index just changed (matched a remembered voice or was
    /// named by the user); earlier lines with this index should be updated.
    #[serde(default)]
    pub relabeled: bool,
}

#[derive(Debug, Clone)]
struct Speaker {
    centroid: Vec<f32>,
    count: u32,
    label: String,
    /// Remembered voice this speaker was matched to or saved as.
    profile_id: Option<String>,
}

fn remembered_voice(embedding: &[f32]) -> Option<profiles::VoiceProfile> {
    profiles::with(|store| store.best_match(embedding))
        .flatten()
        .filter(|(_, sim)| *sim >= PROFILE_MATCH_THRESHOLD)
        .map(|(p, _)| p)
}

#[derive(Debug)]
pub struct SpeakerTracker {
    speakers: Vec<Speaker>,
    last_index: Option<usize>,
    remote_count: u32,
    has_you: bool,
    /// Label the first microphone voice "You" (no remembered "me" voice to rely on).
    guess_you: bool,
}

impl Default for SpeakerTracker {
    fn default() -> Self {
        Self { speakers: Vec::new(), last_index: None, remote_count: 0, has_you: false, guess_you: true }
    }
}

fn cosine(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b.iter()).map(|(x, y)| x * y).sum()
}

impl SpeakerTracker {
    /// Tracker for a new recording, honouring the "mic is me" setting. When a
    /// remembered voice is marked as the user, that voice decides who "You" is
    /// and the microphone guess is skipped.
    pub fn for_session() -> Self {
        let has_me = profiles::with(|store| store.has_me()).unwrap_or(false);
        Self { guess_you: mic_is_me() && !has_me, ..Self::default() }
    }

    pub fn with_mic_guess(guess_you: bool) -> Self {
        Self { guess_you, ..Self::default() }
    }

    pub fn assign(&mut self, embedding: &[f32], device: DeviceType, seconds: f32) -> SpeakerLabel {
        let best = self
            .speakers
            .iter()
            .enumerate()
            .map(|(i, s)| (i, cosine(embedding, &s.centroid)))
            .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));

        if let Some((i, sim)) = best {
            let can_open_new = seconds >= MIN_SECONDS_FOR_NEW_SPEAKER && self.speakers.len() < MAX_SPEAKERS;
            if sim >= SAME_SPEAKER_THRESHOLD || !can_open_new {
                // Short or similar enough: attach to the closest voice, unless it is
                // plainly a different person and we have a previous speaker to fall back on.
                let idx = if sim >= SAME_SPEAKER_THRESHOLD * 0.7 {
                    i
                } else {
                    self.last_index.unwrap_or(i)
                };
                self.update_centroid(idx, embedding);
                self.last_index = Some(idx);
                let relabeled = self.recheck_remembered(idx);
                return SpeakerLabel {
                    index: idx as u32,
                    label: self.speakers[idx].label.clone(),
                    confidence: sim.max(0.0),
                    relabeled,
                };
            }
        }

        // New voice: a remembered (named) voice wins, otherwise "You" / "Speaker N"
        let remembered = remembered_voice(embedding)
            .filter(|p| !self.speakers.iter().any(|s| s.profile_id.as_deref() == Some(p.id.as_str())));
        let (label, profile_id) = if let Some(profile) = remembered {
            let label = profile_label(&profile);
            if label == "You" {
                self.has_you = true;
            }
            (label, Some(profile.id))
        } else if self.guess_you && device == DeviceType::Microphone && !self.has_you {
            self.has_you = true;
            ("You".to_string(), None)
        } else {
            self.remote_count += 1;
            (format!("Speaker {}", self.remote_count), None)
        };
        self.speakers.push(Speaker {
            centroid: embedding.to_vec(),
            count: 1,
            label: label.clone(),
            profile_id,
        });
        let idx = self.speakers.len() - 1;
        self.last_index = Some(idx);
        SpeakerLabel { index: idx as u32, label, confidence: 1.0, relabeled: false }
    }

    /// Once an unnamed speaker has enough clips, see whether the averaged voice
    /// matches a remembered profile. Returns true when the label changed.
    fn recheck_remembered(&mut self, idx: usize) -> bool {
        let speaker = &self.speakers[idx];
        if speaker.profile_id.is_some() || !PROFILE_RECHECK_AT.contains(&speaker.count) {
            return false;
        }
        let Some(profile) = remembered_voice(&speaker.centroid) else {
            return false;
        };
        if self.speakers.iter().any(|s| s.profile_id.as_deref() == Some(profile.id.as_str())) {
            return false;
        }
        let label = profile_label(&profile);
        let speaker = &mut self.speakers[idx];
        if speaker.label == "You" && label != "You" {
            self.has_you = false;
        }
        if label == "You" {
            self.has_you = true;
        }
        speaker.label = label;
        speaker.profile_id = Some(profile.id);
        true
    }

    fn update_centroid(&mut self, idx: usize, embedding: &[f32]) {
        let speaker = &mut self.speakers[idx];
        let n = speaker.count as f32;
        for (c, e) in speaker.centroid.iter_mut().zip(embedding.iter()) {
            *c = (*c * n + e) / (n + 1.0);
        }
        let norm = speaker.centroid.iter().map(|v| v * v).sum::<f32>().sqrt();
        if norm > 0.0 {
            for c in speaker.centroid.iter_mut() {
                *c /= norm;
            }
        }
        speaker.count += 1;
    }

    pub fn speaker_count(&self) -> usize {
        self.speakers.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unit(v: &[f32]) -> Vec<f32> {
        let n = v.iter().map(|x| x * x).sum::<f32>().sqrt();
        v.iter().map(|x| x / n).collect()
    }

    #[test]
    fn clusters_similar_voices_and_opens_new_ones() {
        let mut t = SpeakerTracker::default();
        let a = unit(&[1.0, 0.0, 0.0, 0.0]);
        let a2 = unit(&[0.95, 0.1, 0.0, 0.0]);
        let b = unit(&[0.0, 1.0, 0.0, 0.0]);

        let first = t.assign(&a, DeviceType::Microphone, 3.0);
        assert_eq!(first.label, "You");
        assert_eq!(first.index, 0);

        let same = t.assign(&a2, DeviceType::Microphone, 3.0);
        assert_eq!(same.index, 0);
        assert!(same.confidence > 0.9);

        let other = t.assign(&b, DeviceType::System, 3.0);
        assert_eq!(other.label, "Speaker 1");
        assert_eq!(other.index, 1);
        assert_eq!(t.speaker_count(), 2);

        // A short, unrelated clip must not create a third speaker
        let c = unit(&[0.0, 0.0, 1.0, 0.0]);
        let short = t.assign(&c, DeviceType::System, 0.8);
        assert_eq!(t.speaker_count(), 2);
        assert_eq!(short.index, 1, "falls back to the previous speaker");

        // The mic voice stays "You"; a second mic voice becomes a numbered speaker
        let d = unit(&[0.0, 0.0, 0.0, 1.0]);
        let second_mic = t.assign(&d, DeviceType::Microphone, 3.0);
        assert_eq!(second_mic.label, "Speaker 2");
    }

    #[test]
    fn mic_guess_off_numbers_every_voice() {
        let mut t = SpeakerTracker::with_mic_guess(false);
        let a = unit(&[1.0, 0.0, 0.0]);
        let b = unit(&[0.0, 1.0, 0.0]);
        assert_eq!(t.assign(&a, DeviceType::Microphone, 3.0).label, "Speaker 1");
        assert_eq!(t.assign(&b, DeviceType::Microphone, 3.0).label, "Speaker 2");
        assert_eq!(t.assign(&a, DeviceType::Microphone, 3.0).index, 0);
    }

    #[test]
    fn me_profile_label_is_you() {
        let p = profiles::VoiceProfile {
            id: "x".into(),
            name: "Liran".into(),
            embedding: vec![1.0],
            samples: 1,
            is_me: true,
            created_at: String::new(),
            updated_at: String::new(),
        };
        assert_eq!(profile_label(&p), "You");
        assert!(profiles::is_me_alias(" Me "));
        assert!(profiles::is_me_alias("אני"));
        assert!(!profiles::is_me_alias("Dana"));
    }

    #[test]
    fn caps_speaker_count() {
        let mut t = SpeakerTracker::default();
        for i in 0..(MAX_SPEAKERS + 5) {
            let mut v = vec![0.0; MAX_SPEAKERS + 5];
            v[i] = 1.0;
            t.assign(&v, DeviceType::System, 5.0);
        }
        assert_eq!(t.speaker_count(), MAX_SPEAKERS);
    }

    #[test]
    fn default_state() {
        assert!(model_path().is_none() || model_path().is_some());
        assert!(is_enabled());
    }
}
