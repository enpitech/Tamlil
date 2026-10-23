//! Kaldi-compatible log mel filterbank features, matching what WeSpeaker uses
//! to train its speaker-embedding models:
//! `torchaudio.compliance.kaldi.fbank(num_mel_bins=80, frame_length=25,
//! frame_shift=10, dither=0, sample_frequency=16000, window_type="hamming")`
//! on int16-scaled audio, followed by per-utterance mean normalisation.

use realfft::RealFftPlanner;

pub const SAMPLE_RATE: usize = 16_000;
pub const NUM_MEL_BINS: usize = 80;
const FRAME_LENGTH: usize = 400; // 25 ms
const FRAME_SHIFT: usize = 160; // 10 ms
const FFT_SIZE: usize = 512; // next power of two >= 400
const PREEMPH: f32 = 0.97;
const LOW_FREQ: f32 = 20.0;
const EPS: f32 = 1.1920929e-7; // f32 epsilon, Kaldi's energy floor for log

fn mel_scale(freq: f32) -> f32 {
    1127.0 * (1.0 + freq / 700.0).ln()
}

/// Triangular mel filterbank laid out over the FFT power bins (Kaldi style).
fn mel_banks() -> Vec<Vec<(usize, f32)>> {
    mel_banks_with(0.0)
}

/// `high_freq <= 0` means Nyquist + high_freq (Kaldi convention).
fn mel_banks_with(high_freq: f32) -> Vec<Vec<(usize, f32)>> {
    let nyquist = SAMPLE_RATE as f32 / 2.0;
    let high_freq = if high_freq <= 0.0 { nyquist + high_freq } else { high_freq };
    let num_fft_bins = FFT_SIZE / 2;
    let fft_bin_width = SAMPLE_RATE as f32 / FFT_SIZE as f32;
    let mel_low = mel_scale(LOW_FREQ);
    let mel_high = mel_scale(high_freq);
    let mel_delta = (mel_high - mel_low) / (NUM_MEL_BINS as f32 + 1.0);

    (0..NUM_MEL_BINS)
        .map(|bin| {
            let left = mel_low + bin as f32 * mel_delta;
            let center = mel_low + (bin as f32 + 1.0) * mel_delta;
            let right = mel_low + (bin as f32 + 2.0) * mel_delta;
            let mut weights = Vec::new();
            for i in 0..num_fft_bins {
                let freq = fft_bin_width * i as f32;
                let mel = mel_scale(freq);
                if mel > left && mel < right {
                    let w = if mel <= center {
                        (mel - left) / (center - left)
                    } else {
                        (right - mel) / (right - center)
                    };
                    weights.push((i, w));
                }
            }
            weights
        })
        .collect()
}

/// Kaldi window functions.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum WindowType {
    Hamming,
    /// Kaldi's default: `(0.5 - 0.5 cos)^0.85`.
    Povey,
}

/// Feature options. `speaker()` matches WeSpeaker (int16 scale, Hamming,
/// snip_edges, CMN); `k2()` matches sherpa-onnx / icefall transducers (audio
/// in [-1, 1], Povey window, no edge snipping, high_freq = Nyquist - 400 Hz, no CMN).
#[derive(Debug, Clone, Copy)]
pub struct FbankOptions {
    pub window: WindowType,
    /// `true`: frames fully inside the signal (Kaldi default); `false`: centred
    /// frames with reflected padding, `round(len / shift)` frames.
    pub snip_edges: bool,
    /// Upper edge of the mel banks in Hz; `<= 0` means Nyquist + value.
    pub high_freq: f32,
    /// Multiplier applied to the samples before analysis.
    pub scale: f32,
    /// Subtract the per-utterance mean of every bin.
    pub cmn: bool,
}

impl FbankOptions {
    pub const fn speaker() -> Self {
        Self { window: WindowType::Hamming, snip_edges: true, high_freq: 0.0, scale: 32768.0, cmn: true }
    }

    pub const fn k2() -> Self {
        Self { window: WindowType::Povey, snip_edges: false, high_freq: -400.0, scale: 1.0, cmn: false }
    }
}

/// Compute `[frames, 80]` mean-normalised log-mel features from 16 kHz mono
/// audio in `[-1, 1]` with the speaker-model settings. Returns `None` when the
/// clip is too short for one frame.
pub fn compute_fbank(audio: &[f32]) -> Option<Vec<[f32; NUM_MEL_BINS]>> {
    compute_fbank_with(audio, FbankOptions::speaker())
}

fn window_fn(kind: WindowType) -> Vec<f32> {
    (0..FRAME_LENGTH)
        .map(|i| {
            let a = 2.0 * std::f32::consts::PI * i as f32 / (FRAME_LENGTH as f32 - 1.0);
            match kind {
                WindowType::Hamming => 0.54 - 0.46 * a.cos(),
                WindowType::Povey => (0.5 - 0.5 * a.cos()).powf(0.85),
            }
        })
        .collect()
}

/// Kaldi `snip_edges = false`: frame `f` is centred at `(f + 0.5) * shift`;
/// samples outside the signal are mirrored.
fn frame_start_reflected(audio: &[f32], f: usize, frame: &mut [f32]) {
    let centre = (f as i64 * FRAME_SHIFT as i64) + FRAME_SHIFT as i64 / 2;
    let start = centre - FRAME_LENGTH as i64 / 2;
    let n = audio.len() as i64;
    for (i, v) in frame.iter_mut().enumerate() {
        let mut idx = start + i as i64;
        if idx < 0 {
            idx = -idx - 1;
        }
        if idx >= n {
            idx = 2 * n - idx - 1;
        }
        *v = audio[idx.clamp(0, n - 1) as usize];
    }
}

/// Log-mel features with explicit options; see [`FbankOptions`].
pub fn compute_fbank_with(audio: &[f32], opts: FbankOptions) -> Option<Vec<[f32; NUM_MEL_BINS]>> {
    if audio.len() < FRAME_LENGTH {
        return None;
    }
    let num_frames = if opts.snip_edges {
        1 + (audio.len() - FRAME_LENGTH) / FRAME_SHIFT
    } else {
        ((audio.len() as f32 / FRAME_SHIFT as f32) + 0.5).floor() as usize
    };
    let banks = mel_banks_with(opts.high_freq);
    let window = window_fn(opts.window);

    let mut planner = RealFftPlanner::<f32>::new();
    let fft = planner.plan_fft_forward(FFT_SIZE);
    let mut input = fft.make_input_vec();
    let mut spectrum = fft.make_output_vec();
    let mut power = vec![0f32; FFT_SIZE / 2 + 1];

    let mut features = Vec::with_capacity(num_frames);
    let mut frame = vec![0f32; FRAME_LENGTH];

    for f in 0..num_frames {
        if opts.snip_edges {
            let start = f * FRAME_SHIFT;
            frame.copy_from_slice(&audio[start..start + FRAME_LENGTH]);
        } else {
            frame_start_reflected(audio, f, &mut frame);
        }
        if opts.scale != 1.0 {
            for v in frame.iter_mut() {
                *v *= opts.scale;
            }
        }
        // remove DC offset
        let mean = frame.iter().sum::<f32>() / FRAME_LENGTH as f32;
        for v in frame.iter_mut() {
            *v -= mean;
        }
        // pre-emphasis (in place, from the end)
        for i in (1..FRAME_LENGTH).rev() {
            frame[i] -= PREEMPH * frame[i - 1];
        }
        frame[0] -= PREEMPH * frame[0];
        // window + zero pad
        for i in 0..FFT_SIZE {
            input[i] = if i < FRAME_LENGTH { frame[i] * window[i] } else { 0.0 };
        }
        if fft.process(&mut input, &mut spectrum).is_err() {
            return None;
        }
        for (i, c) in spectrum.iter().enumerate() {
            power[i] = c.re * c.re + c.im * c.im;
        }
        let mut feat = [0f32; NUM_MEL_BINS];
        for (bin, weights) in banks.iter().enumerate() {
            let energy: f32 = weights.iter().map(|(i, w)| power[*i] * w).sum();
            feat[bin] = energy.max(EPS).ln();
        }
        features.push(feat);
    }

    if !opts.cmn {
        return Some(features);
    }
    // Cepstral mean normalisation over the utterance
    let n = features.len() as f32;
    let mut mean = [0f32; NUM_MEL_BINS];
    for feat in &features {
        for (m, v) in mean.iter_mut().zip(feat.iter()) {
            *m += v;
        }
    }
    for m in mean.iter_mut() {
        *m /= n;
    }
    for feat in features.iter_mut() {
        for (v, m) in feat.iter_mut().zip(mean.iter()) {
            *v -= m;
        }
    }
    Some(features)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fbank_shape_and_normalisation() {
        // 1 second of a 440 Hz tone
        let audio: Vec<f32> = (0..SAMPLE_RATE)
            .map(|i| (2.0 * std::f32::consts::PI * 440.0 * i as f32 / SAMPLE_RATE as f32).sin() * 0.5)
            .collect();
        let feats = compute_fbank(&audio).expect("features");
        assert_eq!(feats.len(), 1 + (SAMPLE_RATE - FRAME_LENGTH) / FRAME_SHIFT);
        // mean-normalised: per-bin mean ~ 0
        let mean0: f32 = feats.iter().map(|f| f[0]).sum::<f32>() / feats.len() as f32;
        assert!(mean0.abs() < 1e-3, "mean {}", mean0);
        // the tone's energy lands in a low bin: that bin should have a positive peak somewhere above others before CMN;
        // after CMN the features must still be finite
        assert!(feats.iter().all(|f| f.iter().all(|v| v.is_finite())));
    }

    #[test]
    fn k2_options_centre_frames_and_skip_cmn() {
        let audio: Vec<f32> = (0..SAMPLE_RATE).map(|i| ((i * 7919) % 1000) as f32 / 1000.0 - 0.5).collect();
        let feats = compute_fbank_with(&audio, FbankOptions::k2()).expect("features");
        assert_eq!(feats.len(), SAMPLE_RATE / FRAME_SHIFT); // round(len / shift)
        let mean0: f32 = feats.iter().map(|f| f[0]).sum::<f32>() / feats.len() as f32;
        assert!(mean0.abs() > 1e-3, "no CMN expected");
        assert!(feats.iter().all(|f| f.iter().all(|v| v.is_finite())));
    }

    #[test]
    fn too_short_audio_yields_none() {
        assert!(compute_fbank(&[0.0; 100]).is_none());
    }

    #[test]
    fn mel_banks_cover_all_bins() {
        let banks = mel_banks();
        assert_eq!(banks.len(), NUM_MEL_BINS);
        assert!(banks.iter().all(|b| !b.is_empty()));
    }
}
