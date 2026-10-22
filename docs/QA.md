# End-to-end QA

Real models, real Hebrew speech, the app's own engines. Two parts:

- `frontend/src-tauri/tests/qa_e2e.rs` – recognition suite (ignored tests). Runs
  the pipeline's VAD over a simulated meeting, transcribes each segment with the
  ivrit.ai model exactly as the worker does, scores word error rate against the
  reference, measures real-time factor, checks language identification and
  speaker clustering.
- `qa/summary_probe.py` – drives `llama-helper` with a GGUF model to summarise
  `qa/hebrew_meeting.txt` in Hebrew and scores the result (Hebrew ratio,
  headings, facts, speed).

Speech comes from Google FLEURS (CC-BY 4.0) via `qa/prepare_fleurs.py`; its
Hebrew split is read by male voices only, so the speaker tests add English
female voices for a second speaker (speaker embeddings are language-agnostic).

## Run it in CI

`.github/workflows/qa.yml` runs on every push that touches `qa/`, the harness or
the workflow, and on demand (Actions → QA → Run workflow). It downloads the
models, prepares the corpus (both cached), runs the suite on macOS arm64 and
Linux, and the summary probe on Linux. Each job force-pushes its report to a
branch: `qa/speech-macos`, `qa/speech-linux`, `qa/summary-linux`
(`report.md`, plus `run.log` for the speech jobs). The report is also in the
job summary.

## Run it locally

```bash
mkdir -p qa-models && cd qa-models
curl -L -o ggml-ivrit-large-v3-turbo.bin https://huggingface.co/ivrit-ai/whisper-large-v3-turbo-ggml/resolve/main/ggml-model.bin
curl -L -o ggml-base.bin https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base.bin
curl -L -o wespeaker-voxceleb-campplus-lm.onnx "https://huggingface.co/Wespeaker/wespeaker-voxceleb-campplus-LM/resolve/c5e01c6fcffcce160861e7e79782828320192b5c/voxceleb_CAM%2B%2B_LM.onnx"
cd .. && python3 qa/prepare_fleurs.py qa-data 10

cd frontend/src-tauri
TAMLIL_QA_MODELS=$PWD/../../qa-models TAMLIL_QA_DATA=$PWD/../../qa-data \
TAMLIL_QA_SPEAKER_MODEL=$PWD/../../qa-models/wespeaker-voxceleb-campplus-lm.onnx \
TAMLIL_QA_REPORT=$PWD/../../qa-report.md \
cargo test --release --test qa_e2e -- --ignored --nocapture --test-threads=1
```

Summary probe (needs a built `llama-helper` and a GGUF file):

```bash
python3 qa/summary_probe.py --helper llama-helper/target/release/llama-helper \
  --model qa-models/gemma-4-E4B_q4_0-it.gguf --transcript qa/hebrew_meeting.txt \
  --report qa-report.md --label "Gemma 4 E4B"
```

## What "good" looks like

- Per-clip WER with ivrit large-v3-turbo on FLEURS read speech: well under 20%.
  The meeting simulation (VAD + per-segment decoding, as in the app) should stay
  within a few points of the per-clip number; a large gap means segmentation is
  hurting recognition.
- RTF (compute seconds per second of speech): below 1.0 on the target machine,
  or transcription falls behind during a meeting. CI numbers are CPU-only and
  only comparable run to run.
- Language identification: the general model must be right on every clip; the
  Hebrew fine-tune is expected to answer "he" for everything, which is why it is
  never used for identification when a general model is on disk.
- Speaker clustering: the two halves of a clip get the same label; female and
  male voices never share a label; at most a couple of spurious labels.
- Summary: Hebrew ratio ≥ 0.85, the four headings present, no reasoning
  markers, the named people and decisions appear.
