# Screenplay Studio User Guide

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

## Save, import and export

Keep the .screenplay project for future editing. Export PDF or Word when you need to deliver a script. Your next round of revisions will thank you for keeping the project.

Saved projects default to autosaving every 30 seconds and creating an automatic version every five minutes. Choose a project location for your first save. Check the status bar and resolve any save error.

Prefer FDX or Fountain when available because they carry screenplay structure. PDF and image imports require text and layout recognition; poor scans, unusual fonts and headers can cause errors. Check speakers, dialogue, parentheticals and scenes after import. Word imports preserve common breaks, indentation and some screenplay paragraph styles, but still need review.

### Save or make a copy

⌘S saves the project. Save as in More lets you choose another file or location. A failed save is reported as a failure.

### Go back to an earlier draft

Undo and redo handle recent edits. Open Version history in More to inspect older versions.

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

Get three things from one provider. The API Key authorizes requests; Base URL is the API endpoint supplied by the provider; Model ID is the exact model identifier. Keep all three from the same service.

Display name can be anything useful, such as “Dialogue helper”. Model ID must match the provider, including case, slashes and suffixes. A chat-site password is not an API Key.

For English-speaking users, the following international services offer English-language setup guides. Check availability in your country and current pricing before signing up. Local Ollama is also an option if you prefer downloaded models.

### 1. Open model settings

Go to More, Settings, Models & skills, Manage custom models, then Add custom model.

### 2. Fill the fields

Pick a display name. Usually choose OpenAI-compatible service. Copy Model ID, Base URL and API Key from the same provider. OpenAI omits the URL field; Local Ollama omits URL and key.

### 3. Check the URL

Use the documented Base URL, not the chat website. Do not normally append /chat/completions because the app adds it. Remove quotation marks, extra spaces and line breaks copied from examples.

### 4. Test the connection

Click Test connection. A successful API response confirms that this request worked. The test sends one short message, not your screenplay, and may still incur a provider charge. It does not measure writing quality.

### 5. Save and allow cloud processing

Click Save & use. For a cloud model, enable Allow cloud models to process screenplay text. Local Ollama does not need this switch.

### 6. Try a short request

Open the writing assistant, check the selected model at the bottom and send a small task before trying a long rewrite.

### 7. Keep more than one model

Add other profiles and switch with the model selector. Keys are remembered on this computer; leaving the key blank while editing keeps it. Configure models again on another computer because project files do not carry keys.

### OpenAI

OpenAI is the developer platform for GPT models. Create an API key, check API billing, choose OpenAI in the app and enter a supported model ID. No Base URL field is needed. A ChatGPT subscription does not include API credit.

[Official website](https://platform.openai.com) · [Official guide](https://help.openai.com/en/articles/9039756-managing-billing-for-chatgpt-and-the-api-platform)

### OpenRouter

OpenRouter brings models from multiple providers into one API, useful for comparing models without setting up a separate account for each one. Create an API key in Settings → Keys. Choose OpenAI-compatible service, use https://openrouter.ai/api/v1, and copy the exact model ID from its model page. Check the selected model’s pricing and data policy.

[Official website](https://openrouter.ai/settings/keys) · [Official guide](https://openrouter.ai/docs/quickstart)

### Groq

Groq hosts supported models through its developer API. Create a key in the console and choose a text-chat model listed in the documentation. In the app, choose OpenAI-compatible service and use https://api.groq.com/openai/v1. Copy the exact model ID and check current usage limits and pricing.

[Official website](https://console.groq.com/keys) · [Official guide](https://console.groq.com/docs/quickstart)

### Local Ollama

Install and run Ollama, then download a local text-chat model using its official instructions. Choose Local Ollama in the app and enter the installed model name including its tag. No API Key is required. Choose a downloaded local model rather than a cloud-backed one. Speed and memory needs depend on your hardware and model.

[Official website](https://ollama.com/download) · [Official guide](https://docs.ollama.com/api/openai-compatibility)

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
