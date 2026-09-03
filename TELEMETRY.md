# Crash reports and usage counts

Shorthand can send crash reports and a small number of usage counts to
[Sentry](https://sentry.io), so that problems get found and fixed. It is off
until you say otherwise: the first-run setup asks, with the switch pre-set to
on, and the answer can be changed at any time under **Settings → App → Send
crash reports and usage counts**. Installs that predate this switch stay off
unless you turn it on.

This file is the source of truth for what is sent. The code that sends it is
one module, `src-tauri/src/shorthand/telemetry.rs`, so the two can be checked
against each other.

## What is sent

**Crash and error reports**

- A Rust panic: the panic message and stack frames.
- One of three named failures, with a short kind and, where the text cannot
  contain a path, the engine's error message:
  `model_load` (kind only), `transcription`, `follow_stream_listen` (the I/O
  error kind only).
- With every report: Shorthand version, operating system name and version,
  CPU architecture, Rust version, and the time.

**Usage counts**

- `capture.completed`: one count per finished capture, with the mode
  (meeting, dictation, assisted notes), the transcription model's catalogue
  id (or `custom` for a model you added yourself), and whether it succeeded.
- `capture.duration_seconds`: how long the capture ran, with the mode.
- Sessions: that the app started and ended, and whether it crashed. This is
  what gives an active-install count and a crash-free rate per version.

**Identity**

- A random id generated when you turn the switch on. It links sessions from
  the same install and nothing else. Turning the switch off deletes it;
  turning it on again generates a new one.

## What is never sent

Audio, transcripts, notes, file names or paths, API keys, your computer's
name, your name, email address or IP address. IP addresses are additionally
not stored on the receiving side (Sentry's "prevent storing of IP addresses"
is on for the organisation).

There are no log breadcrumbs: Shorthand's logs can contain transcript text,
so they are deliberately kept out.

## How the switch works

Nothing is sent while the switch is off. The gate is at the network layer,
so events, sessions and usage counts alike are dropped rather than queued.
Development builds never report at all.
