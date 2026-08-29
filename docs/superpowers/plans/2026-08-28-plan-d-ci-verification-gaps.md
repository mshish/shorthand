# Plan D — close the CI gaps that let unverifiable code merge

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make CI compile and lint the code that no developer machine can. Today a
branch can add hundreds of lines of macOS-only code, pass every gate a Windows or
Linux developer can run, and reach `main` before anything has type-checked it.

**Spec:** none. This plan is its own justification; the motivating evidence is in
`docs/superpowers/plans/2026-08-27-plan-c-system-audio-macos.md` and the branch
`feat/system-audio-linux-macos`.

**Status: not started.** Written as a handoff at the end of the session that
produced `feat/system-audio-linux-macos`.

---

## Read this first: the premise you were probably given is wrong

This plan was requested as "add GitHub Actions macOS builds, and Linux if it
doesn't exist." **Both already exist.** Verified on 2026-08-28 against
`.github/workflows/`:

| Workflow            | Trigger                                                    | Platforms                                                                                            |
| ------------------- | ---------------------------------------------------------- | ---------------------------------------------------------------------------------------------------- |
| `build-test.yml`    | `workflow_dispatch` only                                   | macos-26, macos-latest, ubuntu-22.04, ubuntu-24.04, ubuntu-24.04-arm, windows-latest, windows-11-arm |
| `main-build.yml`    | push to `main`                                             | same seven                                                                                           |
| `pr-test-build.yml` | `workflow_dispatch` with a PR number                       | same seven                                                                                           |
| `build.yml`         | `workflow_call` (the shared implementation)                | parameterised                                                                                        |
| `test.yml`          | push to `main`, **and pull_request**, paths `src-tauri/**` | **ubuntu-24.04 only**                                                                                |
| `code-quality.yml`  | paths `src/**`                                             | ubuntu-latest, frontend only                                                                         |
| `playwright.yml`    | paths `src/**`                                             | ubuntu-latest                                                                                        |
| `nix-check.yml`     | —                                                          | ubuntu-24.04                                                                                         |

Do not add a macOS build job. Read the table again and confirm it still holds
before doing anything — if it has changed since 2026-08-28, re-derive the gap
rather than trusting this plan's framing.

## The actual gap

**Nothing compiles macOS or Windows code before it reaches `main`.**

- `main-build.yml` builds all seven platforms — but only on **push to `main`**,
  which is _after_ the merge. It catches breakage; it does not prevent it.
- `pr-test-build.yml` and `build-test.yml` cover every platform but are
  **`workflow_dispatch`** — a human has to remember, and know to.
- `test.yml` is the only thing that runs automatically on a pull request, and it
  runs **`cargo test` on ubuntu-24.04 alone**. On Linux, every
  `#[cfg(target_os = "macos")]` and `#[cfg(windows)]` block is stripped before
  the compiler sees it.

**`cargo clippy` does not run anywhere in CI.** Zero matches for `clippy` across
all nine workflows. `AGENTS.md` asks developers to run it before committing;
nothing enforces it.

### Why this is not hypothetical

The branch this plan hands off from, `feat/system-audio-linux-macos`, added a
macOS permission module, a macOS device guard and a macOS enable path — several
hundred lines, all `#[cfg(target_os = "macos")]`. On the Windows machine that
wrote it: `cargo check` passed, `cargo clippy -- -D warnings` passed, 332 tests
passed, `tsc` passed, every content gate passed.

A review agent then compiled the macOS-gated code in a scratch crate against real
cpal and found **it would not build at all**:

- a pattern-guard binding borrowed mutably (`E0596`) — the entirety of one task
- five `build_input_stream` calls passing `&StreamConfig` where cpal 0.18 takes
  `StreamConfig` by value (`E0308`)
- four further `-D warnings` failures, one of them only in the
  `--no-default-features` build

Every one of those would have merged green and surfaced on a Mac. That is the
class of failure this plan closes, and the reason the fix is worth more than a
faster build.

---

## How to run this work

This process is not ceremony. On the branch this hands off from, **every phase's
review found something the gates could not**, including two design errors made by
the agent writing the plan itself. Follow it.

1. **Research first, in subagents, before writing anything.** Dispatch research
   subagents for the open questions listed under each task. Require primary
   sources — GitHub's own docs, the actions' own READMEs, the repo's existing
   workflows — and require them to state "unverified" rather than guess. On this
   branch, three separate confident-sounding claims turned out to be wrong when
   someone actually checked: that CI ran no tests, that a documented Chromium
   timeout existed, and that a permission API failed on denial.
2. **Implement in a subagent**, with a brief that states what is already known so
   it does not re-derive it, and what is explicitly out of scope.
3. **Review in a separate subagent** — never the one that implemented. Give the
   reviewer the plan, the diff, and an explicit instruction to verify claims
   against primary sources rather than reasoning about them. The highest-value
   review on this branch built throwaway crates to _compile_ its findings instead
   of asserting them.
4. **Verify yourself.** Do not trust an implementer's self-report of a passing
   gate. Run it. On this branch an implementer's verification loop was silently
   broken by shell quoting and it reported nothing useful for forty minutes.
5. **Fix findings, then re-verify**, then commit.

Two hard-won cautions:

- **A tool's status API can lie.** Check the working tree and run the gates; that
  is ground truth. Do not report "done" on the strength of a status line.
- **State what is unverified.** Every plan on this branch that survived contact
  with reality did so because it labelled its guesses. Carry that habit into the
  CI work, where "I think this action does X" is very easy to write.

### Local build note (this machine only)

Building `transcribe-cpp-sys` on the Windows dev machine hits a known MSBuild
FileTracker race (`FTK1011`), unrelated to this repo — it reproduces across
unrelated ecosystems and is documented in `clcache#265`, `npm/cli#7932` and
`microsoft/Olive#1623`. Workaround is `TrackFileAccess=false`, or lowering build
parallelism. **This belongs in nobody's committed configuration** — CI is
unaffected, and a workaround for one machine's toolchain would outlive the
problem and mislead the next reader.

---

## Global constraints

- **Additive only.** Per `AGENTS.md`, `.github/workflows/build.yml` and its
  callers are upstream files. Prefer a new workflow file over restructuring
  `build.yml`; if you must edit it, keep the change small and local.
- **Do not make PRs slower than they are useful.** A full seven-platform Tauri
  release build on every PR is minutes of runner time and will be routed around.
  Compile-and-lint is the goal, not artifacts.
- **Free-tier minutes are not free on macOS.** GitHub bills macOS runners at a
  multiplier against private-repo minutes. `mshish/shorthand` is private (see
  `SIGNING_AND_UPDATES.md`, which plans around exactly that). Cost is a design
  constraint here, not an afterthought — Task 1 exists to quantify it.
- **Do not fix the six pre-existing clippy findings by editing upstream files.**
  Five sit in `clipboard.rs` and `shortcut/*`, which this fork does not own.
  Task 3 decides how to introduce clippy without a red gate on day one.

---

### Task 1: Research — establish the facts before designing anything

**Files:** none. Produces findings that decide Tasks 2 and 3.

Dispatch research subagents. Every answer needs a citation; anything uncertain
must be labelled unverified.

- [ ] **Cost.** What does a macOS runner minute cost against a private repo's
      quota, at what multiplier, and what is this account's plan and remaining
      allowance? What does a `cargo check` on macos-latest actually cost in
      wall-clock for this project — the native `transcribe-cpp-sys` build is the
      dominant term, so a cache miss and a cache hit are very different numbers.
- [ ] **Caching.** Does `swatinem/rust-cache` (already used in `test.yml`) cache
      the `transcribe-cpp-sys` CMake output, which lands outside `target/` in
      `~/Library/Caches` or `%LOCALAPPDATA%\tcs`? If not, what would? A macOS job
      that rebuilds ggml and the Vulkan/Metal shaders every run may be too slow to
      keep, and that single answer decides whether Task 2 is cheap or expensive.
- [ ] **Scope of a check-only job.** Can `cargo check`/`clippy` build this crate
      on macOS without the full Tauri bundling step, and what is the minimum
      dependency set? Note `build.rs` compiles a Swift bridge for Apple
      Intelligence on aarch64 — establish whether that runs under `cargo check`
      and what it needs.
- [ ] **`--no-default-features`.** The `macos-tcc-spi` feature added on
      `feat/system-audio-linux-macos` has a feature-off path that only a macOS
      build can type-check. Confirm `cargo check --no-default-features` is
      meaningful on macOS and cheap enough to add.
- [ ] **Windows too.** The same blindness applies to `#[cfg(windows)]` on a Linux
      PR run. Cost the Windows equivalent; it may be cheaper than macOS and worth
      including in the same task.
- [ ] **Does anything already do this?** Re-read every workflow. Confirm the table
      above still holds and that no path/branch filter makes an existing job cover
      this already.

---

### Task 2: Compile the platform-gated code on pull requests

**Files:** likely a new `.github/workflows/`, or an extension of `test.yml`.

**Interfaces:** produces a required-able check that fails when macOS-gated (and
ideally Windows-gated) code does not compile.

- [ ] **Design from Task 1's numbers**, not from this plan's guesses. If macOS
      minutes are prohibitive on every PR, acceptable fallbacks in preference
      order: run only when `src-tauri/**` changes (matching `test.yml`); run on a
      label; run on merge queue rather than every push. Record which you chose and
      why — a future reader needs the reasoning, not just the YAML.
- [ ] **`cargo check --all-targets` at minimum**, on macOS. That alone would have
      caught both hard errors described above.
- [ ] **Add `--no-default-features`** if Task 1 finds it affordable. The
      feature-off build is an escape hatch nobody makes by accident and therefore
      exactly the one that rots.
- [ ] **Do not run `cargo test` on macOS** without deciding what to do about the
      hardware tests. `device.rs` and `recorder.rs` carry
      `#[cfg(test)] mod hardware_tests` that open real audio devices; they
      self-skip via `std::env::var("CI")`, which GitHub sets — verify that holds
      on a macOS runner rather than assuming.
- [ ] **Verify it actually fails on broken code.** Push a deliberate macOS-only
      compile error to a scratch branch, confirm the job goes red, then remove it.
      A gate nobody has seen fail is not known to work.

---

### Task 3: Run clippy in CI

**Files:** likely `test.yml` or a new workflow.

Nothing lints Rust in CI today. `AGENTS.md` asks for it by convention only.

- [ ] **Establish the baseline first.** On 2026-08-28, `clippy -D warnings` on
      Linux reported six findings that predate current work: one
      `clippy::chunks_exact` in `recorder.rs` (fork-only code, from `e22a920`) and
      five `needless_return`/`needless_late_init` in `clipboard.rs`,
      `shortcut/handy_keys.rs` and `shortcut/tauri_impl.rs` — **all upstream
      files**. Re-check; the count may have moved.
- [ ] **Decide how to land clippy without a red gate**, and record the decision.
      Options, with the trade-off stated: fix only the fork-only finding and
      `#[allow]` the upstream ones with a comment pointing at this plan; pin a
      toolchain so lint results stop depending on who ran it; or start clippy
      as non-blocking and tighten later. Do **not** silently reformat upstream
      files to satisfy a lint upstream does not run — `AGENTS.md` is explicit
      that this trades a clean merge for a manual one.
- [ ] **Pin the toolchain.** Windows had clippy 0.1.97 and WSL 0.1.98 during this
      work, and they disagreed — the newer one reported six findings the older did
      not. Without a pin, this gate's result depends on the machine. A
      `rust-toolchain.toml` is the conventional answer; confirm it does not
      disrupt the existing `dtolnay/rust-toolchain@stable` steps.
- [ ] **Run clippy on macOS too**, if Task 2's job exists — that is where the
      unverifiable code lives, and four of the nine findings on the handoff branch
      were lint failures rather than compile errors.

---

### Task 4: Record what CI still cannot do

**Files:** `docs/` — a short note, or an addition to `BUILD.md`.

CI compiles; it does not plug in a microphone. Three manual matrices remain
un-runnable and should be written down as such so nobody assumes a green tick
means the feature works:

- `docs/superpowers/plans/2026-08-27-plan-a-cpal-018-migration.md` Task 7 —
  Windows regression after the cpal 0.18 migration, including whether saved
  device selections still resolve.
- `docs/superpowers/plans/2026-08-27-plan-b-system-audio-linux.md` Task 8 —
  PipeWire, PulseAudio, and neither.
- `docs/superpowers/plans/2026-08-27-plan-c-system-audio-macos.md` Task 6 — the
  consent prompt, the denied-state CTA, the re-grant path, and which System
  Settings pane the deep link actually opens.

- [ ] **Write the note**, naming those three and stating plainly that a green CI
      run does not cover them.

---

## Open questions for the human

- Is `mshish/shorthand` still private, and is the macOS-minute cost acceptable on
  every PR touching `src-tauri/**`? Task 1 quantifies it; the answer is a budget
  decision, not a technical one.
- Should clippy be blocking from day one, or non-blocking until the six
  pre-existing findings are resolved?
