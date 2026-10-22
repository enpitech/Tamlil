#!/usr/bin/env python3
"""Fetch real Hebrew (and a little English) speech with reference transcripts
for the end-to-end QA harness.

Source: Google FLEURS (CC-BY 4.0), dev split. Clips are written as raw
32-bit float PCM, 16 kHz mono, next to a `clips.tsv` index:

    lang  id  gender  seconds  path  transcription
"""
import csv, io, os, subprocess, sys, tarfile, urllib.request

BASE = "https://huggingface.co/datasets/google/fleurs/resolve/main/data"

def fetch(url, dest):
    if os.path.exists(dest):
        return dest
    print(f"downloading {url}", flush=True)
    tmp = dest + ".part"
    with urllib.request.urlopen(url, timeout=120) as r, open(tmp, "wb") as f:
        while True:
            b = r.read(1 << 20)
            if not b:
                break
            f.write(b)
    os.rename(tmp, dest)
    return dest

def load_index(tsv_path):
    rows = {}
    with open(tsv_path, encoding="utf-8", newline="") as f:
        for row in csv.reader(f, delimiter="\t", quoting=csv.QUOTE_NONE):
            if len(row) < 7:
                continue
            rows[row[1]] = {"id": row[0], "raw": row[2], "norm": row[3], "gender": row[6].strip().upper()}
    return rows

def to_f32(wav_bytes, out_path):
    p = subprocess.run(
        # Loudness-normalise to -23 LUFS (EBU R128), as the app's microphone path
        # does before VAD; FLEURS clips vary from -9 to -65 dBFS.
        ["ffmpeg", "-v", "error", "-i", "pipe:0", "-af", "loudnorm=I=-23:TP=-2:LRA=11",
         "-f", "f32le", "-ac", "1", "-ar", "16000", "-y", out_path],
        input=wav_bytes, capture_output=True)
    if p.returncode != 0:
        raise RuntimeError(p.stderr.decode(errors="replace"))
    return os.path.getsize(out_path) / 4 / 16000

def collect(lang, want_per_gender, out_dir, cache_dir):
    tsv = fetch(f"{BASE}/{lang}/dev.tsv", os.path.join(cache_dir, f"{lang}-dev.tsv"))
    tar_path = fetch(f"{BASE}/{lang}/audio/dev.tar.gz", os.path.join(cache_dir, f"{lang}-dev.tar.gz"))
    index = load_index(tsv)
    taken = {"FEMALE": 0, "MALE": 0}
    out = []
    with tarfile.open(tar_path, "r:gz") as tar:
        for member in tar:
            if not member.isfile() or not member.name.endswith(".wav"):
                continue
            name = os.path.basename(member.name)
            meta = index.get(name)
            if not meta or meta["gender"] not in taken or taken[meta["gender"]] >= want_per_gender:
                continue
            data = tar.extractfile(member).read()
            out_path = os.path.abspath(os.path.join(out_dir, f"{lang}-{meta['id']}.f32"))
            seconds = to_f32(data, out_path)
            if seconds < 3.0 or seconds > 20.0:
                os.remove(out_path)
                continue
            taken[meta["gender"]] += 1
            out.append((lang, meta["id"], meta["gender"], f"{seconds:.2f}", out_path, meta["raw"]))
            if all(v >= want_per_gender for v in taken.values()):
                break
    print(f"{lang}: {taken}", flush=True)
    return out

def main():
    out_dir = sys.argv[1] if len(sys.argv) > 1 else "qa-data"
    per_gender = int(sys.argv[2]) if len(sys.argv) > 2 else 10
    cache_dir = os.path.join(out_dir, "cache")
    os.makedirs(cache_dir, exist_ok=True)
    rows = collect("he_il", per_gender, out_dir, cache_dir) + collect("en_us", max(5, per_gender // 2), out_dir, cache_dir)
    with open(os.path.join(out_dir, "clips.tsv"), "w", encoding="utf-8", newline="") as f:
        w = csv.writer(f, delimiter="\t", quoting=csv.QUOTE_NONE, escapechar="\\")
        for r in rows:
            w.writerow(r)
    print(f"wrote {len(rows)} clips to {out_dir}/clips.tsv")

if __name__ == "__main__":
    main()
