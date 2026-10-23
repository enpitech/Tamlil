//! Zipformer2 streaming transducer (k2 / icefall, sherpa-onnx export) on the
//! ONNX runtime. Used for ivrit.ai's 66M-parameter Hebrew model: ~25x cheaper
//! than whisper large-v3-turbo on CPU with comparable accuracy on read speech
//! (docs/QA.md). Decodes a VAD segment as a sequence of fixed chunks
//! (`T` frames in, `decode_chunk_len` frames advanced) with greedy search.

use std::collections::HashMap;
use std::path::Path;

use anyhow::{anyhow, Context, Result};
use ort::execution_providers::CPUExecutionProvider;
use ort::session::builder::GraphOptimizationLevel;
use ort::session::Session;
use ort::value::{Tensor, Value, ValueType};

use crate::speaker_id::fbank::{self, FbankOptions, NUM_MEL_BINS};

const ENCODER_FILE: &str = "encoder.int8.onnx";
const DECODER_FILE: &str = "decoder.int8.onnx";
const JOINER_FILE: &str = "joiner.int8.onnx";
const TOKENS_FILE: &str = "tokens.txt";
/// Silence appended so the last partial chunk is decoded (sherpa-onnx practice).
const TAIL_PADDING_SECS: f32 = 1.5;

#[derive(Debug, Clone)]
struct StateSpec {
    name: String,
    shape: Vec<usize>,
    int64: bool,
}

pub struct ZipformerModel {
    encoder: Session,
    decoder: Session,
    joiner: Session,
    tokens: HashMap<i64, String>,
    chunk_frames: usize,
    chunk_shift: usize,
    context_size: usize,
    blank_id: i64,
    unk_id: Option<i64>,
    states: Vec<StateSpec>,
}

fn session(path: &Path) -> Result<Session> {
    Session::builder()?
        .with_optimization_level(GraphOptimizationLevel::Level3)?
        .with_execution_providers(vec![CPUExecutionProvider::default().build()])?
        .with_intra_threads(4)?
        .commit_from_file(path)
        .with_context(|| format!("Failed to load {}", path.display()))
}

fn tensor_dims(ty: &ValueType) -> Option<(Vec<i64>, bool)> {
    let shape = ty.tensor_shape()?;
    let dims: Vec<i64> = shape.iter().copied().collect();
    let int64 = matches!(ty.tensor_type(), Some(ort::tensor::TensorElementType::Int64));
    Some((dims, int64))
}

impl ZipformerModel {
    pub fn new(dir: &Path) -> Result<Self> {
        crate::ensure_onnx_runtime_available()?;
        let encoder = session(&dir.join(ENCODER_FILE))?;
        let decoder = session(&dir.join(DECODER_FILE))?;
        let joiner = session(&dir.join(JOINER_FILE))?;

        // Tokens: "<piece> <id>" per line
        let tokens_text = std::fs::read_to_string(dir.join(TOKENS_FILE)).context("tokens.txt")?;
        let mut tokens = HashMap::new();
        let mut unk_id = None;
        for line in tokens_text.lines() {
            let mut parts = line.rsplitn(2, ' ');
            let (Some(id), Some(piece)) = (parts.next(), parts.next()) else { continue };
            if let Ok(id) = id.trim().parse::<i64>() {
                if piece == "<unk>" {
                    unk_id = Some(id);
                }
                tokens.insert(id, piece.to_string());
            }
        }

        // Chunk geometry from the export metadata, with the input shape as fallback
        let (meta_t, meta_shift) = {
            let meta = encoder.metadata()?;
            let read = |key: &str| meta.custom(key).ok().flatten().and_then(|v| v.trim().parse::<usize>().ok());
            (read("T"), read("decode_chunk_len"))
        };
        let meta_usize = |key: &str| if key == "T" { meta_t } else { meta_shift };
        let x_dims = encoder
            .inputs
            .first()
            .and_then(|i| tensor_dims(&i.input_type))
            .map(|(d, _)| d)
            .unwrap_or_default();
        let chunk_frames = meta_usize("T")
            .or_else(|| x_dims.get(1).filter(|d| **d > 0).map(|d| *d as usize))
            .ok_or_else(|| anyhow!("Zipformer encoder: chunk length (T) unknown"))?;
        let chunk_shift = meta_usize("decode_chunk_len").unwrap_or(chunk_frames.saturating_sub(13).max(1));
        let context_size = decoder
            .inputs
            .first()
            .and_then(|i| tensor_dims(&i.input_type))
            .and_then(|(d, _)| d.get(1).filter(|v| **v > 0).map(|v| *v as usize))
            .unwrap_or(2);

        // State tensors: every encoder input after `x`, zero-initialised with the
        // declared shape (dynamic batch dimension = 1).
        let states: Vec<StateSpec> = encoder
            .inputs
            .iter()
            .skip(1)
            .map(|input| {
                let (dims, int64) = tensor_dims(&input.input_type).unwrap_or((vec![1], false));
                StateSpec {
                    name: input.name.clone(),
                    shape: dims.iter().map(|d| if *d > 0 { *d as usize } else { 1 }).collect(),
                    int64,
                }
            })
            .collect();
        log::info!(
            "Zipformer model loaded from {}: T={} shift={} context={} states={} vocab={}",
            dir.display(), chunk_frames, chunk_shift, context_size, states.len(), tokens.len()
        );
        Ok(Self { encoder, decoder, joiner, tokens, chunk_frames, chunk_shift, context_size, blank_id: 0, unk_id, states })
    }

    fn initial_states(&self) -> Result<Vec<(String, Value)>> {
        self.states
            .iter()
            .map(|s| {
                let n: usize = s.shape.iter().product();
                let value: Value = if s.int64 {
                    Tensor::<i64>::from_array((s.shape.clone(), vec![0i64; n]))?.into_dyn()
                } else {
                    Tensor::<f32>::from_array((s.shape.clone(), vec![0f32; n]))?.into_dyn()
                };
                Ok((s.name.clone(), value))
            })
            .collect()
    }

    fn decoder_out(&mut self, context: &[i64]) -> Result<Vec<f32>> {
        let y = Tensor::<i64>::from_array((vec![1usize, context.len()], context.to_vec()))?;
        let outputs = self.decoder.run(vec![("y", y)])?;
        let out = outputs
            .get("decoder_out")
            .ok_or_else(|| anyhow!("decoder output 'decoder_out' missing"))?
            .try_extract_array::<f32>()?;
        Ok(out.iter().copied().collect())
    }

    fn joiner_argmax(&mut self, enc: &[f32], dec: &[f32]) -> Result<i64> {
        let e = Tensor::<f32>::from_array((vec![1usize, enc.len()], enc.to_vec()))?;
        let d = Tensor::<f32>::from_array((vec![1usize, dec.len()], dec.to_vec()))?;
        let outputs = self.joiner.run(vec![("encoder_out", e), ("decoder_out", d)])?;
        let logit = outputs
            .get("logit")
            .ok_or_else(|| anyhow!("joiner output 'logit' missing"))?
            .try_extract_array::<f32>()?;
        let mut best = 0usize;
        let mut best_v = f32::NEG_INFINITY;
        for (i, v) in logit.iter().enumerate() {
            if *v > best_v {
                best_v = *v;
                best = i;
            }
        }
        Ok(best as i64)
    }

    /// Transcribe 16 kHz mono audio in [-1, 1]. Returns the text (may be empty).
    pub fn transcribe(&mut self, audio_16k: &[f32]) -> Result<String> {
        if audio_16k.len() < fbank::SAMPLE_RATE / 10 {
            return Ok(String::new());
        }
        let mut audio = audio_16k.to_vec();
        audio.extend(std::iter::repeat(0.0f32).take((TAIL_PADDING_SECS * fbank::SAMPLE_RATE as f32) as usize));
        let feats = fbank::compute_fbank_with(&audio, FbankOptions::k2()).unwrap_or_default();
        if feats.len() < self.chunk_frames {
            return Ok(String::new());
        }

        let mut states = self.initial_states()?;
        let mut context: Vec<i64> = vec![-1; self.context_size.saturating_sub(1)];
        context.push(self.blank_id);
        let mut dec_out = self.decoder_out(&context)?;
        let mut emitted: Vec<i64> = Vec::new();

        let mut pos = 0usize;
        while feats.len() - pos >= self.chunk_frames {
            let mut x = Vec::with_capacity(self.chunk_frames * NUM_MEL_BINS);
            for frame in &feats[pos..pos + self.chunk_frames] {
                x.extend_from_slice(frame);
            }
            let x = Tensor::<f32>::from_array((vec![1usize, self.chunk_frames, NUM_MEL_BINS], x))?;
            let mut inputs: Vec<(String, Value)> = Vec::with_capacity(states.len() + 1);
            inputs.push(("x".to_string(), x.into_dyn()));
            inputs.extend(states.drain(..));
            let outputs = self.encoder.run(inputs)?;

            // Encoder output frames, then the new states in input order
            let enc = outputs
                .get("encoder_out")
                .ok_or_else(|| anyhow!("encoder_out missing"))?
                .try_extract_array::<f32>()?;
            let dim = enc.shape().last().copied().unwrap_or(0);
            let frames: Vec<f32> = enc.iter().copied().collect();
            let mut new_states = Vec::with_capacity(self.states.len());
            for (spec, (_, value)) in self.states.iter().zip(outputs.iter().skip(1)) {
                let owned: Value = if spec.int64 {
                    let a = value.try_extract_array::<i64>()?;
                    Tensor::<i64>::from_array((a.shape().to_vec(), a.iter().copied().collect::<Vec<_>>()))?.into_dyn()
                } else {
                    let a = value.try_extract_array::<f32>()?;
                    Tensor::<f32>::from_array((a.shape().to_vec(), a.iter().copied().collect::<Vec<_>>()))?.into_dyn()
                };
                new_states.push((spec.name.clone(), owned));
            }
            drop(outputs);
            states = new_states;

            if dim > 0 {
                for t in 0..frames.len() / dim {
                    let enc_t = &frames[t * dim..(t + 1) * dim];
                    let y = self.joiner_argmax(enc_t, &dec_out)?;
                    if y != self.blank_id && Some(y) != self.unk_id {
                        emitted.push(y);
                        context.push(y);
                        let start = context.len() - self.context_size;
                        dec_out = self.decoder_out(&context[start..])?;
                    }
                }
            }
            pos += self.chunk_shift;
        }

        let mut text = String::new();
        for id in emitted {
            if let Some(piece) = self.tokens.get(&id) {
                text.push_str(&piece.replace('▁', " "));
            }
        }
        Ok(text.split_whitespace().collect::<Vec<_>>().join(" "))
    }
}
