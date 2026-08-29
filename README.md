# Shorthand

Local speech-to-text that captures both sides of a meeting and streams the transcript to whatever is listening.

Shorthand transcribes your microphone and the audio playing through your speakers as two separate speaker-labelled lanes, entirely on your machine. Another program can read that transcript while you are still talking — which is how the [Obsidian plugin](https://github.com/mshish/shorthand-obsidian-plugin) keeps a meeting note current during the meeting.

It is a fork of [Handy](https://github.com/cjpais/Handy). Everything Handy does, this does.

## Status

**There are no installers yet.** You build it from source. Releases, signing and in-app updates are being worked on; until they land, the update prompt inside the app is not yours and should be declined.

## What Shorthand adds

- **System audio capture.** What the other side of a call says is transcribed too, in its own speaker-labelled lane alongside your microphone. Off by default; turn it on under Advanced.
- **[`--follow-stream`](FOLLOW_STREAM.md).** Another program reads transcript events over a local socket while the recording runs. This is the interface the Obsidian plugin consumes.
- **Capture modes.** _Meetings_ for two-sided conversation, _Notetaking_ for working alone — dictation for typing with your voice, assisted notes for narrating and letting something else keep the note.
- **A different look.** See [BRANDING.md](BRANDING.md).

## How it works

Press a shortcut, talk, press it again. Everything stays on your computer.

Silence is filtered out with voice activity detection before anything is transcribed. Transcription runs on a model you choose — Whisper (Small, Medium, Turbo, Large) with GPU acceleration where available, or Parakeet V3, which is CPU-friendly and detects the language itself.

Windows, macOS and Linux.

## The Obsidian plugin

[`shorthand-obsidian-plugin`](https://github.com/mshish/shorthand-obsidian-plugin) follows a live capture and keeps a meeting note updated as you talk, with a transcript note beside it. Install it from its own repository — it needs this application running to do anything.

## Building from source

You need [Rust](https://rustup.rs/) and [Bun](https://bun.sh/).

```bash
bun install
bun run tauri dev
```

Before the first run, fetch the voice-activity model:

```bash
mkdir -p src-tauri/resources/models
curl -o src-tauri/resources/models/silero_vad_v4.onnx https://blob.handy.computer/silero_vad_v4.onnx
```

[BUILD.md](BUILD.md) has the platform-specific requirements, which are not optional on Linux.

## Command line

| Flag                      | What it does                                           |
| ------------------------- | ------------------------------------------------------ |
| `--toggle-transcription`  | Start or stop recording on a running instance          |
| `--toggle-post-process`   | Start or stop recording with post-processing           |
| `--toggle-assisted-notes` | Start or stop an assisted-notes capture                |
| `--cancel`                | Cancel whatever is running                             |
| `--follow-stream [MODE]`  | Read live transcript events: `json`, `delta` or `text` |
| `--start-hidden`          | Launch to the tray without showing the window          |
| `--no-tray`               | Launch with no tray icon, so closing the window quits  |
| `--debug`                 | Verbose logging                                        |

`--help` lists the rest, including the file-transcription and device-listing flags.

The flags above control a running Shorthand. `--follow-stream` instead reads from one: you get transcript events until you disconnect, and Shorthand carries on regardless. **Follow live transcript output** under Advanced controls it — on by default for Meetings, off for Dictation. [FOLLOW_STREAM.md](FOLLOW_STREAM.md) has the protocol.

## Platform notes and troubleshooting

Linux setup, Bluetooth microphone behaviour on macOS, model installation behind a proxy, custom Whisper models and the known issues are covered in [HANDY.md](HANDY.md) — upstream's README, kept because that material is still accurate here and still maintained there.

Its sections on releases, sponsors, roadmap and signing keys describe Handy, not Shorthand.

## About the fork

Shorthand tracks [cjpais/Handy](https://github.com/cjpais/Handy) and merges from it regularly, so upstream's fixes and models arrive here. Changes with nothing fork-specific about them can go back the other way.

Some inherited code, comments and documentation still say "Handy" where the rename has not reached. That is deliberate: renaming lines upstream never renamed only makes future merges harder.

[AGENTS.md](AGENTS.md) covers the branch layout, how the fork stays mergeable, and where fork-only work belongs.

## Contributing

Fork it, branch off `main`, test on your platform, open a pull request. Nothing else is required — GitHub pre-fills the description with Handy's template, and you can replace it, because that checklist is for pull requests aimed at Handy.

Sending a change to Handy instead is a different process with real requirements. [AGENTS.md](AGENTS.md#github-workflow-for-ai-coding-assistants) has them.

## License

MIT, inherited from Handy and unchanged. Copyright (c) 2025 CJ Pais — see [LICENSE](LICENSE).

Shorthand exists because Handy is worth forking. The transcription pipeline, the model handling and most of the application are CJ Pais's work and the work of Handy's contributors.
