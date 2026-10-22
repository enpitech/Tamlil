# Releasing Tamlil

## Continuous builds

[`ci.yml`](../.github/workflows/ci.yml) runs on every push and pull request to `main`:

1. **check** (Linux): Rust unit tests for the Tauri crate and `llama-helper`, TypeScript type-check.
2. **build**: unsigned bundles for macOS (Apple Silicon), Windows and Linux through the shared [`build.yml`](../.github/workflows/build.yml).
3. **publish** (pushes to `main` only): recreates the rolling [`latest`](https://github.com/enpitech/Tamlil/releases/tag/latest) pre-release with every bundle, plus stable-named copies (`Tamlil-macos-arm64.dmg`, `Tamlil-windows-x64-setup.exe`, `Tamlil-windows-x64.msi`, `Tamlil-linux-x64.AppImage`, `Tamlil-linux-x64.deb`) that the README download buttons point to.

The build workflow runs the Tauri CLI directly for CI builds so compiler and bundler errors appear as annotations. Unsigned macOS builds are ad-hoc signed.

## Versioned releases

[`release.yml`](../.github/workflows/release.yml) (manual trigger) creates a draft GitHub release named after the version in `frontend/src-tauri/tauri.conf.json`, builds signed macOS and Windows bundles and uploads them with the updater manifest.

Secrets it expects:

| Secret | Purpose |
| --- | --- |
| `TAURI_SIGNING_PRIVATE_KEY` | Updater signatures. The matching public key is in `tauri.conf.json` (`plugins.updater.pubkey`). Without it, CI signs with a throwaway key and the bundles cannot be used as auto-updates. No password is set, so `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` can be omitted. |
| `APPLE_CERTIFICATE`, `APPLE_CERTIFICATE_PASSWORD`, `KEYCHAIN_PASSWORD`, `APPLE_ID`, `APPLE_PASSWORD`, `APPLE_TEAM_ID` | macOS code signing and notarisation. |
| `SM_HOST`, `SM_API_KEY`, `SM_CLIENT_CERT_FILE_B64`, `SM_CLIENT_CERT_PASSWORD`, `SM_CODE_SIGNING_CERT_SHA1_HASH` | Windows code signing through DigiCert KeyLocker. |

Keep an offline copy of the updater private key. If it is lost, a new key can be generated (`pnpm tauri signer generate`), but installs built with the old key will reject updates signed with the new one and need a manual reinstall.

## Build notes

- The workspace `.cargo/config.toml` disables OpenSSL in the bundled llama.cpp build (`CMAKE_DISABLE_FIND_PACKAGE_OpenSSL`). Without it the `llama-helper` sidecar fails to compile against recent Homebrew OpenSSL; the sidecar never needs HTTPS.
- Model downloads need access to `huggingface.co` and its `cdn-lfs*` hosts.
- Telemetry is removed; nothing phones home.
