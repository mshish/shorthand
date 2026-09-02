# Shorthand

Shorthand is AI-assisted note taking for meetings and spoken thinking. It listens while you talk, transcribes the audio on your computer, and keeps a structured Obsidian note up to date throughout the conversation.

- **Meeting notes:** Capture your microphone and meeting audio as separate speakers, then turn both sides of the conversation into one useful note.
- **Assisted notes:** Talk through an idea, plan, or rough draft and let Shorthand organize it as you go.
- **Your notes stay yours:** The plugin updates only its marked section. Anything you write outside that section stays untouched.

The desktop app also includes all of [Handy](https://github.com/cjpais/Handy)'s local transcription and dictation features, including keyboard-shortcut dictation, local model choices, history, and audio controls.

## Use your existing AI subscription

Shorthand can write notes with:

- **Claude Code**, included with Claude Pro and Max subscriptions.
- **Codex**, included with ChatGPT plans, including Plus and Pro.
- **An LLM provider**, including OpenAI, Anthropic, Ollama, or another OpenAI-compatible endpoint.

With Claude Code or Codex, install the command-line app once and sign in. The [Shorthand Obsidian plugin](https://github.com/mshish/shorthand-obsidian-plugin) uses that login, so AI-assisted notes can use the subscription you already pay for without a separate API key or API billing account.

You can also connect an LLM provider if you prefer an API key, a local model, or your own endpoint. Recording and transcription still work without an AI connection.

[docs/AI_NOTE_TAKING.md](docs/AI_NOTE_TAKING.md) is the step-by-step setup guide the app links to.

## Quick start

This walkthrough covers Windows and desktop Obsidian, the combination tested today.

1. **Install the Shorthand desktop app.**
   - Download the Windows installer from the [latest release](../../releases/latest).
   - Run the installer and open Shorthand.
   - Follow the first-run setup to choose and download a transcription model.
   - Windows SmartScreen may warn that the installer is unsigned. Select **More info**, then **Run anyway**.

2. **Connect Obsidian.** In Shorthand, open **Notes** and choose **Install in Obsidian**. Obsidian opens on the Shorthand plugin's page; choose **Install**, then **Enable**.

3. **Choose how Shorthand writes your notes.** Open the Shorthand plugin settings in Obsidian, find **Enhancement backend**, and choose one:
   - **Claude Code (default):** Install [Claude Code](https://code.claude.com/docs/en/setup#install-claude-code). Open PowerShell or Terminal, run `claude auth login`, and sign in with your Claude account.
   - **Codex:** Install the [Codex CLI](https://learn.chatgpt.com/docs/codex/cli#getting-started). Open PowerShell or Terminal, run `codex login`, and sign in with your ChatGPT account.
   - **LLM provider:** Enter the provider, model, endpoint, and API key if one is required.

4. **Create your first note.**
   1. Keep the Shorthand desktop app running and open a Markdown note in Obsidian.
   2. Open the command palette and run **Shorthand: Start meeting capture on this note**.
   3. Talk normally. Shorthand transcribes the meeting and the plugin updates the note.
   4. Run **Shorthand: Stop capture** when you finish.

For a solo thinking session, enable **Assisted notes** under **Shorthand Settings → Modes → Notetaking**, then run **Shorthand: Start assisted notes capture on this note** in Obsidian.

## How it works

| Part                                   | What it does                                                                                                                 |
| -------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------- |
| Shorthand desktop app                  | Records your microphone and, in Meetings mode, can also capture computer audio. It transcribes both locally.                 |
| Shorthand Obsidian plugin              | Follows the live transcript and updates the note you chose. Your own writing stays outside the section managed by Shorthand. |
| Claude Code, Codex, or an LLM endpoint | Turns the transcript and your existing note into a structured note.                                                          |

The desktop app and Obsidian plugin are separate installs. Keep Shorthand running during a capture; the plugin cannot record or transcribe audio by itself.

Audio stays on your computer for transcription. The note-writing AI receives the transcript and current note, not the recording.

## What Shorthand keeps

- **Meetings:** Recordings and transcripts are not saved by default.
- **Assisted notes:** Recordings and transcripts are saved by default; you can turn either off under **Modes**.
- **AI sessions:** Local Claude Code and Codex session history is deleted when the capture ends unless you enable the advanced history setting.
- **Obsidian notes:** The plugin edits only the marked Shorthand section. The rest of the note remains yours.

These settings cover files on your computer. Your selected AI provider may keep its own usage and billing records.

## Platform status

Published Windows, Linux, and macOS builds are available on the [Releases page](../../releases).

- **Windows:** Tested. Installers are unsigned, so SmartScreen warns on first install.
- **Linux:** Builds are published but have not been tested yet.
- **macOS:** Builds are published but have not been tested yet. They are also unsigned, because Shorthand does not have an Apple signing certificate, so macOS quarantines the downloaded app. Clear it once with `xattr -dr com.apple.quarantine /Applications/Shorthand.app`, or open the app from **Privacy & Security** in System Settings. macOS 14.6 or later is required. You can also [build from source](BUILD.md#macos).

Reports and pull requests for the untested platforms are welcome.

For transcription models, permissions, platform notes, and troubleshooting inherited from Handy, see [HANDY.md](HANDY.md).

## Development

Install [Rust](https://rustup.rs/), [Bun](https://bun.sh/), and the [Tauri prerequisites](https://tauri.app/start/prerequisites/) for your operating system. [BUILD.md](BUILD.md) lists the platform packages and known build issues.

Install dependencies and download the voice-activity model:

```sh
bun install
mkdir -p src-tauri/resources/models
curl --output src-tauri/resources/models/silero_vad_v4.onnx https://blob.handy.computer/silero_vad_v4.onnx
```

Run the app in development or build an installer:

```sh
bun run tauri dev
bun run tauri build
```

Use `bun run tauri build --no-bundle` when you need the release binary without an installer or application bundle.

### macOS development

Building locally produces an unsigned `.dmg` and an ad-hoc signed `.app`, which can run on the Mac that built it without an Apple developer account.

Leave `TAURI_SIGNING_PRIVATE_KEY` unset. An empty value still asks Tauri to sign the build and causes it to fail. A build copied to another Mac arrives quarantined; clear it once with:

```sh
xattr -dr com.apple.quarantine /Applications/Shorthand.app
```

Intel Macs need ONNX Runtime from Homebrew and two additional environment variables. [BUILD.md](BUILD.md#macos) has the commands.

### Checks

Before opening a pull request, run the relevant checks:

```sh
bun run lint
bun run format:check
bun run test:unit
bun run check:translations
bun run check:locale-drift
bun run check:fork-translations
bun run check:branding
```

### Command line and integrations

Run `shorthand --help` to see every option. Common controls include:

| Flag                      | What it does                                              |
| ------------------------- | --------------------------------------------------------- |
| `--toggle-transcription`  | Start or stop a meeting recording                         |
| `--start-transcription`   | Start a meeting recording; succeeds if already started    |
| `--stop-transcription`    | Stop a meeting recording; succeeds if already stopped     |
| `--toggle-assisted-notes` | Start or stop assisted notes                              |
| `--start-assisted-notes`  | Start assisted notes; succeeds if already started         |
| `--stop-assisted-notes`   | Stop assisted notes; succeeds if already stopped          |
| `--cancel`                | Cancel the current recording                              |
| `--follow-stream [MODE]`  | Read live transcript events as `json`, `delta`, or `text` |
| `--start-hidden`          | Start in the system tray                                  |
| `--no-tray`               | Run without a tray icon                                   |
| `--debug`                 | Turn on verbose logging                                   |

Use the explicit start and stop pair for the mode you want in scripts and integrations. They are safe to retry; a toggle changes state on every delivery and is better suited to interactive use, where you can see what happened.

Explicit commands report their outcome over an attached `--follow-stream` connection. [FOLLOW_STREAM.md](FOLLOW_STREAM.md) documents the live transcript protocol, and [`shorthand-core`](https://github.com/mshish/shorthand-core) provides the shared follower and note-writing logic used by integrations.

### Contributing

Open Shorthand changes against `main`. [AGENTS.md](AGENTS.md) covers the fork's branch and architecture guidance. [CONTRIBUTING.md](CONTRIBUTING.md) describes the separate process for changes sent to Handy.

## License

The software is MIT licensed. See [LICENSE](LICENSE). Shorthand includes work from Handy by CJ Pais and its contributors.

The Shorthand name and visual identity are not licensed under MIT. See [TRADEMARKS.md](TRADEMARKS.md) and [BRAND_ASSETS_LICENSE.md](BRAND_ASSETS_LICENSE.md).
