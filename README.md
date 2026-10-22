# Tamlil (תמליל)

Local-first meeting recorder, transcriber and summarizer for macOS, with first-class Hebrew support.

Tamlil is a fork of [Meetily](https://github.com/Zackriya-Solutions/meeting-minutes) (MIT). Everything runs on the machine: audio capture, speech-to-text (whisper.cpp / Parakeet) and summaries (llama.cpp built in, or any provider you configure). The upstream README is kept at [docs/UPSTREAM_README.md](docs/UPSTREAM_README.md) for build instructions and architecture notes.

## What Tamlil adds on top of Meetily

- **Model registry.** Transcription (whisper.cpp `.bin`) and summary (`.gguf`) models are no longer a hardcoded list. Add any model from a URL or a local file in the app, or drop a file into the models folder and it shows up. Custom GGUF models use the chat template embedded in the file.
- **Hebrew out of the box.** The [ivrit.ai](https://huggingface.co/ivrit-ai) whisper.cpp models are bundled in the catalog, ready to download.
- **Per-language model routing.** In *Settings → Language*, map a language to the transcription model and the summary model that work best for it. With the transcription language on *Auto Detect*, the first seconds of speech are identified and the routed models take over for the rest of the meeting.
- **Summaries written directly in the meeting language.** Routed languages skip the upstream English-then-translate pipeline.
- **Right-to-left text.** Transcript lines, meeting titles and the summary editor resolve their direction per paragraph, so Hebrew and mixed Hebrew/English content reads correctly.
- **No telemetry.** The upstream PostHog analytics (Rust client, consent UI and event tracking) are removed. The app only talks to the network to download models and to the AI providers you configure.

## Building

See [docs/UPSTREAM_README.md](docs/UPSTREAM_README.md) and [frontend/README.md](frontend/README.md). In short:

```bash
cd frontend
pnpm install
./clean_run.sh          # macOS dev build
./clean_build.sh        # production build
```

Model downloads need access to `huggingface.co` and its `cdn-lfs*` hosts.

The workspace `.cargo/config.toml` disables OpenSSL in the bundled llama.cpp build. Without it, the `llama-helper` sidecar fails to compile on macOS machines with a recent Homebrew OpenSSL (the vendored cpp-httplib does not build against it), and the sidecar never needs HTTPS anyway.

## Platforms

Tamlil inherits Meetily's platform support: macOS on Apple Silicon is the primary target (Metal and CoreML acceleration, ScreenCaptureKit system audio), Windows x64 is supported (Vulkan acceleration, WASAPI loopback), and Linux builds are possible but experimental (ALSA/PulseAudio, `.deb`/AppImage/RPM via the manual `build-linux.yml` workflow). None of the fork's additions are platform-specific.

## Continuous integration

`.github/workflows/ci.yml` runs on every push and pull request to `main`:

1. **check** (Linux): Rust unit tests for the Tauri crate and `llama-helper`, TypeScript type-check.
2. **build**: unsigned bundles for macOS (Apple Silicon) and Windows through the shared `build.yml`, uploaded as workflow artifacts (`tamlil-ci-*`).

`release.yml` (manual) creates a draft GitHub release with signed, notarised builds. Secrets it expects:

| Secret | Purpose |
| --- | --- |
| `TAURI_SIGNING_PRIVATE_KEY`, `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | Updater signatures. Generate with `pnpm tauri signer generate` and put the public key in `tauri.conf.json`. Without the secret, CI signs with a throwaway key. |
| `APPLE_CERTIFICATE`, `APPLE_CERTIFICATE_PASSWORD`, `KEYCHAIN_PASSWORD`, `APPLE_ID`, `APPLE_PASSWORD`, `APPLE_TEAM_ID` | macOS code signing and notarisation. |
| `SM_*` (DigiCert KeyLocker) | Windows code signing. |

## Updating from upstream

The `main` branch tracks Meetily with a thin diff: new code lives in its own modules (`model_registry`, `language_routing`) with small hook points in the engines.

```bash
git remote add upstream https://github.com/Zackriya-Solutions/meeting-minutes.git
git fetch upstream main
git rebase upstream/main   # or merge, per team preference
```

## Auto-update

`tauri.conf.json` points the updater at this repository's releases. Generate a new signing key pair before publishing releases and replace `plugins.updater.pubkey`; the upstream key is still in place and will reject builds signed with any other key.

## License

MIT, same as upstream. See [LICENSE.md](LICENSE.md).
