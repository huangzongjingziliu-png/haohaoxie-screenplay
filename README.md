# 好好写剧本 · Write Your Script

**A screenwriting tool built for the AI era.**

Formatting interrupts a good writing streak. Asking AI to revise a scene means shuttling text between chat and a document. As the story grows, scene order and revision notes get harder to keep together.

Write Your Script brings free writing and screenplay layout into sync, turns scenes into reorderable cards with notes, and lets your usual AI assistant read and edit the project. The story stays yours.

A local-first Mac app with Chinese and English interfaces. AI is optional: choose your own model, or simply write.

**Public beta 0.1.32.** Download the installer from [Releases](https://github.com/huangzongjingziliu-png/haohaoxie-screenplay/releases). Apple Silicon macOS is the tested platform. This is not a Mac App Store build.

[中文说明](README.zh-CN.md) · [Getting started](docs/public/GETTING_STARTED.md) · [Known limitations](docs/public/KNOWN_LIMITATIONS.md) · [Privacy](docs/public/PRIVACY.md)

## Why Write Your Script?

**Get the story down, then shape it into a screenplay. Write it yourself, or ask AI to help revise it.**

### Three core ideas

1. **Let your usual AI assistant work on the screenplay.** The bundled CLI and MCP give local, tool-capable clients such as Codex, WorkBuddy and TRAE a way to read scenes and edit the project, reducing copy-and-paste between chat and editor. The client must be configured first; pasting configuration text into a conversation does not establish a connection.
2. **Write your way and see screenplay formatting alongside it.** Use source mode with a synchronized screenplay view, or show either view on its own. Start with free text when you want to focus on the story, then inspect paragraph types and layout. Ambiguous classifications still need correction; arbitrary prose is not automatically turned into a complete screenplay.
3. **A writing assistant inside the project, with a model you choose.** Discuss plot, polish dialogue, expand scenes or work on selected passages without leaving the editor. Save multiple model profiles and combine them with writing skills. Editable project memory records work on the project; system memory records preferences you explicitly ask the assistant to remember. Selection edits can be previewed before applying. AI is optional; when a cloud model is used, relevant content is sent to your configured service.

### Three practical benefits

- **See the story as scenes.** Each scene has a board card. The board, scene navigator and screenplay share the same order, so reordering scenes updates the text too. Group cards, add comments and draw annotated connections to organize revision ideas. This is an ordered scene board, not a freeform canvas. Write and edit scene summaries yourself, or optionally generate them with a model. Automatic generation will not overwrite your own summaries.
- **Keep your files and room to revise.** Save local `.screenplay` projects, use version history and recovery, and export in multiple formats. Core editing works without AI. Review selected AI revisions before applying them. Memories currently stay on the local machine and do not automatically travel with project files.
- **Make the layout suit your workflow.** Customize fonts, sizes, weight and color by paragraph type, then save named presets. Interface text size is independent of page text size, keeping controls readable when the page is zoomed out.

### How is it different?

Scene boards, revision history and formatting controls are not unique to this app. The focus is the combination of **synchronized source and screenplay views, tool access for external agents, and an in-project writing assistant**. It supports starting with the story and checking its format afterward, alongside AI-assisted revision in the same project.

This public beta does not claim the full feature set or reliability of established professional screenplay applications. Review PDF/image transcription, paragraph classification and AI suggestions yourself.

## What it does

- Write in screenplay or source view, or use both side by side.
- Reorder scenes in the navigator or board; screenplay order follows.
- Customize paragraph styles and save named presets.
- Save `.screenplay` projects, use recovery and version history, and export PDF, Word, Fountain, FDX, text or HTML.
- Use optional model profiles and writing skills. Preview selection edits before applying them.
- Connect an external local agent through the bundled CLI or MCP.

PDF/image conversion and AI features are experimental. Inspect converted text and classifications before relying on them.

## Install

The GitHub pre-release includes a tested DMG. Download it, open it and drag the app to Applications. Users do not need Rust, Node.js or Codex CLI. Do not treat a source ZIP as an installer.

Signing/notarization status will be stated in each release. Do not disable system security protections to install the app. Intel Mac compatibility has not been validated.

## Interface language

Open **More → Settings → Language → Interface language**. Choose Follow system, 简体中文 or English. This does not translate your screenplay or change its layout. Some backend diagnostics and built-in skill descriptions remain Chinese in this beta.

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
