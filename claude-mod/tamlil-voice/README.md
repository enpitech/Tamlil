# tamlil-voice: talk to Claude Code in Hebrew

A Claude Code mod. `/listen` records your voice, transcribes it on your Mac with the
ivrit.ai whisper model Tamlil uses, and puts the Hebrew text in your prompt box.
Nothing leaves your machine.

## Install (macOS)

```bash
brew install sox whisper-cpp
claude plugin install tamlil-voice --marketplace enpitech/Tamlil
```

The model: in Tamlil, Settings → Models, download **ivrit.ai whisper large-v3 turbo**
(the mod finds it in `~/Library/Application Support/com.enpitech.tamlil/models`).
Tamlil's default Hebrew model is the streaming Zipformer, which whisper.cpp cannot run,
so this one download is needed. Without Tamlil:

```bash
curl -L -o ~/ggml-ivrit-large-v3-turbo.bin \
  https://huggingface.co/ivrit-ai/whisper-large-v3-turbo-ggml/resolve/main/ggml-model.bin
```

and set **Model file** to that path in `/config`.

The first `/listen` asks macOS for microphone access for your terminal app.

## Use

- `/listen`: speak; recording stops after 2 seconds of silence (or run `/listen` again).
- The transcript waits in the prompt box for you to fix and send.

## Settings (`/config`)

| Setting | Default | |
|---|---|---|
| Language | `he` | `auto` to detect |
| Model file | Tamlil's ivrit.ai model | any whisper.cpp `.bin` |
| Send at once | off | send the transcript without checking |
| Read replies aloud | off | speaks the answer to a spoken prompt |
| Voice | `Carmit` | the macOS Hebrew voice |
| Stop after silence | 2 s | 0 records until `/listen` again |
| Longest recording | 120 s | |

## Develop

```bash
claude plugin validate claude-mod/tamlil-voice
claude plugin test claude-mod/tamlil-voice
claude --plugin-dir claude-mod/tamlil-voice   # try it without installing
```
