# 好好写剧本 · Write Your Script

**A professional screenwriting workspace for the AI era.**

Writing a scene means juggling dialogue, paragraph types and indentation. Revising with AI often means copying text into a chat, then bringing the results back. Rearranging a long script adds another round of scrolling and cutting.

Write Your Script keeps the work in one project. Write freely while watching the formatted page, arrange scenes as cards, and ask the assistant to revise a passage. Review the changes before applying them.

**Public beta 0.1.35 · Apple Silicon Mac · Chinese / English UI · MIT open source.**

[Download the app](https://github.com/huangzongjingziliu-png/haohaoxie-screenplay/releases/tag/v0.1.35-beta.1) · [中文](README.zh-CN.md) · [Getting started](docs/public/GETTING_STARTED.md) · [Privacy](docs/public/PRIVACY.md) · [Known limitations](docs/public/KNOWN_LIMITATIONS.md)

## Get the scene down. See the formatted page.

Free text and screenplay views share the same content. Edit either side and the other updates. Scrolling and paragraph highlights stay matched, so finding your place takes less effort.

Start in free-writing mode if formatting is unfamiliar, or write directly on the screenplay page. Keep either view, swap sides, and adjust free-text size with a slider. Correct ambiguous character, action or dialogue classifications with the paragraph switcher.

![Free text and formatted screenplay with matching paragraph highlights](docs/public/images/split-writing.png)

## One scene per card. See the whole story.

The board, scene navigator and screenplay share scene order. Move a card and its scene moves in the script.

Double-click to edit titles and summaries in place. Titles update the screenplay. Write summaries yourself or ask a model to generate them; automatic summaries preserve manual edits. Group cards, add comments and connections, change colors, or zoom out. A corner shortcut takes you back to that scene in the editor.

![Scene cards on the board share their order with the screenplay](docs/public/images/scene-board.png)

## Ask for help with the passage you are editing

The writing assistant sits beside your script. Select a passage or scene for polishing, dialogue revision or continuation, then preview the changes before applying them. Discuss characters and plot, or attach material for transcription.

Use built-in writing skills, add your own, or have a model draft a skill for you to review and install. The assistant offers options. You choose what belongs in the script.

![The writing assistant beside the screenplay, with skill and model selectors](docs/public/images/writing-assistant.png)

## Choose your model. Keep your key saved.

Save multiple custom model profiles and switch between them. Configure OpenAI, compatible endpoints or a local Ollama service. Keys stay in an encrypted local vault and are excluded from project files.

Core editing needs no model. AI features use your configured provider, with its fees and limits. No shared key is supplied. Cloud requests send relevant screenplay text, memory and skill content to that provider. Ollama requires a separately installed and running local service.

![Saved custom model profiles and the active model selection](docs/public/images/custom-models.png)

## Bring the agent you already use

The installer includes **CLI and MCP** tools. Once connected, local tool-capable clients such as Codex, WorkBuddy and TRAE can read scenes, revise dialogue and work on structure. Stay in your usual agent and apply its work to the screenplay project.

The app includes setup guidance. Your client must support local tools or MCP and have the connection configured. Pasting configuration into an ordinary chat does not establish a connection. Using these tools does not require an additional Codex CLI installation.

## Project memory travels with the script

With a usable model configured, project memory can automatically organize story facts, creative decisions, suggestions and unresolved conflicts. Author-locked notes remain under your control, and up to 20 previous automatic versions can be restored. Memory is saved inside `.screenplay` projects and can be excluded from shared copies.

Personalization holds preferences you write yourself. It stays on your Mac and applies across projects. Automatic memory can be disabled, and its results need your review.

## The everyday tools are here too

- Screenplay and Chinese reading layouts, with fonts, size, weight, color and italics by paragraph type, plus named style presets.
- Autosave, recovery, version history, recent projects and a choice of save location. Try writing without saving first.
- Search and scene, character, location and outline navigation. Highlights follow the caret; characters and locations can list all occurrences.
- Import text, Markdown, PDFs and images. Export PDF, Word, Fountain, FDX, plain text and HTML.

PDF/image transcription requires a configured model and permission to use its service. OCR and AI conversion can omit or misclassify content. Check the original, and keep backups alongside autosave.

## Get started

1. Download the **DMG** from the release and drag the app into Applications. A source ZIP is not an installer.
2. Create or open a project; choose screenplay, free-writing or split view.
3. For AI, add your model under **More → Settings → Models & skills**. Language and personalization also live in Settings.

Core editing needs no model, Rust, Node.js or Codex CLI. External agents require a client that supports local tools or MCP and a configured connection.

This Apple Silicon beta has **no Developer ID signature or Apple notarization**. macOS may require approval in System Settings; do not disable security protections. Intel, Windows and Linux are not release-tested. This is not a Mac App Store build. Some backend messages and skill descriptions remain Chinese.

Read the [privacy notes](docs/public/PRIVACY.md) for details on data processing.

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

Report bugs in [Issues](https://github.com/huangzongjingziliu-png/haohaoxie-screenplay/issues) with the app version, macOS version, steps and expected result. Use a small invented script and keep private manuscripts and API keys out of reports.

The beta is free. We want to learn whether it earns a place in everyday writing before exploring maintained distributions or paid support. Stars are nice; returning writers tell us more.

Code is [MIT licensed](LICENSE); see [THIRD_PARTY.md](THIRD_PARTY.md). Forks should not imply official affiliation.
