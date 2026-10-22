<div align="center">

<img src="docs/tamlil/banner.svg" alt="Tamlil — local-first meeting notes, Hebrew-first" width="100%">

<br>

**Record, transcribe and summarize meetings on your own machine.**
No cloud, no account, no telemetry. Built for Hebrew and mixed Hebrew/English meetings, works in 50+ languages. Speakers are told apart by voice, each with its own avatar.

<br>

<a href="https://github.com/enpitech/Tamlil/releases/download/latest/Tamlil-macos-arm64.dmg"><img src="docs/tamlil/btn-macos.svg" alt="Download for macOS (Apple Silicon, .dmg)" height="64"></a>
&nbsp;
<a href="https://github.com/enpitech/Tamlil/releases/download/latest/Tamlil-windows-x64-setup.exe"><img src="docs/tamlil/btn-windows.svg" alt="Download for Windows (x64 setup .exe)" height="64"></a>
&nbsp;
<a href="https://github.com/enpitech/Tamlil/releases/download/latest/Tamlil-linux-x64.AppImage"><img src="docs/tamlil/btn-linux.svg" alt="Download for Linux (x64 AppImage)" height="64"></a>

<a href="https://github.com/enpitech/Tamlil/releases/tag/latest"><img src="docs/tamlil/btn-all.svg" alt="All downloads" height="64"></a>

<sub>Latest build from <code>main</code>, refreshed by CI on every push. Also available: <a href="https://github.com/enpitech/Tamlil/releases/download/latest/Tamlil-windows-x64.msi">Windows .msi</a> · <a href="https://github.com/enpitech/Tamlil/releases/download/latest/Tamlil-linux-x64.deb">Linux .deb</a> · <a href="https://github.com/enpitech/Tamlil/releases">versioned releases</a></sub>

<br><br>

[![CI](https://github.com/enpitech/Tamlil/actions/workflows/ci.yml/badge.svg)](https://github.com/enpitech/Tamlil/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-0d9488.svg)](LICENSE.md)
[![Fork of Meetily](https://img.shields.io/badge/fork%20of-Meetily-6366f1.svg)](https://github.com/Zackriya-Solutions/meeting-minutes)

</div>

<br>

<p align="center">
  <img src="docs/tamlil/app-mockup.svg" alt="Tamlil: meetings sidebar, live transcript with Hebrew and English lines, and a summary written in Hebrew" width="100%">
</p>

<br>

## How language routing works

<p align="center">
  <img src="docs/tamlil/routing.svg" alt="Diagram: meeting audio goes through language identification; Hebrew is routed to ivrit.ai whisper and a Hebrew summary, English to the default models, other languages to the global settings" width="100%">
</p>

In **Settings → Language** map each language to its transcription and summary models. On *Auto Detect*, Tamlil identifies the language from the first seconds of speech and switches models for the rest of the meeting; routed languages get summaries written directly in that language. Any whisper.cpp `.bin` or GGUF model can be added from a URL or a file.

- **Platforms**: macOS on Apple Silicon (primary), Windows x64, Linux x64 (experimental). Unsigned builds: on macOS right-click → Open on first launch.
- **Build from source**: `cd frontend && pnpm install && ./clean_run.sh`. Details and architecture in [docs/UPSTREAM_README.md](docs/UPSTREAM_README.md).
- **CI and releases**: every push tests, bundles and refreshes the `latest` pre-release; signed versioned releases are manual. See [docs/RELEASING.md](docs/RELEASING.md).
- **Upstream**: fork of [Meetily](https://github.com/Zackriya-Solutions/meeting-minutes) (MIT). Fork code lives in `model_registry/` and `language_routing/`; sync with `git fetch upstream main && git rebase upstream/main`.
