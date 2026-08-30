# Follow-stream `begin.mode` Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Every `begin` record on the `--follow-stream` wire names the capture mode that produced the session, and `hello` advertises `begin-mode` so a follower can tell a build that predates the field from one that simply has not started a session yet.

**Architecture:** The app already knows the answer at the emission point — `TranscribeAction::start` writes the process-wide active-mode cell at `src-tauri/src/actions.rs:550`, and the sole non-test `hub.begin()` call is at `:617`. A new `FollowMode` serde enum lives in `follow_stream::protocol`, is re-exported from `follow_stream::mod`, and is threaded through `FollowStreamHub::begin`. The mapping from the app's `shorthand::mode::Mode` to it lives in `shorthand/mode.rs`, so `follow_stream` stays ignorant of the mode module and keeps its fork-only boundary.

**Tech Stack:** Rust, serde, Tauri. Tests are `cargo test` from `src-tauri`.

**Spec:** `D:/tools/shorthand-repos/shorthand-obsidian-plugin/docs/superpowers/specs/2026-08-29-plugin-ux-improvements-design.md` § 4

## Global Constraints

- **Additive under protocol 1. Do not bump `FOLLOW_PROTOCOL_VERSION`.** `FOLLOW_STREAM.md` states the rule: "A bump is reserved for a removal, a rename, or a changed event meaning." Adding a field is none of those, and consumers are already told to ignore fields they do not recognize.
- **But read `FOLLOW_STREAM.md` before you start, and take Task 4 Step 2 seriously.** This repo's `CLAUDE.md` records that "the protocol has already shipped a field addition without a version bump that silently dropped every event downstream." The rule above is right and the risk is real at the same time: the danger is not the missing bump, it is a consumer whose parser rejects a record carrying a field it does not know. `shorthand-core`'s `parseWireRecord` extracts named fields and ignores the rest, so it is safe — but that is a fact to confirm against a live follower, not to assume, which is exactly what the hand-verification step exists for.
- **Wire values are kebab-case:** `"meeting"`, `"assisted-notes"`, `"dictation"`. These match the existing capability-string convention (`"toggle-assisted-notes"`).
- **The new capability string is exactly `"begin-mode"`.**
- **Keep the diff mergeable** (`AGENTS.md` § "Keep the diff mergeable"). `follow_stream/` and `shorthand/` are fork-only modules and may be edited freely. `actions.rs` is an upstream file: change the two lines the feature needs and nothing else — no reformatting, no import reordering, no tidying.
- **Branch:** `feat/follow-stream-begin-mode`, cut from **`main`** at `34be65d` or later. One PR. `AGENTS.md` forbids sweeping up work that is not yours: **never `git add -A`, `git add .` or `git commit -a`,** stage the explicit paths each task names, and read `git diff --cached` before every commit.
- **Run before every push:** `cd src-tauri && cargo fmt && cargo clippy --all-targets -- -D warnings && cargo test`.
- **`FollowMode` must be re-exported from `follow_stream/mod.rs`.** `mod protocol;` is private (`mod.rs:5`) and the `pub use protocol::{…}` list at `mod.rs:18-21` does not name it. Without the re-export, `shorthand::mode` cannot name the type, and a `pub fn begin` taking an unreachable parameter type also trips clippy's private-interfaces lint under `-D warnings`.
- **There is no version bump and no tag in this plan.** The `v0.9.x` tags in this repository are upstream Handy's releases, inherited by the fork; the fork's own `version` fields were deliberately reset to `0.1.1` by the `feat: create Shorthand` commit and the fork has never cut a release of its own. The plugin does not compile against this repo — it spawns `shorthand.exe` — so nothing downstream is waiting on a version number. The change ships when a user installs a build containing it, and the plugin discovers it through the `begin-mode` capability rather than a version.

---

### Task 1: `FollowMode` and the `mode` field on `Begin`

**Files:**
- Modify: `src-tauri/src/follow_stream/protocol.rs` (enum definition near `FollowEvent`, around line 44-92; the existing `Begin` construction in its own tests at line 179-182)
- Modify: `src-tauri/src/follow_stream/mod.rs:18-21` (the `pub use protocol::{…}` re-export list)

**Interfaces:**
- Consumes: nothing from earlier tasks.
- Produces: `pub enum FollowMode { Meeting, AssistedNotes, Dictation }`, re-exported as `crate::follow_stream::FollowMode`, serializing to `"meeting"` / `"assisted-notes"` / `"dictation"`. `FollowEvent::Begin` gains a `mode: FollowMode` field, declared after `streaming`.

- [ ] **Step 1: Write the failing test**

Add to the existing `mod tests` in `src-tauri/src/follow_stream/protocol.rs`. Find the existing `begin` line assertion (it asserts the string `{"t":"begin","session":1,"streaming":true,"emitted_at":"2026-08-15T14:03:21.412-07:00","session_elapsed_ms":0}`) and add this test beside it:

```rust
    #[test]
    fn begin_names_the_capture_mode_in_kebab_case() {
        let stamp = Stamp::new(
            DateTime::parse_from_rfc3339("2026-08-15T14:03:21.412-07:00").unwrap(),
            Some(0),
        );
        // The wire spelling is the contract a follower gates on, so it is asserted
        // literally rather than round-tripped through serde.
        for (mode, expected) in [
            (FollowMode::Meeting, "meeting"),
            (FollowMode::AssistedNotes, "assisted-notes"),
            (FollowMode::Dictation, "dictation"),
        ] {
            let line = FollowEvent::Begin {
                session: 1,
                streaming: true,
                mode,
            }
            .to_line(&stamp);
            assert_eq!(
                &*line,
                format!(
                    "{{\"t\":\"begin\",\"session\":1,\"streaming\":true,\"mode\":\"{expected}\",\"emitted_at\":\"2026-08-15T14:03:21.412-07:00\",\"session_elapsed_ms\":0}}\n"
                )
            );
        }
    }
```

- [ ] **Step 2: Run the test to verify it fails**

```bash
cd src-tauri && cargo test --lib follow_stream::protocol
```

Expected: FAIL to compile — `cannot find type FollowMode in this scope`, and `struct variant FollowEvent::Begin has no field named mode`.

- [ ] **Step 3: Write the implementation**

In `src-tauri/src/follow_stream/protocol.rs`, add the enum immediately above `pub enum FollowEvent`:

```rust
/// Which capture mode produced a session, as it appears on the wire.
///
/// Deliberately its own type rather than `shorthand::mode::Mode` re-serialized:
/// this is a wire contract a follower gates behaviour on, and it must not
/// change spelling because someone renamed an internal variant. The mapping
/// between the two lives in `shorthand::mode`, so this module stays ignorant of
/// the mode cell.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum FollowMode {
    Meeting,
    AssistedNotes,
    Dictation,
}
```

Then add the field to the `Begin` variant, after `streaming`:

```rust
    Begin {
        session: u64,
        streaming: bool,
        /// Additive under protocol 1. An older follower ignores it; a current
        /// one uses it to decide whether a session is any of its business at
        /// all. Advertised by the `begin-mode` capability on `hello`, because
        /// "field absent" and "app predates the field" are the same bytes and a
        /// follower must not guess between them from a version number.
        mode: FollowMode,
    },
```

Add `FollowMode` to the re-export list in `src-tauri/src/follow_stream/mod.rs:18-21`, which currently reads:

```rust
pub use protocol::{
    FollowEvent, Speaker, ERR_DISABLED, ERR_FOLLOWER_LIMIT, ERR_SERIALIZATION_FAILED,
    FOLLOW_PROTOCOL_VERSION,
};
```

It becomes:

```rust
pub use protocol::{
    FollowEvent, FollowMode, Speaker, ERR_DISABLED, ERR_FOLLOWER_LIMIT, ERR_SERIALIZATION_FAILED,
    FOLLOW_PROTOCOL_VERSION,
};
```

`mod protocol;` is private, so without this the type is unreachable from `shorthand::mode` (Task 3) and a `pub fn begin` taking it trips clippy's private-interfaces lint.

Finally, the module's own tests already construct a `Begin` at `protocol.rs:179-182`. Add `mode: FollowMode::Meeting` to it and `,\"mode\":\"meeting\"` to the literal it asserts — that test is not about the mode, so meeting is the right filler.

- [ ] **Step 4: Run the test to verify it passes**

```bash
cd src-tauri && cargo test --lib follow_stream::protocol
```

Expected: the new test PASSES. Other tests in the crate still fail to compile — every `FollowEvent::Begin { .. }` and `hub.begin(..)` call site is now short an argument. Task 2 and Task 3 fix those; do not fix them here.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/follow_stream/protocol.rs src-tauri/src/follow_stream/mod.rs
git commit -m "feat: name the capture mode on follow-stream begin records

A follower cannot tell a meeting from a dictation burst, so it cannot
decide whether a session is its business. The app knows at the emission
point; the wire did not carry it."
```

---

### Task 2: Advertise `begin-mode` on `hello`

**Files:**
- Modify: `src-tauri/src/follow_stream/hub.rs:436-440` (the `capabilities` vec in `subscribe`) and its six tests asserting a literal `hello` line (746, 779, 839, 924, 978, 1129)
- Modify: `src-tauri/src/follow_stream/protocol.rs` — the `capabilities` doc comment at 50-58, and the `hello` test literal at 173
- Modify: `src-tauri/src/follow_stream/server.rs` — `hello` test literals at 386, 421, 458
- Modify: `src-tauri/src/follow_stream/client.rs` — `hello` test literal at 781
- Modify: `FOLLOW_STREAM.md`

**Interfaces:**
- Consumes: `FollowMode` from Task 1 (referenced by the doc text only).
- Produces: `hello.capabilities` is `["toggle-assisted-notes", "begin-mode"]`.

- [ ] **Step 1: Write the failing test**

Add to `mod tests` in `src-tauri/src/follow_stream/hub.rs`, beside the other subscribe tests:

```rust
    #[test]
    fn hello_advertises_begin_mode_so_a_follower_need_not_guess_from_a_version() {
        let hub = FollowStreamHub::default();
        hub.set_enabled(true);
        let (_follower, initial) = hub.subscribe("0.9.5").unwrap();
        assert_eq!(
            events(initial),
            ["{\"t\":\"hello\",\"protocol\":1,\"version\":\"0.9.5\",\"capabilities\":[\"toggle-assisted-notes\",\"begin-mode\"]}\n"]
        );
    }
```

`FollowStreamHub::default()`, `subscribe("0.9.5")` and the `events()` helper that strips stamps are all copied from the neighbouring `follower_observes_begin_partial_and_final_in_order`.

- [ ] **Step 2: Run the test to verify it fails**

```bash
cd src-tauri && cargo test --lib follow_stream::hub::tests::hello_advertises_begin_mode
```

Expected: FAIL — the assertion reports `"capabilities":["toggle-assisted-notes"]`.

- [ ] **Step 3: Write the implementation**

In `src-tauri/src/follow_stream/hub.rs`, at the `capabilities` vec inside `subscribe` (line 440), extend the list and the comment above it:

```rust
            // Advertises that this binary's parser accepts
            // `--toggle-assisted-notes`, and that its `begin` records name the
            // capture mode. Both exist so a follower can distinguish an older
            // installed app from a current one with the corresponding mode
            // simply turned off — a version-number guess is what this replaces.
            // See `FollowEvent::Hello`'s own doc comment.
            capabilities: vec!["toggle-assisted-notes", "begin-mode"],
```

The doc comment on `FollowEvent::Hello.capabilities` (`protocol.rs:50-58`) describes the field as naming *control flags* — "named exactly as the CLI flag minus its `--`". `begin-mode` is not a control flag, so that description becomes false. Generalize it, keeping the reason the field exists:

```rust
        /// Optional protocol capabilities this binary supports, as kebab-case
        /// names. Control flags appear here as the CLI flag minus its `--`
        /// (e.g. `"toggle-assisted-notes"`); other capabilities name a feature
        /// of the wire format (e.g. `"begin-mode"`, meaning `begin` records
        /// carry a `mode`). It advertises what this binary can do, never
        /// whether a mode is currently enabled — a follower still gets the
        /// app's own settings pane as the single description of behaviour.
        /// This exists so a follower can tell an installed binary that
        /// predates a capability from one that merely has the corresponding
        /// setting turned off, instead of guessing from a version number.
        /// Additive under protocol 1: an older follower ignores a field it
        /// does not recognize.
        capabilities: Vec<&'static str>,
```

Then update every test that asserts a literal `hello` line. They currently contain:

```
"capabilities":["toggle-assisted-notes"]
```

and must become:

```
"capabilities":["toggle-assisted-notes","begin-mode"]
```

They are in four files, not two — `hub.rs` (6), `server.rs` (386, 421, 458), `client.rs` (781) and `protocol.rs` (173). Find them all with:

```bash
cd src-tauri && grep -rn 'capabilities\\":\[\\"toggle-assisted-notes\\"\]' src
```

- [ ] **Step 4: Update `FOLLOW_STREAM.md`**

In the "NDJSON protocol" section, replace the two sample `hello` lines and the two sample `begin` lines so they carry the new field. The first sample block becomes:

```jsonl
{"t":"hello","protocol":1,"version":"0.9.7","capabilities":["toggle-assisted-notes","begin-mode"],"emitted_at":"2026-08-15T14:03:20.100-07:00"}
{"t":"begin","session":1,"streaming":true,"mode":"meeting","emitted_at":"2026-08-15T14:03:20.200-07:00","session_elapsed_ms":0}
```

Make the same two edits to the dual-speaker sample block below it (that one's `begin` uses `"session":42`).

Then extend the paragraph that begins "`hello` is always the first event on a connection". After the sentence ending "without guessing from a version number.", insert:

> The `begin-mode` capability says this binary's `begin` records carry a `mode` field.

And after the sentence ending "`streaming` says whether partial events are available for the selected model.", insert:

> `mode` names the capture mode that produced the session — `meeting`, `assisted-notes` or `dictation` — so a follower can decide whether a session is any of its business. Which modes reach a follower at all is still each mode's own publication setting, described at the top of this document; `mode` says what a delivered session was, not what is enabled. A follower that needs this field must gate on the `begin-mode` capability rather than on the field's absence, because an app that predates it is indistinguishable from one that has simply not started a session yet.

Finally, add `mode` to the list in the "Both fields were added without bumping `protocol`" paragraph, or add a sentence after it:

> `begin.mode` was added the same way and for the same reason.

- [ ] **Step 5: Run the tests to verify they pass**

```bash
cd src-tauri && cargo test --lib follow_stream
```

Expected: every `follow_stream` test that asserts a `hello` line PASSES. `hub.begin(..)` call sites still fail to compile — Task 3 fixes those.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/follow_stream/hub.rs src-tauri/src/follow_stream/protocol.rs src-tauri/src/follow_stream/server.rs src-tauri/src/follow_stream/client.rs FOLLOW_STREAM.md
git commit -m "feat: advertise begin-mode in the follow-stream hello

A missing mode field and an app that predates the field are the same
bytes. A follower that gates on the field alone would guess, and the
capability list is where that question is already answered."
```

---

### Task 3: Thread the active mode from `actions.rs` to the hub

**Files:**
- Modify: `src-tauri/src/shorthand/mode.rs` (add the `From` impl and its test)
- Modify: `src-tauri/src/follow_stream/hub.rs:296` (`begin`'s signature) and its 19 test call sites
- Modify: `src-tauri/src/follow_stream/client.rs:757,814` (two test call sites)
- Modify: `src-tauri/src/follow_stream/server.rs:388,424,452` (three test call sites)
- Modify: `src-tauri/src/actions.rs:617-620` (the one production call site)

**Interfaces:**
- Consumes: `FollowMode` from Task 1.
- Produces: `impl From<Mode> for FollowMode` in `crate::shorthand::mode`. `FollowStreamHub::begin(&self, streaming: bool, mode: FollowMode)`.

- [ ] **Step 1: Write the failing test**

Add to `mod tests` in `src-tauri/src/shorthand/mode.rs`:

```rust
    #[test]
    fn every_mode_maps_to_its_wire_spelling() {
        use crate::follow_stream::FollowMode;
        assert_eq!(FollowMode::from(Mode::Meeting), FollowMode::Meeting);
        assert_eq!(FollowMode::from(Mode::Dictation), FollowMode::Dictation);
        assert_eq!(
            FollowMode::from(Mode::AssistedNotes),
            FollowMode::AssistedNotes
        );
    }
```

And add to `mod tests` in `src-tauri/src/follow_stream/hub.rs`. The construction and the `events()` helper are copied from the neighbouring `follower_observes_begin_partial_and_final_in_order`, which is the closest existing test:

```rust
    #[test]
    fn begin_carries_the_mode_it_was_given() {
        let hub = FollowStreamHub::default();
        hub.set_enabled(true);
        let (follower, _) = hub.subscribe("0.9.5").unwrap();
        hub.begin(true, FollowMode::AssistedNotes);
        assert_eq!(
            events(follower.drain()),
            ["{\"t\":\"begin\",\"session\":1,\"streaming\":true,\"mode\":\"assisted-notes\"}\n"]
        );
    }
```

- [ ] **Step 2: Run the test to verify it fails**

```bash
cd src-tauri && cargo test --lib
```

Expected: FAIL to compile — `the trait bound FollowMode: From<Mode> is not satisfied`, and `this method takes 2 arguments but 1 argument was supplied` at every `hub.begin` call.

- [ ] **Step 3: Add the mapping**

In `src-tauri/src/shorthand/mode.rs`, after the `impl Mode` block:

```rust
/// The wire spelling of a mode, for `--follow-stream`'s `begin` record.
///
/// The mapping lives here rather than in `follow_stream::protocol` so that the
/// protocol module — which is the liftable, self-contained fork feature — does
/// not depend on the active-mode cell. Exhaustive by construction: adding a
/// `Mode` variant without a wire spelling is a compile error, which is the
/// point.
impl From<Mode> for crate::follow_stream::FollowMode {
    fn from(mode: Mode) -> Self {
        use crate::follow_stream::FollowMode;
        match mode {
            Mode::Meeting => FollowMode::Meeting,
            Mode::Dictation => FollowMode::Dictation,
            Mode::AssistedNotes => FollowMode::AssistedNotes,
        }
    }
}
```

- [ ] **Step 4: Change the hub's signature**

In `src-tauri/src/follow_stream/hub.rs`, change `begin`:

```rust
    /// `mode` is passed in rather than read here: the hub has no `AppHandle`,
    /// and the caller is `TranscribeAction::start`, which has already written
    /// the active-mode cell for this very capture.
    pub fn begin(&self, streaming: bool, mode: FollowMode) {
```

and the line that builds the record (currently line 325):

```rust
        let line = FollowEvent::Begin {
            session,
            streaming,
            mode,
        }
        .to_line(&self.stamp(Some(started)));
```

Add `FollowMode` to the module's existing `use` of `protocol` items.

- [ ] **Step 5: Update the test call sites — all four files, both halves**

**Do not reach for a blanket `sed` here.** The literal `begin` assertions come in stamped and unstamped shapes, they live in four files rather than two, and a regex that matches most of them leaves a handful of failures that look like a broken implementation rather than a missed rewrite. Enumerate, then edit.

Find everything first:

```bash
cd src-tauri && grep -rn 'hub\.begin(\|FollowEvent::Begin\|\\"t\\":\\"begin\\"\|capabilities\\":\[' src/follow_stream src/actions.rs
```

Expected as of 2026-08-29 — check the counts against what you actually see, and treat a mismatch as this plan being stale rather than as a reason to skip one:

| File | `hub.begin(…)` calls | Literal `begin` assertions | Literal `hello` assertions |
| --- | --- | --- | --- |
| `follow_stream/hub.rs` | 19 (tests) + 1 definition | 13 | 6 |
| `follow_stream/client.rs` | 2 (tests) | 3 full-line (486, 510, 782) | 1 (781) |
| `follow_stream/server.rs` | 3 (tests: 388, 424, 452) | at least 1 stamped (430) | 3 (386, 421, 458) |
| `follow_stream/protocol.rs` | — | 1 (in its own tests) | 1 (173) |

*Half one — the calls.* None of the 24 test calls is about the mode, so each takes `FollowMode::Meeting`. `server.rs` was missing from the first draft of this plan and is the file most likely to be forgotten; it is not optional.

*Half two — the assertions.* Each literal `begin` line gains `,"mode":"meeting"` immediately after `streaming`. Two shapes exist and both must be handled:

```
{"t":"begin","session":1,"streaming":true}
{"t":"begin","session":1,"streaming":true,"emitted_at":"…","session_elapsed_ms":0}
```

The stamped shape appears at `hub.rs:858`, `hub.rs:925` and `server.rs:430` among others — `mode` goes before `emitted_at`, because `StampedEvent` flattens the event's own fields first and `to_line` therefore emits them in declaration order.

Then confirm nothing was missed or over-written — in particular that `begin_carries_the_mode_it_was_given` keeps its `AssistedNotes` argument and assertion:

```bash
cd src-tauri && grep -rn 'hub\.begin(\|\\"t\\":\\"begin\\"' src/follow_stream | grep -v 'mode' 
```

Expected: only `client.rs:758`, which is `wait_until(|| output.text().contains("\"t\":\"begin\""))` — a substring probe, not a full-line assertion. It is correct unchanged; leave it. Anything else in that output is something you have not updated yet.

Finally, add `FollowMode` to the imports of each `mod tests` that now names it.

- [ ] **Step 6: Update the production call site**

In `src-tauri/src/actions.rs`, the block at lines 616-620. Keep the existing comment exactly as it is — it explains the publication gate, which is unchanged — and change only the call:

```rust
        if crate::shorthand::dictation::resolve_settings(app).follow_stream_enabled {
            if let Some(hub) = crate::follow_stream::hub(app) {
                // The cell was written by this same function at its top, for this
                // same capture, so it cannot describe a different one.
                hub.begin(
                    model_supports_streaming,
                    crate::shorthand::mode::active(app).into(),
                );
            }
        }
```

- [ ] **Step 7: Run the full suite**

```bash
cd src-tauri && cargo fmt && cargo clippy --all-targets -- -D warnings && cargo test
```

Expected: PASS, clean, no warnings.

- [ ] **Step 8: Commit**

```bash
git add src-tauri/src/shorthand/mode.rs src-tauri/src/follow_stream/hub.rs src-tauri/src/follow_stream/client.rs src-tauri/src/follow_stream/server.rs src-tauri/src/actions.rs
git commit -m "feat: emit the active capture mode on follow-stream begin

The mode cell is written by TranscribeAction::start sixty lines above
the hub.begin call that could not see it. Passed as an argument rather
than read from the hub, which has no AppHandle."
```

---

### Task 4: Verify against a real follower, then ship

**Files:** none — this task runs gates and opens the PR.

**Interfaces:**
- Consumes: everything above.
- Produces: the merged change on `main`. No version bump, no tag — see Global Constraints for why.

- [ ] **Step 1: Run the whole gate**

```bash
cd src-tauri && cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test
```

Expected: PASS.

```bash
bun run lint && bun run format:check && bun run build
```

Expected: PASS. (The frontend is untouched, so this is a regression check, not a change check.)

- [ ] **Step 2: Verify the real wire output by hand**

This is the step no test covers: the tests assert what `to_line` builds, not what a running app puts on the socket.

```bash
bun run tauri dev
```

In another terminal, with the built binary:

```bash
shorthand --follow-stream
```

Then, in the app: start a Meetings recording with its hotkey and stop it; enable Assisted notes under Settings → Modes → Notetaking → Assisted notes and do the same with its hotkey.

Expected on the follower's stdout: the `hello` line lists both capabilities, the first `begin` reads `"mode":"meeting"`, and the second reads `"mode":"assisted-notes"`. If a `mode` is missing or wrong, stop — the active-mode cell is not being written where this plan assumes, and the rest of the feature depends on it.

- [ ] **Step 3: Push and open the PR**

```bash
git push -u origin feat/follow-stream-begin-mode
gh pr create --title "feat: name the capture mode on follow-stream begin records" --body "$(cat <<'EOF'
## What

`begin` records now carry `mode` — `meeting`, `assisted-notes` or `dictation` — and `hello` advertises a `begin-mode` capability.

## Why

A follower could see that a recording started but not what kind. The Obsidian plugin needs that to decide whether to attach a meeting note to a session, and refusing to guess is the difference between following a meeting and writing a dictation burst into someone's note.

Additive under protocol 1, per FOLLOW_STREAM.md's own rule: a bump is reserved for a removal, a rename, or a changed event meaning.

## Verification

- `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test` — clean
- Verified by hand against a live `shorthand --follow-stream`: Meetings and Assisted notes hotkey recordings emit the right `mode`, and `hello` lists both capabilities

🤖 Generated with [Claude Code](https://claude.com/claude-code)
EOF
)"
```

- [ ] **Step 4: Merge**

After review, merge the PR. Nothing further ships from this repo — `shorthand-core`'s plan can start as soon as this is on `main`, and the two do not block each other at build time, only at the point a user actually exercises the feature.
