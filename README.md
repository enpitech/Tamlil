<div align="center">

<img src="docs/tamlil/banner.svg" alt="Tamlil — local-first meeting notes, Hebrew-first" width="100%">

<br>

**Record, transcribe and summarize meetings on your own machine.**
No cloud, no account, no telemetry. Built for Hebrew and mixed Hebrew/English meetings, works in 50+ languages.

<br>

<a href="https://github.com/enpitech/Tamlil/releases/download/latest/Tamlil-macos-arm64.dmg"><img src="docs/tamlil/btn-macos.svg" alt="Download for macOS (Apple Silicon, .dmg)" height="68"></a>
&nbsp;
<a href="https://github.com/enpitech/Tamlil/releases/download/latest/Tamlil-windows-x64-setup.exe"><img src="docs/tamlil/btn-windows.svg" alt="Download for Windows (x64 setup .exe)" height="68"></a>
&nbsp;
<a href="https://github.com/enpitech/Tamlil/releases/download/latest/Tamlil-linux-x64.AppImage"><img src="docs/tamlil/btn-linux.svg" alt="Download for Linux (x64 AppImage)" height="68"></a>

<a href="https://github.com/enpitech/Tamlil/releases/tag/latest"><img src="docs/tamlil/btn-all.svg" alt="All downloads" height="68"></a>

<sub>Latest build from <code>main</code>, refreshed by CI on every push. Also available: <a href="https://github.com/enpitech/Tamlil/releases/download/latest/Tamlil-windows-x64.msi">Windows .msi</a> · <a href="https://github.com/enpitech/Tamlil/releases/download/latest/Tamlil-linux-x64.deb">Linux .deb</a> · <a href="https://github.com/enpitech/Tamlil/releases">versioned releases</a></sub>

<br><br>

[![CI](https://github.com/enpitech/Tamlil/actions/workflows/ci.yml/badge.svg)](https://github.com/enpitech/Tamlil/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-0d9488.svg)](LICENSE.md)
[![Platforms](https://img.shields.io/badge/macOS%20%C2%B7%20Windows%20%C2%B7%20Linux-111827.svg)](#platforms)
[![Fork of Meetily](https://img.shields.io/badge/fork%20of-Meetily-6366f1.svg)](https://github.com/Zackriya-Solutions/meeting-minutes)

</div>

<br>

<p align="center">
  <img src="docs/tamlil/app-mockup.svg" alt="Tamlil: meetings sidebar, live transcript with Hebrew and English lines, and a summary written in Hebrew" width="100%">
</p>

<br>

## Why Tamlil

<table>
<tr>
<td width="33%" valign="top">

### 🔒 Everything stays local
Audio capture, speech-to-text (whisper.cpp / Parakeet) and summaries (llama.cpp built in) all run on the machine. No account, no server, no telemetry. Network access is only used to download models you ask for.

</td>
<td width="33%" valign="top">

### 🇮🇱 Hebrew that actually works
The [ivrit.ai](https://huggingface.co/ivrit-ai) whisper.cpp models ship in the catalog. Summaries can be written **directly in Hebrew**, and transcript, titles and editor render right-to-left and mixed-direction text correctly.

</td>
<td width="33%" valign="top">

### 🧩 Bring any model
Add a whisper.cpp `.bin` or an LLM `.gguf` from a URL or a local file inside the app, or drop it into the models folder. Custom LLMs use the chat template embedded in the file, so no code changes are needed.

</td>
</tr>
<tr>
<td valign="top">

### 🌐 Per-language routing
Map each language to the transcription model and summary model that work best for it. On *Auto Detect*, Tamlil identifies the language from the first seconds of speech and switches models for the rest of the meeting.

</td>
<td valign="top">

### ⚡ Hardware acceleration
Metal and CoreML on Apple Silicon, Vulkan on Windows, OpenBLAS on Linux. Choose quantized or full-precision models to match your machine.

</td>
<td valign="top">

### 🔁 Thin fork, easy upstream sync
Built on [Meetily](https://github.com/Zackriya-Solutions/meeting-minutes). Fork-specific code lives in its own modules with small hook points, so upstream improvements keep flowing in.

</td>
</tr>
</table>

<br>

## How language routing works

<p align="center">
  <img src="docs/tamlil/routing.svg" alt="Diagram: meeting audio goes through language identification; Hebrew is routed to ivrit.ai whisper and a Hebrew summary, English to the default models, other languages to the global settings" width="100%">
</p>

Set it up once in **Settings → Language → Models per language**. Add Hebrew and Tamlil pre-selects the ivrit.ai model; pick a summary model and whether summaries are written directly in Hebrew or produced in English and translated (the upstream behaviour). Languages without a route keep the global settings.

<br>

## Getting started

1. **Install** from the buttons above. On macOS the build is ad-hoc signed: right-click → **Open** on first launch, or run `xattr -dr com.apple.quarantine /Applications/Tamlil.app`. Windows SmartScreen warns once for unsigned installers.
2. **Download models** when prompted. For Hebrew pick *ivrit.ai Whisper Large v3 Turbo* for transcription and a built-in GGUF (Qwen 3.5 or Gemma 3) for summaries.
3. **Grant permissions** for the microphone and system audio (Screen Recording on macOS).
4. **Record.** Transcription streams live; the summary is generated when you stop.

Model downloads need access to `huggingface.co` and its `cdn-lfs*` hosts.

<br>

## Platforms

| Platform | Status | Acceleration | Notes |
| --- | --- | --- | --- |
| macOS (Apple Silicon) | Primary | Metal + CoreML | ScreenCaptureKit system audio. Intel Macs are not supported. |
| Windows 10/11 (x64) | Supported | Vulkan | WASAPI loopback for system audio. |
| Linux (x64) | Experimental | OpenBLAS | ALSA/PulseAudio. `.deb` and AppImage from CI. |

<br>

## Building from source

```bash
git clone https://github.com/enpitech/Tamlil.git
cd Tamlil/frontend
pnpm install
./clean_run.sh          # macOS development build (Metal)
./clean_build.sh        # production build
```

Toolchain: Rust stable, Node 20, pnpm. Windows and Linux use `build.bat` / the `build-gpu.*` scripts. Detailed notes, including GPU variants and the architecture, are in the upstream documentation kept at [docs/UPSTREAM_README.md](docs/UPSTREAM_README.md).

The workspace `.cargo/config.toml` disables OpenSSL in the bundled llama.cpp build. Without it the `llama-helper` sidecar fails to compile against recent Homebrew OpenSSL, and it never needs HTTPS anyway.

<br>

## Continuous integration and releases

[`ci.yml`](.github/workflows/ci.yml) runs on every push and pull request to `main`:

1. **check** (Linux): Rust unit tests for the Tauri crate and `llama-helper`, TypeScript type-check.
2. **build**: unsigned bundles for macOS (Apple Silicon), Windows and Linux through the shared [`build.yml`](.github/workflows/build.yml).
3. **publish** (pushes to `main` only): refreshes the rolling [`latest`](https://github.com/enpitech/Tamlil/releases/tag/latest) pre-release with every bundle plus the stable-named files the download buttons point to.

[`release.yml`](.github/workflows/release.yml) (manual) creates a versioned, signed and notarised release. Secrets it expects:

| Secret | Purpose |
| --- | --- |
| `TAURI_SIGNING_PRIVATE_KEY` | Updater signatures. The matching public key is in `tauri.conf.json`. Without it, CI signs with a throwaway key. |
| `APPLE_CERTIFICATE`, `APPLE_CERTIFICATE_PASSWORD`, `KEYCHAIN_PASSWORD`, `APPLE_ID`, `APPLE_PASSWORD`, `APPLE_TEAM_ID` | macOS code signing and notarisation. |
| `SM_*` (DigiCert KeyLocker) | Windows code signing. |

Keep an offline copy of the updater private key: if it is lost, installed copies can no longer verify updates and a new key means a manual reinstall for every user.

<br>

## Keeping up with upstream

`main` tracks Meetily with a thin diff. New behaviour lives in `model_registry/` and `language_routing/` with small hook points in the engines, and `llama-helper` understands chat messages for GGUF-embedded templates.

```bash
git remote add upstream https://github.com/Zackriya-Solutions/meeting-minutes.git
git fetch upstream main
git rebase upstream/main   # or merge, per team preference
```

<br>

## License

MIT, same as upstream Meetily by Zackriya Solutions. See [LICENSE.md](LICENSE.md).

<div align="center">
<sub>Tamlil (תמליל) means “transcript” in Hebrew.</sub>
</div>
