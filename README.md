# Shorthand

Shorthand turns speech into text on your computer. It can transcribe your microphone and the audio from a meeting, label them separately, and share the live transcript with tools such as the [Shorthand Obsidian plugin](https://github.com/mshish/shorthand-obsidian-plugin).

Your audio and transcription stay on your machine.

Shorthand is a fork of [Handy](https://github.com/cjpais/Handy), so it includes Handy's local dictation and transcription features.

## Install

Published Windows and Linux builds appear on the [Releases page](../../releases). macOS is not in the release build yet, so you need to build it from source.

## What you can do

- Dictate into any app with a keyboard shortcut.
- Capture your microphone and meeting audio as separate speakers.
- Choose a local transcription model that fits your computer.
- Stream a live transcript to another program with [`--follow-stream`](FOLLOW_STREAM.md).
- Use assisted notes to speak freely while another program maintains the note.

Shorthand runs on Windows, macOS, and Linux. See [HANDY.md](HANDY.md) for model choices, permissions, and troubleshooting that still apply from Handy.

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
