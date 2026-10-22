#!/usr/bin/env python3
"""Cross-check the Rust speaker-embedding path (fbank.rs + ort) against the
reference WeSpeaker recipe: Kaldi fbank (kaldi-native-fbank: 80 mel, 25/10 ms,
Hamming, no dither) + per-utterance mean normalisation + the same CAM++ ONNX model.

    embed_reference.py --model campplus.onnx --data qa-data [--rust qa-data/embeddings-rust.json]

Prints, per clip, the cosine between the Rust embedding and the reference one
(should be > 0.98), and the same-speaker / different-speaker similarity stats of
the reference embeddings.
"""
import argparse, array, csv, json, math, sys
import numpy as np
import kaldi_native_fbank as knf
import onnxruntime as ort

def load_clip(path):
    a = array.array("f"); a.frombytes(open(path, "rb").read())
    return np.array(a, dtype=np.float32)

def kaldi_fbank(wav):
    opts = knf.FbankOptions()
    opts.frame_opts.samp_freq = 16000
    opts.frame_opts.dither = 0.0
    opts.frame_opts.window_type = "hamming"
    opts.frame_opts.frame_length_ms = 25
    opts.frame_opts.frame_shift_ms = 10
    opts.mel_opts.num_bins = 80
    opts.energy_floor = 0.0
    opts.use_energy = False
    fb = knf.OnlineFbank(opts)
    fb.accept_waveform(16000, (wav * 32768.0).tolist())  # Kaldi int16 scale, as WeSpeaker does
    fb.input_finished()
    return np.array([fb.get_frame(i) for i in range(fb.num_frames_ready)], dtype=np.float32)

def reference_embedding(session, wav):
    feats = kaldi_fbank(wav)
    feats = feats - feats.mean(axis=0, keepdims=True)
    name = session.get_inputs()[0].name
    out = session.run(None, {name: feats[None, :, :]})[0][0]
    return out / (np.linalg.norm(out) + 1e-9)

def cosine(a, b):
    return float(np.dot(a, b) / (np.linalg.norm(a) * np.linalg.norm(b) + 1e-9))

def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--model", required=True); ap.add_argument("--data", required=True)
    ap.add_argument("--rust", default=None)
    args = ap.parse_args()
    session = ort.InferenceSession(args.model, providers=["CPUExecutionProvider"])
    print("onnx input:", [(i.name, i.shape) for i in session.get_inputs()])
    rows = list(csv.reader(open(f"{args.data}/clips.tsv", encoding="utf-8"), delimiter="\t", quoting=csv.QUOTE_NONE))
    rust = json.load(open(args.rust or f"{args.data}/embeddings-rust.json", encoding="utf-8"))
    refs, genders = {}, {}
    worst = 1.0
    for r in rows:
        key = f"{r[0]}/{r[1]}"
        emb = reference_embedding(session, load_clip(r[4]))
        refs[key] = emb; genders[key] = r[2]
        if key in rust:
            c = cosine(np.array(rust[key]), emb); worst = min(worst, c)
            print(f"{key:14} rust-vs-reference cosine {c:.4f}")
    keys = list(refs)
    same = []
    for r in rows:
        key = f"{r[0]}/{r[1]}"; wav = load_clip(r[4]); h = len(wav) // 2
        same.append(cosine(reference_embedding(session, wav[:h]), reference_embedding(session, wav[h:])))
    diff = [cosine(refs[a], refs[b]) for i, a in enumerate(keys) for b in keys[i+1:] if genders[a] != genders[b]]
    st = lambda v: f"mean {np.mean(v):.2f} min {np.min(v):.2f} max {np.max(v):.2f} (n={len(v)})"
    print("reference same-speaker (clip halves):", st(same))
    print("reference different-gender pairs:    ", st(diff))
    print("worst rust-vs-reference cosine:", f"{worst:.4f}")
    sys.exit(0 if worst > 0.95 else 1)

if __name__ == "__main__":
    main()
