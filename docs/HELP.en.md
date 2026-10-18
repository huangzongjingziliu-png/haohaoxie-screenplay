# Screenplay Studio guide

## Start with one scene

Welcome to Screenplay Studio. Give your characters something worth doing; let the app handle the formatting. Start with a short scene and learn the other tools as you need them.

Writing, manual board planning, saving and exporting work without a model. The writing assistant, automatic scene summaries, automatic project memory, and the current PDF/image import workflow require model configuration.

### Create a project and choose its home

Click New, enter a title and choose a save location in the dialog. A dedicated screenplay folder will make later drafts easier to find.

### Pick a writing view

Use Screenplay for formatted writing, Free text for drafting, or Split view to see both. Position and selection are synchronized between the two views.

### Save and reopen

Editable projects use the .screenplay extension. Reopen them from Recent projects or by double-clicking the project file.

## Writing and formatting

Place the caret in a paragraph to show its type selector. Choose action, character, parenthetical, dialogue or another type to change that paragraph.

Parentheticals such as “quietly” usually go below the character. Voice qualifiers such as V.O. stay beside the name. Brackets appear automatically. The app adds CONT'D when the same character resumes after action, and handles dialogue continued across a page.

Recognizable scene headings are standardized according to your layout settings. Complete the location and time before checking the result.

### Keep your hands on the keyboard

Open the type menu to see shortcuts. Tab cycles common types; customize shortcuts in Text & layout settings.

### Make the page comfortable

The Free text slider changes its font size and can reset it. Set screenplay fonts, sizes and paragraph styles in Settings, under Text & layout. Save named presets for later use.

### Find a line

Use Search or ⌘F. Enter a name, line or memorable word and jump to a result. Press Esc to leave Focus mode.

## Series and episodes

One .screenplay project can contain a whole series. Open Episodes in the toolbar and enable episodes. Existing content becomes Episode 1. Each episode has its own script and board, with shared project settings and memory.

Use the episode selector to switch. Manage episodes to rename, move up or down, or delete. Undo restores deletion. Pending edits are committed before switching; remember to save the project.

The toolbar exports the current episode as PDF, Word, Markdown, FDX and more. Export all episodes creates one numbered file per episode. Choose an empty folder to avoid overwriting drafts. Share the .screenplay project to send the complete series.

Use an episode-capable version to edit series projects. Older versions may recognize only the current episode.

The assistant and external CLI/MCP work on the current episode by default. Switch to the intended episode before requesting revisions.

## Planning, continuity and draft revisions

Open Episodes → Season and episode overview. Write the season outline, then select an episode to enter its synopsis, conflict, turning point, cliffhanger and progress. Save planning to keep it in the project. Planning does not automatically become script text.

Continuity checking needs a configured model. Select 2–8 episodes per batch. The app reads scripts and planning in chunks before comparing characters, timeline and setups. Relevant content is sent to your provider, which may charge. Progress is visible and checking can be cancelled.

Findings are questions for the author, not verdicts. Quotes are checked against sources. Click a quote to open its script passage or planning field. Mark findings as to review, resolved or dismissed. Changed source material flags a report as potentially stale. Checks never rewrite scripts and do not guarantee finding every issue.

In More → Version history, enter a draft label such as V2.1 and create a snapshot. Later edits do not change that snapshot. The toolbar displays the current label; exports include it in file names and title pages. Timestamps distinguish snapshots with the same label.

Compare two snapshots, or a snapshot with current content. Red shows earlier text and green shows later text. Episodes are matched by identity; switching the active episode does not count as rewriting it. Restoring first backs up current work.

Edit this data with version 0.1.49 or newer. Older versions may lose planning, reports or draft labels when saving.

## Save, import and export

Keep the .screenplay project for future editing. Export PDF or Word when you need to deliver a script. Your next round of revisions will thank you for keeping the project.

Saved projects default to autosaving every 30 seconds and creating an automatic version every five minutes. Choose a project location for your first save. Check the status bar and resolve any save error.

Prefer FDX or Fountain when available because they carry screenplay structure. PDF and image imports require text and layout recognition; poor scans, unusual fonts and headers can cause errors. Check speakers, dialogue, parentheticals and scenes after import. Word imports preserve common breaks, indentation and some screenplay paragraph styles, but still need review.

### Save or make a copy

⌘S saves the project. Save as in More lets you choose another file or location. A failed save is reported as a failure.

### Go back to an earlier draft

Open History in the toolbar to browse the timeline and filter named or automatic snapshots. Preview is read-only; compare shows changes. Restoring first preserves your current draft as a checkpoint and can be undone. Save the project to retain its history.

### Choose carefully when closing

If there are unsaved changes, the dialog offers the applicable save, discard or recovery-copy choices. A recovery copy is separate from a formally saved project.

### Check your export

Choose Export, then a format and destination. Review page breaks, names and dialogue, particularly after PDF or image import.

## Build the story on the board

Each card represents one scene. Start from an existing script or add cards before you write the scenes.

Add card creates a scene heading in the screenplay. The summary stays on the board. Reordering cards also reorders the screenplay and scene navigator.

### Edit directly

Double-click the title or summary and type. Clicking outside commits the edit. Title changes synchronize with the screenplay. Scroll inside a card to read a long summary.

### Group and connect

Command-click multiple cards or drag a selection box, then group them. Select two cards to connect them. Double-click a connection to add text; hide connections with the bottom-right control.

### Make room

Change colors or resize a card from its corner. Hold the right mouse button and scroll, or pinch on a trackpad, to zoom. Two-finger scrolling pans the view. The arrow jumps to the scene; the speech bubble opens comments.

### Write your own summaries

Summaries can be edited manually. With a model configured, automatic summaries can be enabled; scene text is sent to that model.

## Work with the writing assistant

Configure a model, open the writing assistant, choose a skill and send a clear request. Try a short passage first. If the dialogue starts sounding like a committee meeting, ask for another pass.

State the change you want and what to preserve. For example, ask for shorter dialogue while keeping the conflict, cast and ending. Right-click selected text, a scene or a card for local editing, then review the preview before applying it.

### Skills are reusable instructions

Choose built-in polishing, expansion or dialogue skills. Install a folder containing SKILL.md from Settings, or use Create skill to generate a draft, review it and install it.

### Project memory travels with the project

With a model configured and automatic organization enabled, project memory is maintained and saved in the project. Inspect it in Settings and use author notes for confirmed facts. Review automatic updates.

### Personalization is yours to write

Add preferences in Settings, under Personalization. They apply to projects on this computer, are not shared with the project and are not rewritten automatically by the model.

### Know where text goes

Cloud-model features send relevant screenplay text to your chosen provider and may incur provider charges. Keys stay on this computer and are not included when sharing a project.

## Connect a model, step by step

A custom model is the brain you choose for the writing assistant. Pick a provider and model; the app sends your request and brings back the answer. No programming or model training required.

Three details connect a cloud model. API Key is your access pass; Base URL is the service address; Model ID identifies the exact assistant. Give the profile any display name, but copy the three connection details from the same provider.

Choose Try free models for a guided start. Choose service → Connect → Choose model fills the address, fetches model IDs and lets you test the connection. Unknown pricing is never labelled free.

English defaults to international services; Chinese defaults to China-based services. You can switch either way. OpenRouter offers a free router and Groq a limited free plan; China-based SiliconFlow has some free models. You still need your own provider account and API key. Availability, limits and terms can change.

For no cloud API key, choose Local Ollama. Install and open Ollama, download a local chat model through its interface, then click Find local models here. Model weights are not bundled and no large download starts automatically. Local models use your disk, memory and power.

Free models can queue, hit limits, time out or disappear. Try short tasks and keep a reliable alternative for important drafts. The app has no shared cloud API key and never silently switches to a paid model.

Get three things from one provider. The API Key authorizes requests; Base URL is the API endpoint supplied by the provider; Model ID is the exact model identifier. Keep all three from the same service.

Display name can be anything useful, such as “Dialogue helper”. Model ID must match the provider, including case, slashes and suffixes. A chat-site password is not an API Key.

For English-speaking users, the following international services offer English-language setup guides. Check availability in your country and current pricing before signing up. Local Ollama is also an option if you prefer downloaded models.

### 1. Open model settings

Open Settings → Models & skills → Manage custom models, then Add custom model or Try free models.

### 2. Choose a service and connect

Choose a regional service card. Its address is filled automatically; name the profile and paste your own API key. Local Ollama needs no key. Custom API lets you enter another endpoint.

### 3. Fetch and choose a model

Click Fetch models, search and choose an exact model ID, or enter one manually. The free filter includes only explicitly free models; verify unknown pricing with the provider. Use the documented Base URL, not a chat webpage, without appending /chat/completions.

### 4. Test the connection

Click Test connection. A successful API response confirms that this request worked. The test sends one short message, not your screenplay, and may still incur a provider charge. It does not measure writing quality.

### 5. Save and allow cloud processing

If you want cloud processing, check Allow this cloud model to process relevant screenplay text, then Save & use. You can change cloud permission later in Settings → Models & skills. Local Ollama needs no cloud permission.

### 6. Try a short request

Open the writing assistant, check the selected model at the bottom and send a small task before trying a long rewrite.

### 7. Keep more than one model

Add other profiles and switch with the model selector. Keys are remembered on this computer; leaving the key blank while editing keeps it. Configure models again on another computer because project files do not carry keys.

### OpenAI

OpenAI is the developer platform for GPT models. Create an API key, check API billing, choose OpenAI in the app and enter a supported model ID. No Base URL field is needed. A ChatGPT subscription does not include API credit.

[Official website / 官方入口](https://platform.openai.com) · [Guide / 官方说明](https://help.openai.com/en/articles/9039756-managing-billing-for-chatgpt-and-the-api-platform)

### OpenRouter

OpenRouter brings models from multiple providers into one API, useful for comparing models without setting up a separate account for each one. Create an API key in Settings → Keys. Choose OpenAI-compatible service, use https://openrouter.ai/api/v1, and copy the exact model ID from its model page. Check the selected model’s pricing and data policy.

[Official website / 官方入口](https://openrouter.ai/settings/keys) · [Guide / 官方说明](https://openrouter.ai/docs/quickstart)

### Groq

Groq hosts supported models through its developer API. Create a key in the console and choose a text-chat model listed in the documentation. In the app, choose OpenAI-compatible service and use https://api.groq.com/openai/v1. Copy the exact model ID and check current usage limits and pricing.

[Official website / 官方入口](https://console.groq.com/keys) · [Guide / 官方说明](https://console.groq.com/docs/quickstart)

### Local Ollama

Install and run Ollama, then download a local text-chat model using its official instructions. Choose Local Ollama in the app and enter the installed model name including its tag. No API Key is required. Choose a downloaded local model rather than a cloud-backed one. Speed and memory needs depend on your hardware and model.

[Official website / 官方入口](https://ollama.com/download) · [Guide / 官方说明](https://docs.ollama.com/api/openai-compatibility)

[Find and create an OpenAI API key](https://help.openai.com/en/articles/4936850-where-do-i-find-my-openai-api-key)

## When something goes wrong

### 401 or invalid key

Copy the key again. Check extra spaces, revoked keys and provider mismatch. Never send anyone the complete key.

### 404 or model not found

Check the Base URL and full model ID, and confirm your account has access. A display name may differ from its API ID.

### 429, low balance or exhausted quota

Check the provider balance and request limits. Wait as instructed or select another configured model, then test its connection.

### Timeout

Check network, endpoint and provider status. For Ollama, keep the local service running and ensure the model is downloaded. Retry a short task.

### Model configuration required

Check that the profile is saved and selected, and that cloud processing is enabled when needed. Use the configuration button in the reminder.

### Import still looks wrong

Try FDX or Fountain from the same source if available. Keep the original and note the affected page, speaker and paragraph. A small reproducible sample is much easier to fix than “the whole thing looks weird”.

### I only want to write without AI

You can. Ordinary writing, manual board summaries, saving and exporting do not require a model. Connect one later if you want to.

## Connect an external agent

CLI and MCP let other agents operate Screenplay Studio. A client needs support for local commands or local MCP.

Open Connect external Agent in More, check the bundled tools and follow the configuration instructions. Put MCP configuration in the client settings. Pasting it into a conversation does not activate a server. The bundled connection currently works locally on the same Mac.

Start with a read-only request to confirm the project name and path, then request edits. The built-in assistant and an external client have separate model configuration; accounts and keys are not automatically shared.

## Contact the creator

Found a bug or a useful idea? Email the creator. Include the app version, steps, error screenshot and a sample you are allowed to share. For confidential scripts, start with a small anonymized example.

Do not send API keys, passwords or verification codes. Hide the key before sending a model-settings screenshot.

Email huangzongjing.ziliu@gmail.com
