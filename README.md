# 好好写剧本 · Write Your Script

A local-first screenplay editor for Mac, with Chinese and English interface options, screenplay/source views, a scene board, and optional AI assistance.

**Public beta 0.1.25.** Download the installer from [Releases](https://github.com/huangzongjingziliu-png/haohaoxie-screenplay/releases). Apple Silicon macOS is the tested platform. This is not a Mac App Store build.

[中文说明](README.zh-CN.md) · [Getting started](docs/public/GETTING_STARTED.md) · [Known limitations](docs/public/KNOWN_LIMITATIONS.md) · [Privacy](docs/public/PRIVACY.md)

## What it does

- Write in screenplay or source view, or use both side by side.
- Reorder scenes in the navigator or board; screenplay order follows.
- Customize paragraph styles and save named presets.
- Save `.screenplay` projects, use recovery and version history, and export PDF, Word, Fountain, FDX, text or HTML.
- Use optional model profiles and writing skills. Preview selection edits before applying them.
- Connect an external local agent through the bundled CLI or MCP.

PDF/image conversion and AI features are experimental. Inspect converted text and classifications before relying on them.

## Install

A tested DMG will be attached to a GitHub pre-release. Download it, open it and drag the app to Applications. Users do not need Rust, Node.js or Codex CLI. Do not treat a source ZIP as an installer.

Signing/notarization status will be stated in each release. Do not disable system security protections to install the app. Intel Mac compatibility has not been validated.

## Interface language

Open **More → Settings → General → Interface language**. Choose Follow system, 简体中文 or English. This does not translate your screenplay or change its layout. Some backend diagnostics and built-in skill descriptions remain Chinese in this beta.

## Build from source

On an Apple Silicon Mac, install Node.js 22+, a recent Rust stable toolchain and Apple's Swift/Xcode command-line tools.

```sh
source scripts/env.sh
npm ci
bash scripts/build_sidecars.sh
node scripts/prepare-agent-tools.mjs
npm test
cargo build --locked -p screenplay-testtool
cargo test --workspace --locked
npm run tauri:build
```

The build compiles the OCR/PDF helpers and bundled CLI/MCP. Artifacts appear under `target/release/bundle/`. No API key is needed to build or use the core editor. If your PATH uses an old Cargo, run `source scripts/env.sh` first.

## Feedback and commercial exploration

Report reproducible bugs through GitHub Issues. Include app version, macOS version, expected behavior and actual behavior. Use a made-up sample, never private scripts or API keys.

This beta does not charge users and makes no promise of future paid features. We are exploring whether users would pay for an official maintained distribution or support. Stars and download counts are not proof of demand.

## License

Project code is MIT licensed; see [LICENSE](LICENSE) and [THIRD_PARTY.md](THIRD_PARTY.md). Third-party assets retain their own licenses. A code license is not a grant of trademark rights; forks should avoid implying official affiliation. This release contains no user credentials or private project files.
