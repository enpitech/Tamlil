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
    let nyquist = SAMPLE_RATE as f32 / 2.0;
    let high_freq = nyquist;
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

/// Compute `[frames, 80]` mean-normalised log-mel features from 16 kHz mono
/// audio in `[-1, 1]`. Returns `None` when the clip is too short for one frame.
pub fn compute_fbank(audio: &[f32]) -> Option<Vec<[f32; NUM_MEL_BINS]>> {
    if audio.len() < FRAME_LENGTH {
        return None;
    }
    let num_frames = 1 + (audio.len() - FRAME_LENGTH) / FRAME_SHIFT;
    let banks = mel_banks();
    let window: Vec<f32> = (0..FRAME_LENGTH)
        .map(|i| 0.54 - 0.46 * (2.0 * std::f32::consts::PI * i as f32 / (FRAME_LENGTH as f32 - 1.0)).cos())
        .collect();

    let mut planner = RealFftPlanner::<f32>::new();
    let fft = planner.plan_fft_forward(FFT_SIZE);
    let mut input = fft.make_input_vec();
    let mut spectrum = fft.make_output_vec();
    let mut power = vec![0f32; FFT_SIZE / 2 + 1];

    let mut features = Vec::with_capacity(num_frames);
    let mut frame = vec![0f32; FRAME_LENGTH];

    for f in 0..num_frames {
        let start = f * FRAME_SHIFT;
        // Kaldi works on int16-scaled samples
        for i in 0..FRAME_LENGTH {
            frame[i] = audio[start + i] * 32768.0;
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
