# Dictation mode — handoff

Written 2026-08-20, at the point where all 11 planned tasks are implemented and
committed but the branch has not been reviewed as a whole, exercised in a
running app, or merged.

- **Branch:** `shorthand-dictation-mode`, cut from `shorthand` at `31c4360`
- **Feature commits:** `b62a3f8` … `16f1d38` (12 commits)
- **Spec:** [`../specs/2026-08-20-shorthand-dictation-mode-design.md`](../specs/2026-08-20-shorthand-dictation-mode-design.md)
- **Plan:** [`2026-08-20-dictation-mode.md`](2026-08-20-dictation-mode.md)
- **Execution ledger:** `.superpowers/sdd/2026-08-20-dictation-mode/progress.md`
  — every ruling, deferred finding and per-task verification lives there. Read
  it before trusting anything in this file.

## State

Green as of the last run: `cargo test` 303 passed / 0 failed;
`bun run check:translations` all 24 locales complete; `bun run lint` clean;
`bun run build` succeeds.

All 11 tasks were reviewed by a separate subagent except Task 11, which was
committed by the controller after a session interruption killed its
implementer — see "Known gaps".

## What remains

### 1. Final whole-branch review

Not yet run. It should cover the full range `31c4360..16f1d38` and pay
particular attention to Task 11's commit (`16f1d38`), which never got an
implementer self-review pass.

Point it at the ledger's `minor (deferred)` lines so it can triage which, if
any, must be fixed before merge. Those are:

- `mode.rs` uses `Ordering::Release`/`Acquire` where the house-style examples
  (`OVERLAY_ENABLED`, `WINDOWS_OVERLAY_IS_STREAMING`) use `Relaxed`. Correct,
  just stricter than needed.
- `DictationEnableToggle` returns a Fragment, so `SettingsGroup`'s `divide-y`
  draws a divider between the toggle and its own error text. Cosmetic.
- `as DictationSettings | undefined` casts are no-ops — `AppSettings.dictation`
  is already optional-typed in `bindings.ts`.
- `{...dictation, field}` would spread a partial object if `dictation` were
  ever `undefined`. It cannot be — `settings.rs` has `#[serde(default)]` on the
  field. Theoretical.
- The six new dictation row components lack `React.memo`, which every upstream
  settings-row component has. No correctness impact; they re-render together
  through one store subscription.

### 2. Manual QA — nothing below was verified

There is no React test harness in this repo and adding one is forbidden (it
would put devDependencies in upstream's `package.json` and `bun.lock`). So
every frontend behaviour below is unverified. Run `bun run tauri dev`.

**The feature is off by default. Confirm that first.**

1. Fresh profile, dictation off: sidebar shows Capture, Transcription, App,
   Dictation, History, Debug, About. The Dictation section's rows render
   dimmed, not hidden.
2. Dictation off: pressing `ctrl+space` (macOS `option+space`) does nothing,
   and the log shows no registration for `dictate`.
3. Enable dictation, then dictate into a text editor: **text is pasted**.
4. Trigger meeting mode's shortcut (`ctrl+alt+space`, macOS
   `ctrl+shift+space`): **nothing is pasted**, and the transcript still
   reaches the follower.
5. **The cross-talk test.** Set the two modes to *different* paste methods and
   *different* overlay styles. Confirm each mode uses its own. This is the
   scenario a shared-state bug produces, and it is the single most valuable
   manual check on this list.
6. Set dictation's overlay to None while meeting's is Live. Confirm no overlay
   flashes mid-dictation — this exercises the `overlay.rs` swap specifically.
7. Turn dictation's save-transcripts on and meeting's off. Confirm only
   dictation runs reach History, tagged "Dictation".
8. Enable dictation post-processing: the Post-Processing section appears in the
   sidebar, and the dictation AI-cleanup shortcut registers.
9. Bind `ctrl+space` to something else first, *then* enable dictation. The
   toggle should fail to stick and show the shortcut-conflict message. This
   verifies the error-propagation chain fixed in `8c9ff1c` — it was dead code
   until then, and it is the one path with no automated coverage.
10. Restart the app. Every dictation setting persisted. This is what catches a
    missing `settingUpdaters` entry.
11. macOS only: revoke Accessibility permission. The Dictation section reports
    it and offers Grant.
12. Toggle `show_all_settings` in About: Dictation disappears with the other
    fork sections; toggle back, it returns.
13. Linux only: confirm the typing-tool row appears only when paste method is
    Direct.

### 3. Merge decision

Not started, and deliberately left for a human — merging into `shorthand` is a
shared-branch action. `superpowers:finishing-a-development-branch` covers the
options.

## Known gaps and environment notes

- **Task 11 has no implementer self-review.** Its implementer was killed by a
  session limit after editing all 24 locale files but before committing. The
  controller verified the work directly (all 20 keys present in all 24 locales
  by scripted check; the `transcribe.name` rename in exactly 24 files by diff
  count; all four suites green) and committed it. It is the one commit on this
  branch that skipped a step.
- **`cargo clippy --all-targets -- -D warnings` fails on this branch, and did
  before this work started.** Pre-existing violations in `lib.rs:456`,
  `paste_tx/windows.rs`, `audio_toolkit/audio/recorder.rs`,
  `managers/transcription.rs`, `managers/gguf_meta.rs`, `portable.rs`,
  `secure_input.rs`. Verified by `git stash` before/after. The working bar for
  this plan was "introduces no new findings", checked per task by grepping
  clippy output for our own file paths. Fixing the baseline would mean editing
  seven upstream files — pure merge-conflict surface, deliberately out of scope.
- **`bun run format:check` also fails on the clean tree** — 79 lines,
  byte-identical before and after this work. A Windows CRLF/Prettier
  environment issue affecting the whole repo, not caused by this branch.
- **Pre-existing i18n bug, unrelated to this work, worth its own fix:**
  `settings.privacy.saveRecordings.label`, `settings.privacy.saveTranscripts.label`
  and `settings.privacy.title` are referenced by upstream's `SaveRecordings.tsx`
  and `SaveTranscripts.tsx` and by the fork's `CaptureSettings.tsx`, but do not
  exist in `en/translation.json`. `check:translations` does not catch it because
  it only verifies key parity between locales, not that referenced keys resolve.
  Dictation sidesteps it with its own `settings.dictation.privacy.*` keys.
- **`src-tauri/Cargo.toml` had an uncommitted modification predating this
  plan**, and `.serena/` is untracked. Neither belongs to this work; both were
  deliberately excluded from every commit.

## Design decisions most likely to be questioned later

Recorded here because the reasoning is not obvious from the diff.

- **The active-mode cell is a process-global `AtomicBool`, not Tauri managed
  state.** It matches house style (`OVERLAY_ENABLED` and
  `WINDOWS_OVERLAY_IS_STREAMING` in `overlay.rs`, `WEBVIEW_LOG_STREAMING` in
  `lib.rs`); `app.manage()` is reserved for manager objects here. It is sound
  because only one capture runs at a time.
- **It is set on every capture start and never cleared.** "The mode of the most
  recently started capture" is always right for work belonging to that capture,
  including async work outliving the recording. Clearing it would add a race.
- **`apply_mode` returns a full `AppSettings`** rather than a narrow delivery
  struct. That is what kept `clipboard.rs` to a 4-line diff with `paste()`'s
  body untouched. Narrowing it would push real edits into upstream functions.
- **`DictationSettings::default().paste_method` must never be
  `PasteMethod::None`.** The top-level `PasteMethod` default is `None` for
  meeting mode; inheriting it would make enabling dictation silently do
  nothing. There is an explicit `Default` impl with a comment saying so.
- **External Script is deliberately absent from dictation's paste methods.**
  `external_script_path` stays global; offering the method while sharing the
  path would reintroduce cross-mode bleed.
- **Existing installs keep their shortcuts.** No migration, no schema bump — the
  bindings merge fills vacant keys only, so the new defaults reach fresh
  installs only.
- **One shared model across both modes, streaming-only catalog filter retained.**
  A known trade: a dictation user has less model choice than in plain Handy, and
  the streaming constraint that motivates the filter does not technically apply
  to dictation. Accepted to avoid model-swap churn. Revisit if it bites.
