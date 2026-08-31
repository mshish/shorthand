# Shorthand

Shorthand turns speech into text on your computer. It can transcribe your microphone and the audio from a meeting, label them separately, and share the live transcript with tools such as the [Shorthand Obsidian plugin](https://github.com/mshish/shorthand-obsidian-plugin).

Transcription runs on your machine, so your audio never leaves it. Turning a transcript into a written note is a separate step that uses an AI assistant. Shorthand drives the Claude Code or Codex CLI you are already signed in to, so note taking comes out of a subscription you already pay for rather than a per-token API bill. See [AI note taking](#ai-note-taking).

Shorthand is a fork of [Handy](https://github.com/cjpais/Handy), so it includes Handy's local dictation and transcription features.

## Install

Published Windows and Linux builds appear on the [Releases page](../../releases).

Only Windows has been tested so far. The Linux builds are published but untried. macOS is not in the release build at all, because there is no Apple signing certificate yet, so macOS users need to [build from source](#macos) for now. Shorthand needs macOS 14.6 or later.

Installers are unsigned. Windows SmartScreen warns the first time; choose More info, then Run anyway.

## What you can do

- Dictate into any app with a keyboard shortcut.
- Capture your microphone and meeting audio as separate speakers.
- Choose a local transcription model that fits your computer.
- Stream a live transcript to another program with [`--follow-stream`](FOLLOW_STREAM.md).
- Use assisted notes to speak freely while another program maintains the note.

Shorthand builds and runs on Windows, macOS, and Linux; [Install](#install) says what is tested today. See [HANDY.md](HANDY.md) for model choices, permissions, and troubleshooting that still apply from Handy.

## AI note taking

Dictation and transcription need nothing else installed. Note taking needs an AI assistant. Assisted notes and meeting notes stream the live transcript to a follower, either the Obsidian plugin or [`shorthand-core`](https://github.com/mshish/shorthand-core), and the follower asks that assistant to keep the written note up to date as you talk. The assistant is a command-line tool you install and sign in to yourself.

A paid Claude or ChatGPT subscription already includes one:

- A Claude subscription (Pro or Max) includes Claude Code.
- A ChatGPT subscription (Plus or Pro) includes Codex.

Shorthand uses whichever one you are signed in to, so your notes are covered by the subscription you already pay for. There is no API key to create and no per-token bill.

Set this up before your first capture:

1. Install [Claude Code](https://docs.claude.com/en/docs/claude-code/setup) or [Codex](https://github.com/openai/codex).
2. Run `claude` (or `codex`) once in a terminal and finish signing in.
3. Then start assisted notes or meeting capture in Shorthand.

Claude Code is the default. Core takes `--backend codex` for Codex, and `--backend llm` for an API-key provider (OpenAI, Anthropic, Ollama, or another OpenAI-compatible endpoint) if you would rather pay per token.

Shorthand's own AI cleanup, under Modes then Advanced, is a separate feature that calls an API provider with a key you supply. It is not part of this path.

Without a signed-in assistant, recording and transcription still work, but no note is written.

### What is kept

Meeting mode saves no recording and no transcript by default, because a meeting can include people who did not choose to be recorded. Assisted notes captures only your own microphone, so it keeps both; either can be turned off under Modes.

The assistant's session history is deleted when the capture ends. Core uses a resumable Claude or Codex session as memory for one capture and then disposes of it: the Claude backend deletes the session through the Agent SDK, and the Codex backend throws away the temporary home it ran in. Keeping that history is an advanced opt-in, off by default. This covers the copy on your machine, not the provider's own usage and billing records.

## Obsidian notes

The [Shorthand Obsidian plugin](https://github.com/mshish/shorthand-obsidian-plugin) uses the live transcript to update a meeting note while you talk. Install the plugin separately and keep Shorthand running during capture.

## Build from source

Install [Rust](https://rustup.rs/), [Bun](https://bun.sh/), and the [Tauri prerequisites](https://tauri.app/start/prerequisites/) for your operating system. [BUILD.md](BUILD.md) lists the required platform packages and known build issues.

Then install the dependencies and download the voice-activity model:

```sh
bun install
mkdir -p src-tauri/resources/models
curl --output src-tauri/resources/models/silero_vad_v4.onnx https://blob.handy.computer/silero_vad_v4.onnx
```

Run the app in development:

```sh
bun run tauri dev
```

Build an installer or application bundle:

```sh
bun run tauri build
```

### macOS

There is no published macOS build yet, so this is the only way to run Shorthand on a Mac, and none of it needs an Apple developer account. `bun run tauri build` produces an unsigned `.dmg` and an ad-hoc signed `.app`, which is enough to run on the machine that built it.

Leave `TAURI_SIGNING_PRIVATE_KEY` unset. It exists for update signing in CI, and a value that is present but empty fails the build rather than skipping the step.

A build copied to a different Mac arrives quarantined. Clear it once:

```sh
xattr -dr com.apple.quarantine /Applications/Shorthand.app
```

Intel Macs need ONNX Runtime from Homebrew and two environment variables; [BUILD.md](BUILD.md#macos) has the exact command.

For just the binary, without bundling an installer:

```sh
bun run tauri build --no-bundle
```

Before opening a pull request, run the relevant checks:

```sh
bun run lint
bun run format:check
bun run test:unit
bun run check:translations
bun run check:branding
```

## Command line

Run `shorthand --help` to see every option. Common controls include:

| Flag                      | What it does                                              |
| ------------------------- | --------------------------------------------------------- |
| `--toggle-transcription`  | Start or stop recording                                   |
| `--toggle-assisted-notes` | Start or stop assisted notes                              |
| `--cancel`                | Cancel the current recording                              |
| `--follow-stream [MODE]`  | Read live transcript events as `json`, `delta`, or `text` |
| `--start-hidden`          | Start in the system tray                                  |
| `--no-tray`               | Run without a tray icon                                   |
| `--debug`                 | Turn on verbose logging                                   |

The full live transcript protocol is documented in [FOLLOW_STREAM.md](FOLLOW_STREAM.md).

## Contributing

Open Shorthand changes against `main`. See [AGENTS.md](AGENTS.md) for the fork's branch and architecture guidance. [CONTRIBUTING.md](CONTRIBUTING.md) describes the separate process for changes sent to Handy.

## License

The software is MIT licensed. See [LICENSE](LICENSE). Shorthand includes work from Handy by CJ Pais and its contributors.

The Shorthand name and visual identity are not licensed under MIT. See [TRADEMARKS.md](TRADEMARKS.md) and [BRAND_ASSETS_LICENSE.md](BRAND_ASSETS_LICENSE.md).
