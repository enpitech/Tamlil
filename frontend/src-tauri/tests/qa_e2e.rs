//! End-to-end QA for the recognition path, run against real models and real
//! Hebrew speech (Google FLEURS). Ignored by default: needs models and data.
//!
//! ```text
//! TAMLIL_QA_MODELS=<dir with ggml-*.bin>      TAMLIL_QA_DATA=<dir with clips.tsv>
//! TAMLIL_QA_SPEAKER_MODEL=<campplus .onnx>    TAMLIL_QA_REPORT=<report.md>
//! cargo test --release --test qa_e2e -- --ignored --nocapture --test-threads=1
//! ```
//!
//! What it checks, mirroring the app's worker: VAD segmentation of a mixed
//! "meeting", per-segment whisper transcription in Hebrew, word error rate against
//! the reference, real-time factor, language identification on the first seconds,
//! and speaker clustering of the segments.

use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::time::Instant;

use app_lib::audio::vad::ContinuousVadProcessor;
use app_lib::audio::RecordingDeviceType as DeviceType;
use app_lib::speaker_id::{SpeakerEngine, SpeakerTracker};
use app_lib::whisper_engine::{LanguageIdentifier, ModelStatus, WhisperEngine};

const HEBREW_MODEL: &str = "ivrit-large-v3-turbo";
const SAMPLE_RATE: usize = 16_000;

#[derive(Debug, Clone)]
struct Clip {
    lang: String,
    id: String,
    gender: String,
    seconds: f32,
    audio: Vec<f32>,
    text: String,
}

fn env_path(name: &str) -> Option<PathBuf> {
    std::env::var_os(name).map(PathBuf::from).filter(|p| !p.as_os_str().is_empty())
}

fn report(section: &str) {
    println!("{}", section);
    if let Some(path) = env_path("TAMLIL_QA_REPORT") {
        if let Ok(mut f) = fs::OpenOptions::new().create(true).append(true).open(path) {
            let _ = writeln!(f, "{}\n", section);
        }
    }
}

fn load_clips() -> Vec<Clip> {
    let data = env_path("TAMLIL_QA_DATA").expect("TAMLIL_QA_DATA not set");
    let index = fs::read_to_string(data.join("clips.tsv")).expect("clips.tsv");
    let mut clips = Vec::new();
    for line in index.lines() {
        let cols: Vec<&str> = line.split('\t').collect();
        if cols.len() < 6 {
            continue;
        }
        let bytes = fs::read(cols[4]).unwrap_or_else(|e| panic!("{}: {}", cols[4], e));
        let audio: Vec<f32> = bytes
            .chunks_exact(4)
            .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
            .collect();
        clips.push(Clip {
            lang: cols[0].to_string(),
            id: cols[1].to_string(),
            gender: cols[2].to_string(),
            seconds: cols[3].parse().unwrap_or(audio.len() as f32 / SAMPLE_RATE as f32),
            audio,
            text: cols[5..].join("\t"),
        });
    }
    assert!(!clips.is_empty(), "no clips in {}", data.display());
    clips
}

/// Lower-case, strip niqqud/cantillation and punctuation, split on whitespace.
fn normalise(text: &str) -> Vec<String> {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        let code = ch as u32;
        if (0x0591..=0x05C7).contains(&code) {
            continue; // Hebrew points and accents
        }
        if ch.is_alphanumeric() || ch == '\'' {
            out.extend(ch.to_lowercase());
        } else if ch == '־' || ch == '-' || ch.is_whitespace() || ch.is_ascii_punctuation() || "…״׳“”„".contains(ch) {
            out.push(' ');
        }
    }
    out.split_whitespace().map(|s| s.to_string()).collect()
}

/// Word error rate: Levenshtein distance over words / reference length.
fn wer(reference: &str, hypothesis: &str) -> (f32, usize, usize) {
    let r = normalise(reference);
    let h = normalise(hypothesis);
    let mut prev: Vec<usize> = (0..=h.len()).collect();
    for i in 1..=r.len() {
        let mut cur = vec![i; h.len() + 1];
        for j in 1..=h.len() {
            let sub = prev[j - 1] + usize::from(r[i - 1] != h[j - 1]);
            cur[j] = sub.min(prev[j] + 1).min(cur[j - 1] + 1);
        }
        prev = cur;
    }
    let errors = prev[h.len()];
    (errors as f32 / r.len().max(1) as f32, errors, r.len())
}

async fn whisper_with(model: &str) -> WhisperEngine {
    let models = env_path("TAMLIL_QA_MODELS").expect("TAMLIL_QA_MODELS not set");
    app_lib::model_registry::init(models.clone());
    let engine = WhisperEngine::new_with_models_dir(Some(models.clone())).expect("whisper engine");
    let discovered = engine.discover_models().await.expect("discover models");
    let info = discovered
        .iter()
        .find(|m| m.name == model)
        .unwrap_or_else(|| panic!("model {} not in registry; found: {:?}", model, discovered.iter().map(|m| &m.name).collect::<Vec<_>>()));
    assert!(matches!(info.status, ModelStatus::Available), "{} status {:?} (dir {})", model, info.status, models.display());
    let t = Instant::now();
    engine.load_model(model).await.expect("load model");
    report(&format!("- Loaded `{}` in {:.1} s", model, t.elapsed().as_secs_f32()));
    engine
}

/// Build a "meeting": clips of two speakers alternating, 0.8 s of silence between.
/// Returns the mixed audio and, per clip, (start_sample, end_sample, index into clips).
fn build_meeting(clips: &[&Clip]) -> (Vec<f32>, Vec<(usize, usize, usize)>) {
    let gap = vec![0.0f32; SAMPLE_RATE * 8 / 10];
    let mut audio = Vec::new();
    let mut spans = Vec::new();
    for (i, clip) in clips.iter().enumerate() {
        audio.extend_from_slice(&gap);
        let start = audio.len();
        audio.extend_from_slice(&clip.audio);
        spans.push((start, audio.len(), i));
    }
    audio.extend_from_slice(&gap);
    (audio, spans)
}

/// Run the pipeline's VAD over the audio in 100 ms windows, as the app does.
fn vad_segments(audio: &[f32]) -> Vec<app_lib::audio::vad::SpeechSegment> {
    let mut vad = ContinuousVadProcessor::new(SAMPLE_RATE as u32, 500).expect("vad");
    let mut segments = Vec::new();
    for window in audio.chunks(SAMPLE_RATE / 10) {
        segments.extend(vad.process_audio(window).expect("vad window"));
    }
    segments.extend(vad.flush().expect("vad flush"));
    segments.retain(|s| s.samples.len() >= 800);
    segments
}

/// Hebrew clips only, for recognition. (FLEURS Hebrew is read by male voices only.)
fn hebrew_clips<'a>(clips: &'a [Clip], n: usize) -> Vec<&'a Clip> {
    clips.iter().filter(|c| c.lang == "he_il").take(n).collect()
}

/// Two clearly different speakers for the speaker tests: Hebrew male clips and
/// female clips from any language (speaker embeddings are language-agnostic).
fn pick_two_speakers<'a>(clips: &'a [Clip], per_gender: usize) -> Vec<&'a Clip> {
    let f: Vec<&Clip> = clips.iter().filter(|c| c.gender == "FEMALE").take(per_gender).collect();
    let m: Vec<&Clip> = clips.iter().filter(|c| c.lang == "he_il" && c.gender == "MALE").take(per_gender).collect();
    let mut out = Vec::new();
    for i in 0..per_gender.min(f.len()).min(m.len()) {
        out.push(f[i]);
        out.push(m[i]);
    }
    out
}

// ---------------------------------------------------------------------------

#[tokio::test]
#[ignore]
async fn qa_transcription_per_clip() {
    let clips = load_clips();
    let max: usize = std::env::var("TAMLIL_QA_MAX_CLIPS").ok().and_then(|v| v.parse().ok()).unwrap_or(16);
    let engine = whisper_with(HEBREW_MODEL).await;

    // Variants "label:ctx:no_timestamps:max_initial_ts" from TAMLIL_QA_WHISPER_VARIANTS
    let variants: Vec<String> = std::env::var("TAMLIL_QA_WHISPER_VARIANTS")
        .map(|v| v.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect())
        .unwrap_or_else(|_| vec!["full:full:1:1.0".to_string(), "dynamic:dynamic:1:1.0".to_string()]);
    let mut worst = 0.0f32;
    for variant in &variants {
    let parts: Vec<&str> = variant.split(':').collect();
    let mode = variant.as_str();
    if parts.len() == 4 {
        std::env::set_var("TAMLIL_AUDIO_CTX", parts[1]);
        std::env::set_var("TAMLIL_WHISPER_NO_TIMESTAMPS", parts[2]);
        std::env::set_var("TAMLIL_WHISPER_MAX_INITIAL_TS", parts[3]);
    }
    let mut rows = Vec::new();
    let (mut total_errors, mut total_words, mut total_audio, mut total_time) = (0usize, 0usize, 0f32, 0f32);
    for clip in clips.iter().filter(|c| c.lang == "he_il").take(max) {
        let t = Instant::now();
        let (text, _conf, _partial) = engine
            .transcribe_audio_with_confidence(clip.audio.clone(), Some("he".to_string()))
            .await
            .unwrap_or_else(|e| (format!("<error: {}>", e), 0.0, false));
        let secs = t.elapsed().as_secs_f32();
        let (rate, errors, words) = wer(&clip.text, &text);
        total_errors += errors;
        total_words += words;
        total_audio += clip.seconds;
        total_time += secs;
        rows.push(format!(
            "| {} | {} | {:.1} | {:.2} | {:.0}% | {} | {} |",
            clip.id, clip.gender.to_lowercase(), clip.seconds, secs / clip.seconds, rate * 100.0,
            clip.text.replace('|', "/"), text.trim().replace('|', "/")
        ));
    }
    let overall = total_errors as f32 / total_words.max(1) as f32;
    worst = worst.max(overall);
    report(&format!(
        "## Per-clip transcription (ivrit large-v3-turbo, language=he, encoder context: {})\n\n\
         Clips: {} · audio {:.0} s · compute {:.0} s · **RTF {:.2}** · **WER {:.1}%** ({} errors / {} words)\n\n\
         | clip | voice | s | RTF | WER | reference | recognised |\n|---|---|---|---|---|---|---|\n{}",
        mode, rows.len(), total_audio, total_time, total_time / total_audio.max(0.1), overall * 100.0, total_errors, total_words,
        rows.join("\n")
    ));
    }
    std::env::remove_var("TAMLIL_AUDIO_CTX");
    std::env::remove_var("TAMLIL_WHISPER_NO_TIMESTAMPS");
    std::env::remove_var("TAMLIL_WHISPER_MAX_INITIAL_TS");
    assert!(worst < 0.6, "WER {:.1}% is unusable", worst * 100.0);
}

#[tokio::test]
#[ignore]
async fn qa_meeting_vad_pipeline() {
    let clips = load_clips();
    let speakers = hebrew_clips(&clips, 8);
    assert!(speakers.len() >= 4, "need Hebrew clips");
    let (audio, spans) = build_meeting(&speakers);
    let engine = whisper_with(HEBREW_MODEL).await;

    // VAD variants: "name:threshold:pre_pad_ms" entries in TAMLIL_QA_VAD_VARIANTS
    // (default: the production settings only).
    let variants: Vec<String> = std::env::var("TAMLIL_QA_VAD_VARIANTS")
        .map(|v| v.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect())
        .unwrap_or_else(|_| vec!["default:0.50:300".to_string()]);
    let mut worst_rate = 0.0f32;
    for variant in &variants {
    let parts: Vec<&str> = variant.split(':').collect();
    if parts.len() == 3 {
        std::env::set_var("TAMLIL_VAD_THRESHOLD", parts[1]);
        std::env::set_var("TAMLIL_VAD_PRE_PAD_MS", parts[2]);
    }
    let segments = vad_segments(&audio);
    let seg_secs: Vec<f32> = segments.iter().map(|s| s.samples.len() as f32 / SAMPLE_RATE as f32).collect();
    let covered: f32 = seg_secs.iter().sum();
    let speech: f32 = speakers.iter().map(|c| c.seconds).sum();

    let t = Instant::now();
    let mut hypothesis = String::new();
    let mut lines = Vec::new();
    for (i, seg) in segments.iter().enumerate() {
        let (text, _, _) = engine
            .transcribe_audio_with_confidence(seg.samples.clone(), Some("he".to_string()))
            .await
            .unwrap_or_else(|e| (format!("<error: {}>", e), 0.0, false));
        hypothesis.push_str(text.trim());
        hypothesis.push(' ');
        lines.push(format!("| {} | {:.1} | {:.1} | {} |", i, seg.start_timestamp_ms / 1000.0, seg_secs[i], text.trim().replace('|', "/")));
    }
    let compute = t.elapsed().as_secs_f32();
    let reference: String = speakers.iter().map(|c| c.text.as_str()).collect::<Vec<_>>().join(" ");
    let (rate, errors, words) = wer(&reference, &hypothesis);
    let _ = spans;

    worst_rate = worst_rate.max(rate);
    report(&format!(
        "## Simulated meeting through VAD + per-segment whisper (VAD variant {})\n\n\
         {} Hebrew clips, {:.0} s speech in {:.0} s of audio. VAD produced **{} segments** \
         (mean {:.1} s, min {:.1} s, max {:.1} s) covering {:.0} s. Whisper compute {:.0} s → **RTF {:.2}** vs speech. \
         **WER {:.1}%** ({} / {}).\n\n| seg | start | s | text |\n|---|---|---|---|\n{}",
        variant, speakers.len(), speech, audio.len() as f32 / SAMPLE_RATE as f32, segments.len(),
        covered / segments.len().max(1) as f32,
        seg_secs.iter().cloned().fold(f32::INFINITY, f32::min),
        seg_secs.iter().cloned().fold(0.0, f32::max),
        covered, compute, compute / speech.max(0.1), rate * 100.0, errors, words,
        lines.join("\n")
    ));
    }
    std::env::remove_var("TAMLIL_VAD_THRESHOLD");
    std::env::remove_var("TAMLIL_VAD_PRE_PAD_MS");
    assert!(worst_rate < 0.7, "meeting WER {:.1}% is unusable", worst_rate * 100.0);
}

#[tokio::test]
#[ignore]
async fn qa_language_detection() {
    let clips = load_clips();
    let models = env_path("TAMLIL_QA_MODELS").expect("TAMLIL_QA_MODELS not set");
    let general = ["base", "tiny"]
        .iter()
        .map(|n| (n.to_string(), models.join(format!("ggml-{}.bin", n))))
        .find(|(_, p)| p.exists())
        .expect("ggml-base.bin or ggml-tiny.bin in TAMLIL_QA_MODELS");
    let identifier = LanguageIdentifier::load(&general.0, &general.1).expect("language-id model");
    let ivrit = whisper_with(HEBREW_MODEL).await;

    let mut lines = Vec::new();
    let mut general_ok = 0;
    let mut general_total = 0;
    for clip in clips.iter().filter(|c| c.lang == "he_il").take(4).chain(clips.iter().filter(|c| c.lang == "en_us").take(4)) {
        let expected = if clip.lang == "he_il" { "he" } else { "en" };
        let window: Vec<f32> = clip.audio.iter().take(SAMPLE_RATE * 8).cloned().collect();
        let t = Instant::now();
        let g = identifier
            .scores(&window)
            .and_then(|sc| app_lib::whisper_engine::choose_language(&sc, &["he".to_string(), "en".to_string()]).ok_or_else(|| anyhow::anyhow!("no scores")));
        let g_secs = t.elapsed().as_secs_f32();
        let t = Instant::now();
        let i = ivrit.detect_language(&window).await;
        let i_secs = t.elapsed().as_secs_f32();
        let fmt = |r: &Result<(String, f32), anyhow::Error>| match r {
            Ok((c, p)) => format!("{} ({:.2})", c, p),
            Err(e) => format!("error: {}", e),
        };
        general_total += 1;
        if matches!(&g, Ok((c, _)) if c == expected) { general_ok += 1; }
        lines.push(format!("| {} {} | {} | {} | {:.2} s | {} | {:.2} s |", clip.lang, clip.id, expected, fmt(&g), g_secs, fmt(&i), i_secs));
    }
    report(&format!(
        "## Language identification (first 8 s)\n\nGeneral model `{}` vs the Hebrew fine-tune. {}/{} correct with the general model.\n\n\
         | clip | expected | {} | time | ivrit-turbo | time |\n|---|---|---|---|---|---|\n{}",
        general.0, general_ok, general_total, general.0, lines.join("\n")
    ));
    assert_eq!(general_ok, general_total, "general-model language identification failed: {:?}", lines);
}

#[tokio::test]
#[ignore]
async fn qa_speaker_identification() {
    let clips = load_clips();
    let model = env_path("TAMLIL_QA_SPEAKER_MODEL").expect("TAMLIL_QA_SPEAKER_MODEL not set");
    let engine = SpeakerEngine::load(&model).expect("speaker model");

    // Same-speaker vs different-speaker similarity on whole clips (threshold calibration)
    fn cosine(a: &[f32], b: &[f32]) -> f32 { a.iter().zip(b).map(|(x, y)| x * y).sum() }
    let he: Vec<&Clip> = clips.iter().collect();
    let mut same = Vec::new();
    let mut diff = Vec::new();
    let mut embeddings: HashMap<String, Vec<f32>> = HashMap::new();
    let t = Instant::now();
    for clip in &he {
        let half = clip.audio.len() / 2;
        let (Ok(Some(a)), Ok(Some(b))) = (engine.embed(&clip.audio[..half]), engine.embed(&clip.audio[half..])) else { continue };
        same.push(cosine(&a, &b));
        if let Ok(Some(full)) = engine.embed(&clip.audio) {
            embeddings.insert(format!("{}/{}", clip.lang, clip.id), full);
        }
    }
    let embed_time = t.elapsed().as_secs_f32() / (he.len() * 3).max(1) as f32;
    for (i, a) in he.iter().enumerate() {
        for b in he.iter().skip(i + 1) {
            if a.gender != b.gender {
                if let (Some(ea), Some(eb)) = (embeddings.get(&format!("{}/{}", a.lang, a.id)), embeddings.get(&format!("{}/{}", b.lang, b.id))) {
                    diff.push(cosine(ea, eb));
                }
            }
        }
    }
    let stats = |v: &[f32]| {
        let mean = v.iter().sum::<f32>() / v.len().max(1) as f32;
        let min = v.iter().cloned().fold(f32::INFINITY, f32::min);
        let max = v.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
        format!("mean {:.2}, min {:.2}, max {:.2} (n={})", mean, min, max, v.len())
    };

    // Clustering with known ground truth: each clip is cut in two utterances, the
    // halves of one clip are certainly the same speaker, and female (English) vs
    // male (Hebrew) clips are certainly different speakers. Utterances alternate
    // F1a M1a F2a M2a ... F1b M1b ... as a remote (system-channel) conversation.
    let speakers = pick_two_speakers(&clips, 5);
    let mut utterances: Vec<(String, Vec<f32>)> = Vec::new(); // (clip key, audio)
    for part in 0..2 {
        for clip in &speakers {
            let half = clip.audio.len() / 2;
            let audio = if part == 0 { clip.audio[..half].to_vec() } else { clip.audio[half..].to_vec() };
            utterances.push((format!("{}/{}", clip.lang, clip.id), audio));
        }
    }
    let mut tracker = SpeakerTracker::with_mic_guess(false);
    let mut labels_by_clip: HashMap<String, Vec<String>> = HashMap::new();
    let mut unassigned = 0;
    for (key, audio) in &utterances {
        match engine.embed(audio) {
            Ok(Some(e)) => {
                let label = tracker.assign(&e, DeviceType::System, audio.len() as f32 / SAMPLE_RATE as f32);
                labels_by_clip.entry(key.clone()).or_default().push(label.label);
            }
            _ => unassigned += 1,
        }
    }
    let consistent = labels_by_clip.values().filter(|v| v.len() == 2 && v[0] == v[1]).count();
    let clips_total = labels_by_clip.len();
    let mut female_labels = std::collections::HashSet::new();
    let mut male_labels = std::collections::HashSet::new();
    for clip in &speakers {
        let key = format!("{}/{}", clip.lang, clip.id);
        for l in labels_by_clip.get(&key).cloned().unwrap_or_default() {
            if clip.gender == "FEMALE" { female_labels.insert(l); } else { male_labels.insert(l); }
        }
    }
    let crossed: Vec<&String> = female_labels.intersection(&male_labels).collect();
    let purity = consistent as f32 / clips_total.max(1) as f32;
    let purity_lines = vec![
        format!("- Same clip, two halves → same label: {}/{} clips", consistent, clips_total),
        format!("- Labels used by female voices: {:?}", female_labels),
        format!("- Labels used by male voices: {:?}", male_labels),
        format!("- Labels shared across genders (confusions): {:?}", crossed),
    ];
    let segments: Vec<Vec<f32>> = utterances.iter().map(|(_, a)| a.clone()).collect();

    // With "my microphone is only me" everything from the mic must be You
    let mut mic_tracker = SpeakerTracker::with_mic_guess(true);
    let mut you = 0;
    let mut mic_total = 0;
    for seg in segments.iter().take(20) {
        if let Ok(Some(e)) = engine.embed(seg) {
            mic_total += 1;
            if mic_tracker.assign(&e, DeviceType::Microphone, seg.len() as f32 / SAMPLE_RATE as f32).label == "You" { you += 1; }
        }
    }

    report(&format!(
        "## Speaker identification (WeSpeaker CAM++)\n\n\
         - Embedding time per clip: {:.0} ms\n- Same speaker (two halves of one clip): {}\n- Different speakers (female vs male clips, any language): {}\n\
         - Conversation clustering (system channel, mic guess off): **{} distinct labels** for 2 speakers, same-clip consistency **{:.0}%**, {} utterances too short to embed\n{}\n\
         - Mic-is-me: {}/{} mic segments labelled You\n",
        embed_time * 1000.0, stats(&same), stats(&diff), tracker.speaker_count(), purity * 100.0, unassigned,
        purity_lines.join("\n"), you, mic_total
    ));
    assert!(purity >= 0.75, "same-clip consistency {:.0}%", purity * 100.0);
    assert!(tracker.speaker_count() <= 4, "too many speakers: {}", tracker.speaker_count());
    assert!(crossed.is_empty(), "female and male voices share labels: {:?}", crossed);
    assert_eq!(you, mic_total, "mic-is-me must label every mic segment You");
}

/// Dump whole-clip speaker embeddings as JSON for cross-checking the Rust
/// fbank + ONNX path against a reference implementation (see qa/embed_reference.py).
#[tokio::test]
#[ignore]
async fn qa_dump_speaker_embeddings() {
    let clips = load_clips();
    let model = env_path("TAMLIL_QA_SPEAKER_MODEL").expect("TAMLIL_QA_SPEAKER_MODEL not set");
    let engine = SpeakerEngine::load(&model).expect("speaker model");
    let out = env_path("TAMLIL_QA_DATA").unwrap().join("embeddings-rust.json");
    let mut entries = Vec::new();
    for clip in &clips {
        if let Ok(Some(e)) = engine.embed(&clip.audio) {
            let vals: Vec<String> = e.iter().map(|v| format!("{:.6}", v)).collect();
            entries.push(format!("\"{}/{}\": [{}]", clip.lang, clip.id, vals.join(",")));
        }
    }
    fs::write(&out, format!("{{{}}}", entries.join(",\n"))).unwrap();
    report(&format!("- Wrote {} embeddings to `{}`", entries.len(), out.display()));
}

/// Dump raw fbank features of the first clip (frame count + first frames) to
/// compare with the Kaldi reference (qa/embed_reference.py --fbank).
#[tokio::test]
#[ignore]
async fn qa_dump_fbank() {
    let clips = load_clips();
    let clip = &clips[0];
    let feats = app_lib::speaker_id::fbank::compute_fbank(&clip.audio).expect("fbank");
    let out = env_path("TAMLIL_QA_DATA").unwrap().join("fbank-rust.json");
    let rows: Vec<String> = feats
        .iter()
        .take(4)
        .map(|f| format!("[{}]", f.iter().map(|v| format!("{:.5}", v)).collect::<Vec<_>>().join(",")))
        .collect();
    fs::write(&out, format!("{{\"clip\": \"{}/{}\", \"frames\": {}, \"samples\": {}, \"first\": [{}]}}", clip.lang, clip.id, feats.len(), clip.audio.len(), rows.join(","))).unwrap();
    report(&format!("- Wrote fbank of {}/{} ({} frames) to `{}`", clip.lang, clip.id, feats.len(), out.display()));
}
