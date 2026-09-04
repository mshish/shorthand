# Shorthand: crash reports and usage counts

Status: approved
Date: 2026-09-03

## Context

Shorthand has no error reporting and no usage data. Every bug report is a
user's description of what happened, and there is no way to tell how many
installs are active or which of the three modes people actually use.

A Sentry organisation (`shorthand-f4`) now exists with errors and metrics
enabled. This spec adds crash and error reporting plus a deliberately small set
of usage counts to the desktop app, behind a single consent toggle that is
offered on first run and defaults to on, and that existing installs never see
turned on without asking.

The Obsidian plugin is excluded. Obsidian's
[Developer policies](https://docs.obsidian.md/Developer+policies) list
"Include client-side telemetry" under *Not allowed*, and staff have said on the
[forum](https://forum.obsidian.md/t/telemetry-only-for-beta-version/89069) that
error-reporting services count and that BRAT and manual installs are expected to
follow the same policy. The plugin README's "does not collect telemetry" stays
true. `shorthand-core` runs inside Obsidian's process for the plugin, so it is
excluded for the same reason.

## Constraints

- **Mergeability.** The fork merges from upstream Handy indefinitely. Prefer
  new files; keep edits to upstream files to single lines at named touch
  points. `follow_stream/` is the model.
- **Nothing before consent.** With the toggle off, zero bytes leave the
  machine. Not a session, not a metric, not a buffered event.
- **Existing installs stay off.** A store that predates this feature has no
  `telemetry_enabled` key and must load as `None`, which is treated as off.
  Only the first-run consent step, or the Settings toggle, ever writes
  `true`.
- **No user content, ever.** No audio, transcript, note, file name or path, API
  key, hostname, name, email or IP address reaches Sentry. The capture points
  are enumerated below and each one is reviewed for what its error text can
  contain.
- **Not overkill.** Two metrics plus release-health sessions. Anything more is a
  new spec.

## Decisions

### Official Rust SDK, no Tauri plugin

Sentry has no Tauri guide. The community `tauri-plugin-sentry` crate describes
itself as experimental, its npm half is a year stale and pinned to a Tauri API
beta, and its injected browser SDK forwards console breadcrumbs to the client
unfiltered, which is exactly where transcript text would leak from the history
screen. The official `sentry` crate (0.49) covers what v1 needs: panics,
explicit error capture, release-health sessions, and `sentry::metrics`.

Webview error capture and native minidumps are a possible v2, decided on
evidence from v1.

Cargo features, chosen explicitly rather than taking the defaults:
`backtrace`, `contexts`, `panic`, `release-health`, `metrics`, `transport`.
Not `logs`, and not the `log` crate integration: log lines are the one place
transcript text does appear in this codebase, and they must not become
breadcrumbs. Not `debug-images` either: it attaches every loaded module's
absolute path to every event, which on a per-user Windows install contains
the account name.

### One toggle, not two

An "errors only" mode would still need sessions to compute a crash-free rate,
so the split would be cosmetic while adding a second setting, gate, string set
and test path. Clarity comes from the consent copy naming exactly what "usage"
means and what is never sent.

### The gate is the transport

One `AtomicBool` consent flag, read by a transport wrapper around Sentry's
default transport factory. When the flag is false, `send_envelope` drops the
envelope. That single chokepoint covers events, sessions and metrics alike, so
there is no second gate to forget. The client is created at process start
with the flag closed; `setup()` opens it from the stored setting once the
settings store is available; the Settings toggle flips it live. No early
settings-file read, no restart.

Session hygiene around the flag: opening it calls `sentry::start_session()` so
the session Sentry sees begins at consent. Closing it calls
`sentry::end_session()`, which only enqueues the session-exit envelope, then
flushes the client explicitly (a bounded wait, not a fire-and-forget) while
consent still holds, and only then closes the gate — so the session exit is
sent before the gate shuts, rather than left for the periodic flusher to send
(or drop) up to 60 seconds later, possibly after opt-out.

### Setting and first-run behaviour

Two new `AppSettings` fields, both with serde defaults:

| Field                   | Type             | Default | Meaning                                        |
| ----------------------- | ---------------- | ------- | ---------------------------------------------- |
| `telemetry_enabled`     | `Option<bool>`   | `None`  | `None` = never asked; the consent step and the toggle write `Some`. |
| `telemetry_install_id`  | `Option<String>` | `None`  | Random UUID; set on opt-in, cleared on opt-out. |

`get_default_settings()` also returns `None`/`None`, so a fresh install sends
nothing until the consent step is confirmed. The consent step's toggle is
pre-set to on; pressing Continue writes the explicit value. That is what
"default on for new installs" means here: the default is the toggle's
position, not the stored value.

No migration touches existing stores. A missing key is `None`, which is
treated as off, and the frozen-store test in `settings.rs` pins it.

The install id is what lets release health count distinct installs rather
than sessions. It is attached as the Sentry `user.id` and nothing else. It is
generated when consent turns on and there is none, and cleared when consent
turns off, so opting out and back in produces an unlinked identity.

### Consent step

A new onboarding step `telemetry`, shown after model selection, before the
main app, new users only (`isReturningUser === false`). Returning users who
lack permissions re-enter at `accessibility` and go straight to `done` as
today. `select_model` already stamps `onboarding_completed` before this step
runs, so quitting during the consent screen leaves telemetry unanswered
(`None`, treated as off) — the safe direction.

One screen: the wordmark, a title, a two-sentence intro, a "What is sent" list
of two items, a "What is never sent" line, one toggle, a "See exactly what is
sent" link to `TELEMETRY.md` on GitHub, and Continue. Continue awaits
`updateSetting("telemetry_enabled", value)` and then advances. The toggle is
always pre-set to on — a first-run question, not a reflection of whatever the
store holds — and Continue writes the chosen value.

Copy, English only, in `src/shorthand/locales/en.json` under
`onboarding.telemetry.*`:

- title: "Help improve Shorthand"
- intro: "Shorthand can send crash reports and a little usage info so problems
  get found and fixed. It is anonymous, and you can change this any time under
  Settings → App."
- sends.heading: "What is sent"
- sends.errors: "Crash and error reports: what failed and where in the app,
  with your operating system and Shorthand version."
- sends.usage: "Usage info: how many captures finish, in which mode, with
  which model, and how long they ran."
- never.heading: "What is never sent"
- never.body: "Audio, transcripts, notes, file names or paths, API keys, your
  name, email address or IP address."
- toggle: "Send crash reports and usage info"
- link: "See exactly what is sent"
- continue: "Continue"

### Settings toggle

`TelemetryToggle` in `src/shorthand/telemetry/`, rendered in the fork's App
section directly after `AutostartToggle`, outside the Advanced-only rows, so
it is reachable without opening Advanced settings. Label "Send crash reports
and usage info"; description "Anonymous crash reports and a little usage
info. Never audio, transcripts, notes or personal details." Keys under
`settings.app.telemetry.*`. The settings-coverage check picks it up because
`src/shorthand/settings` is an entry point and the component lives under
`src/shorthand`, which is in `SETTINGS_COMPONENT_DIRS`.

### What is reported

**Errors**

- Rust panics, via the SDK's panic integration. Panic messages in this codebase
  are static `expect` strings; a panic that formats user content is a bug to fix
  at the source, not to scrub.
- Three explicit capture points, each sending a fixed `kind` tag and, only
  where the text cannot carry a path or user content, the error's Display
  text. The reviewer of each point confirms that before the detail is passed.
  1. Model load failure (`managers/transcription.rs`): kind only, since load
     errors name the model file on disk.
  2. Transcription failure at capture stop (`actions.rs`, the `Err` arm of
     `transcription_result`): a fixed reason code, never the message.
  3. Follow-stream listener failure (`follow_stream/server.rs`): the I/O error
     kind only, since the message can name the per-user socket path.

**Metrics**, emitted from Rust at the point a capture's transcription result is
known:

| Metric                     | Type         | Attributes                       |
| -------------------------- | ------------ | -------------------------------- |
| `capture.completed`        | counter      | `mode`, `model`, `outcome`       |
| `capture.duration_seconds` | distribution | `mode`                           |

`mode` is `meeting`, `dictation` or `assisted_notes` from the active-mode
cell. `model` is the catalogue id, or the literal `custom` when
`ModelInfo::is_custom` is true, so user-named models never appear. `outcome` is
`ok` or `error`. Duration is recording start to transcription result.

**Sessions**, started and ended explicitly by `set_consent` (application
session mode), which gives active installs, crash-free rate, and both per
release and per OS, at no further instrumentation.

**Never**: `send_default_pii` stays false. `server_name` is set to the
constant placeholder `"shorthand"` rather than left `None`, because
`ContextIntegration::setup` fills a `None` `server_name` from the machine
hostname; the placeholder keeps that from ever happening. No breadcrumbs from
logs. No request or environment context beyond what the `contexts`
integration adds (OS name and version, device architecture, Rust version). No
loaded-module paths: the `debug-images` feature that would attach them is
off.

Server-side, the organisation's "Prevent Storing of IP Addresses" setting is
turned on, so the "no IP address" claim holds for the ingest path too, not just
for what the SDK puts in the payload.

### DSN, release and environment

The DSN is a compile-time constant in the telemetry module. A DSN is public by
design, and the alternative of an environment variable in CI would make
builds from source silently telemetry-free for reasons nobody documented.

`release` is `shorthand@<CARGO_PKG_VERSION>`, matching what the updater reports.
`environment` is `production`. Under `debug_assertions` the client is not
created at all, so `bun run tauri dev` never reports; the transport gate is
therefore a second line of defence in debug builds, not the first.

Release builds strip symbols, so panics arrive with frames but without line
numbers. Debug images are off because they carry every loaded module's
absolute path, which on a per-user Windows install contains the account name.
Debug-file upload is a follow-up if that turns out to hurt; any future
version must strip paths from what it uploads, since re-enabling the feature
as-is would reopen this finding.

### Module layout and touch points

New, fork-only:

- `src-tauri/src/shorthand/telemetry.rs`: the consent flag, the gated
  transport, `init()`, `set_consent()`, `capture_completed()`,
  `report_error()`, and the `change_telemetry_enabled_setting` command.
- `src/shorthand/telemetry/TelemetryOnboarding.tsx`, `TelemetryToggle.tsx`.
- `TELEMETRY.md`: the "what is sent" source of truth, linked from the consent
  screen and the README.
- `tests/telemetry-onboarding.spec.ts`: the first fork-only Playwright spec
  that stubs `window.__TAURI_INTERNALS__`, as `docs/FRONTEND_TESTING.md`
  prescribes.

Edits to upstream files, each one line or one block:

- `src-tauri/Cargo.toml`: the `sentry` dependency and `uuid` if not already
  transitive.
- `src-tauri/src/shorthand/mod.rs`: `pub mod telemetry;`.
- `src-tauri/src/lib.rs`: `init()` after `portable::init()`; `set_consent()` in
  `setup()` after settings load; the command in `collect_commands!`.
- `src-tauri/src/settings.rs`: two fields, two default fns, two lines in
  `get_default_settings()`.
- `src-tauri/src/actions.rs`: one call at capture start, one at the
  transcription result.
- Three error capture sites: one line each in `actions.rs` and
  `follow_stream/server.rs`; a thin wrapper around `load_model_with_device`
  in `managers/transcription.rs`.
- `src/App.tsx`: the `telemetry` step in the union and the two transitions.
- `src/stores/settingsStore.ts`: one line mapping `telemetry_enabled` to its
  command.
- `src/shorthand/settings/AppSettings.tsx`: one component in the first group.
- `README.md`: a Privacy section. `CLAUDE.md`: a line pointing at
  `TELEMETRY.md`.

### Testing

- Rust unit tests in `telemetry.rs`: the gated transport drops envelopes while
  closed and forwards them while open, using a recording fake as the inner
  transport; `set_consent(true)` generates an install id once and
  `set_consent(false)` clears it; the model attribute maps a custom model to
  `custom`.
- `settings.rs`: defaults are `None`/`None`; the frozen v0.9 store loads with
  `telemetry_enabled == None`.
- Playwright: a fresh profile (`onboarding_completed: false`) reaches the
  consent step after permissions, the toggle is on, and Continue invokes the
  settings command with `true`; toggling off first invokes it with `false`.
- Existing gates: `check:settings`, `check:fork-translations`,
  `check:locale-drift`, `cargo clippy`, `cargo test`.

### Out of scope

Webview error capture, native minidumps, debug-file upload, alert rules, a
diagnostics command in the Obsidian plugin, and any prompt to existing
installs. Each is a separate decision with its own spec if wanted.
