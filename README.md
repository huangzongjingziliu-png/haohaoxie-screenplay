# 好好写剧本 · Write Your Script

**A professional screenwriting workspace for the AI era.**

A scene is taking shape. Now is a terrible time to argue with indentation—or ferry half a screenplay into a chat window.

Write Your Script brings free writing, screenplay layout, scene planning and AI assistance into one project. Formatting gets a home. Revisions get a preview. The story stays yours.

**Public beta 0.1.35 · Apple Silicon Mac · Chinese / English UI · MIT open source.**

[Download the app](https://github.com/huangzongjingziliu-png/haohaoxie-screenplay/releases/tag/v0.1.35-beta.1) · [中文](README.zh-CN.md) · [Getting started](docs/public/GETTING_STARTED.md) · [Privacy](docs/public/PRIVACY.md) · [Known limitations](docs/public/KNOWN_LIMITATIONS.md)

![Welcome screen and recent projects in Write Your Script](docs/public/images/welcome.png)

## Less shuttling. More writing.

- **Write freely, see screenplay layout.** Free text and screenplay views share edits, corresponding scrolling and paragraph highlights. Keep either view, swap sides, or adjust source text size independently. Start with the story, then correct ambiguous paragraph classifications.
- **Bring your usual agent.** The installer bundles CLI and MCP tools for local, tool-capable clients such as Codex, WorkBuddy and TRAE. Once configured, they can read scenes, revise dialogue and work on structure. Pasting configuration into a chat is not a connection; the app includes setup guidance.
- **Keep the writing assistant in the project.** Choose and switch models, use writing skills, transcribe attachments and preview selected revisions before applying them. A model can draft a new skill; you review it before installation.
- **Lay out the story, move it together.** Board cards, the scene navigator and screenplay share scene order. Reorder, group, comment, connect and zoom. Double-click to edit titles and summaries in place; titles update the screenplay. Write summaries yourself or generate them with a model—manual summaries are protected from automatic replacement.
- **Let the project carry its memory.** With a usable model configured, automatic project memory organizes story facts, decisions, suggestions and unresolved conflicts. Author-locked notes stay under your control. Restore up to 20 previous automatic versions. Memory travels in `.screenplay` files and can be excluded from shared copies. Personalization is user-written, local and shared across your projects.
- **Find your place faster.** Scene, location and outline highlights follow the caret. Characters and locations jump to their nearest occurrence or expand into a list of positions. Search, recent projects, a collapsible navigator and a writing area that accommodates the assistant help you return to the scene at hand.

Not every feature here is unique. The point is how they work together: **you write, the app formats; you decide, AI assists.**

## Layout and files

Use screenplay or Chinese reading layouts, customize fonts, size, weight, color and italics by paragraph type, and save named style presets. Interface text size remains independent of page zoom.

Save `.screenplay` projects with autosave, recovery and version history. Export PDF, Word, Fountain, FDX, plain text or HTML. Choose a save location for a new project, or try writing without saving first. Save to a file to create a portable project. Autosave is not a substitute for backups.

Import text, Markdown, PDFs and images. PDF/image transcription requires a configured model and permission to use the relevant service. OCR and AI conversion can omit or misclassify content; check against the original.

## Get started

1. Download the **DMG** from the release and drag the app into Applications. A source ZIP is not an installer.
2. Create or open a project; choose screenplay, free-writing or split view.
3. For AI, add your model under **More → Settings → Models & skills**. Language and personalization also live in Settings.

Core editing needs no model, Rust, Node.js or Codex CLI. External agents require a client that supports local tools or MCP and a configured connection.

This Apple Silicon beta has **no Developer ID signature or Apple notarization**. macOS may require approval in System Settings; do not disable security protections. Intel, Windows and Linux are not release-tested. This is not a Mac App Store build. Some backend messages and skill descriptions remain Chinese.

## AI, with clear boundaries

Bring your own provider; its fees and limits apply. No shared keys or unlimited-free-model promise. Cloud requests send relevant screenplay text, memory and skill content to your configured service. Local Ollama requires a separately installed and running model service.

Review revisions and automatic memory. An assistant suggestion is not an author-confirmed decision. Project memory travels with the file; personal preferences do not. Read the [privacy notes](docs/public/PRIVACY.md).

## Build from source

On Apple Silicon macOS, install Node.js 22+, recent Rust stable and Apple's Swift/Xcode command-line tools.

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

The build includes OCR/PDF helpers and CLI/MCP tools. Artifacts appear in `target/release/bundle/`. No API key is needed to build or use core editing.

## Help make it a daily tool

Report bugs in [Issues](https://github.com/huangzongjingziliu-png/haohaoxie-screenplay/issues) with the app version, macOS version, steps and expected result. Use a small invented script—never private manuscripts or API keys.

The beta is free. We want to learn whether it earns a place in everyday writing before exploring maintained distributions or paid support. Stars are nice; returning writers tell us more.

Code is [MIT licensed](LICENSE); see [THIRD_PARTY.md](THIRD_PARTY.md). Forks should not imply official affiliation.
