# Review rubric: test coverage

> **DRAFT — to be refined with Mike.** The only existing steering on testing is
> [docs/FRONTEND_TESTING.md](../FRONTEND_TESTING.md), which covers the frontend.
> This rubric extends it to the whole repo and is seeded from four sources: the
> repo's own code and history, current primary-source practice on test coverage,
> agentic engineering practice, and the docs for the model the reviewers run on.
> See [Sources](#sources).

A reviewer applying this lens answers one question per changed behaviour: **would a
test fail if this change were reverted or broken?** Judge against
[AGENTS.md](../../AGENTS.md) first; where this rubric and AGENTS.md disagree,
AGENTS.md wins.

## Scope

This lens covers every **deterministic** behaviour: anything whose correct output
can be stated exactly. That includes the LLM post-processing plumbing, which is
deterministic once the model's reply is fixed:

- the request `src-tauri/src/llm_client.rs` sends: structured-output
  `response_format` with `strict: true`, and the reasoning-disable fields per
  provider;
- the reasoning-disable retry: a 400/422 on a request carrying those fields is
  retried without them, and the endpoint is remembered only if that retry
  succeeds;
- parsing in `post_process_transcription` (`src-tauri/src/actions.rs`): the
  `transcription` field is extracted from the JSON reply, a reply missing the
  field or failing to parse falls back to the raw content, and `<think>` blocks
  and invisible characters are stripped;
- `${output}` handling: removed from the system prompt in structured mode,
  substituted with the transcript in legacy mode.

Whether the model's _wording_ is good is not this lens; see
[llm-evals.md](llm-evals.md).

## Checks

### 1. A behaviour change comes with a test that would catch its reversal

- New or changed Rust logic has a `#[test]` / `#[tokio::test]` that exercises it.
  The crate already has about 600, so "there is no harness" is not a reason.
- A bug fix includes a test that fails without the fix. Two frontend defects on
  the dictation branch shipped with lint, build and 306 Rust tests green, and
  were found only by a reviewer reading code
  ([docs/FRONTEND_TESTING.md](../FRONTEND_TESTING.md)).
- The assertion targets the behaviour. A test that re-states a constant, or
  passes against a stubbed-out function, does not count.

Basis: Google's testing team, "A high code coverage percentage does not
guarantee high quality in the test coverage", and "What's not covered is more
meaningful than what is covered".

### 2. Fork tests do not add code to upstream files

Per AGENTS.md ["Keep the diff mergeable"](../../AGENTS.md#keep-the-diff-mergeable),
fork tests must not grow upstream files. `actions.rs` and `llm_client.rs` both
exist in `upstream/main`. A test for fork behaviour lives in a fork-owned file
and reaches the code through a visible surface: for example,
`llm_client::send_chat_completion_with_schema` is `pub`, so a test under
`src-tauri/src/shorthand/` can drive it against a local stub server without
touching an upstream line. The `serve_one_response` helper in `llm_client.rs`'s
own test module shows the stub technique.

When the code under test is reachable only from inside an upstream file, the
reviewer records it as a **known gap** and does not recommend editing that file to
close it. Closing it is a design decision for Mike, made deliberately as
AGENTS.md asks.

Existing drift, which new work does not copy:

- `actions.rs`'s upstream `mod tests` holds four fork-added tests.
- `src-tauri/src/audio_toolkit/audio/recorder.rs` declares
  `#[cfg(test)] mod shorthand_tests;` (bodies in `recorder/shorthand_tests.rs`)
  and also carries an inline fork-only `mod hardware_tests`.

### 3. Deterministic LLM plumbing is tested deterministically

The items under [Scope](#scope) need stubbed HTTP and fixture replies, never a
live provider. Check that:

- no test needs network access or a real API key;
- fixtures cover the failure branches, not only the happy path: a reply missing
  the field, unparsable JSON, a reply with no message content, a 400 on the
  reasoning fields, a failed retry (which must not be remembered), and a
  structured-output error that falls through to the legacy request.

Known gap: `${output}` handling, the parsing and the fallbacks cannot be reached
from a fork-owned test today. They live in private functions of `actions.rs`
(`build_system_prompt` and the `async fn post_process_transcription`), which no
module outside `actions.rs` can call. Even with access, the fork changed
`post_process_transcription` to take an `AppHandle` and read the key from the
credential store, and the `tauri` dependency is not built with its `test`
feature, so there is no mock `AppHandle`. Reviewers flag changes to this code as
untested until Mike decides how to close the gap.

### 4. Structured-output schemas stay strict

`post_process_transcription` sends a schema with `"additionalProperties": false`
and every property in `required`. A change to it keeps both:

- OpenAI's Structured Outputs (the `openai` provider; whether `zai`,
  `openrouter`, `cerebras` and `bedrock_mantle` enforce the same rules is not
  verified) says `additionalProperties: false`
  "must always be set in objects" and "All fields or function parameters must be
  specified as `required`".
- Anthropic's structured outputs also require `additionalProperties` "set to
  `false` for objects", but allow optional properties. That rule does not reach
  this schema today: the `anthropic` provider has
  `supports_structured_output: false`, so it uses the legacy path.

Apple Intelligence also has the flag set but never receives this schema. The
schema is a literal inside the private function in Check 3's known gap, so this
check is done by reading the diff, not by a test.

### 5. Frontend changes follow docs/FRONTEND_TESTING.md

There is no React unit harness, and adding one means devDependencies in upstream's
`package.json`. Use what is already installed:

- `bun run test:unit` (`bun test src/shorthand`) for fork-only logic in `.ts`
  files;
- Playwright specs in a fork-only file under `tests/`, stubbing
  `window.__TAURI_INTERNALS__` as `tests/telemetry-onboarding.spec.ts` does.

A new fork-only React component without either is a finding.

### 6. Tests were run, and the PR shows it

Anthropic's Claude Code guidance: have the agent "show evidence rather than
asserting success: the test output, the command it ran and what it returned".
The reviewer checks for that evidence. CI supplies it only partly:

- `test.yml` runs `cargo test` only when `src-tauri/**` changes;
- `playwright.yml` runs only when `src/**`, `tests/**`, `package.json`,
  `bun.lock` or `playwright.config.*` change;
- no workflow runs `bun run test:unit`, so its result must come from the PR
  description.

### 7. Tests generalise

A test passes because the code is right, not because the code was shaped to the
test. Anthropic's prompting guidance warns that Claude "can sometimes focus too
heavily on making tests pass" and asks for solutions that work "for all valid
inputs, not just the test cases". Reviewers flag hard-coded values or special
cases that exist only to satisfy a test.

## Line coverage: proposal only

The repo has no line-coverage tooling, and this rubric does not add any. The
proposal, for Mike to decide separately:

- run [cargo-llvm-cov](https://github.com/taiki-e/cargo-llvm-cov) in CI only,
  installed with `taiki-e/install-action@cargo-llvm-cov`;
- publish the result as a report — for example
  `cargo llvm-cov --lcov --output-path lcov.info` uploaded as an artifact — and
  not as a gate (no `--fail-under-lines`).

"Report, not gate" is Mike's decision for this repo, not Google's: the Google
post above does recommend gating ("We should gate deployments that do not meet
our code coverage standards"). The reason here is that a percentage does not
answer Check 1, and the post's own point that uncovered code is the meaningful
signal is served by a report.

Until then, reviewers answer Check 1 by reading the diff and the tests.

## How the reviewer reports

Report each finding with the file, the untested behaviour, and the concrete
failure a missing test would let through. Anthropic's guidance for review
subagents is to flag "only gaps that affect correctness or the stated
requirements", because a reviewer asked to find gaps "will usually report some,
even when the work is sound". Claude Opus 5.5 defaults to `medium` effort (Claude
Opus 5 defaulted to `high`), and Anthropic advises setting effort explicitly; set
it for review runs.

## Sources

Accessed 2026-10-10.

- Repo: [AGENTS.md](../../AGENTS.md), [docs/FRONTEND_TESTING.md](../FRONTEND_TESTING.md),
  `src-tauri/src/actions.rs`, `src-tauri/src/llm_client.rs`,
  `src-tauri/src/settings.rs`, `src-tauri/src/audio_toolkit/audio/recorder.rs`,
  `src-tauri/Cargo.toml`, `.github/workflows/`, `package.json`;
  `git diff upstream/main` for which lines are upstream's.
- Google Testing Blog, Code Coverage Best Practices (2020-08-07) —
  <https://testing.googleblog.com/2020/08/code-coverage-best-practices.html>
- cargo-llvm-cov README — <https://github.com/taiki-e/cargo-llvm-cov>
- OpenAI, Structured Outputs —
  <https://developers.openai.com/api/docs/guides/structured-outputs>
- Anthropic, Structured outputs —
  <https://platform.claude.com/docs/en/build-with-claude/structured-outputs>
- Anthropic, Best practices for Claude Code (verification, adversarial review
  step) — <https://code.claude.com/docs/en/best-practices>
- Anthropic, Prompting best practices ("Avoid focusing on passing tests and
  hardcoding") —
  <https://platform.claude.com/docs/en/build-with-claude/prompt-engineering/claude-prompting-best-practices>
- Anthropic, What's new in Claude Opus 5.5 —
  <https://platform.claude.com/docs/en/models/opus-5-5/whats-new-opus-5-5>
