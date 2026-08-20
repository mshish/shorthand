# Frontend testing: the gap, and what to do about it

Written 2026-08-20, after shipping the dictation-mode branch, which added
roughly ten new fork-only React components with **zero** automated coverage.

## The state today

There is no unit-test harness for React in this repo. The only frontend tests
are two Playwright smoke checks in `tests/app.spec.ts`:

```ts
test("dev server responds", ...)   // asserts HTTP 200
test("page has html structure", ...) // asserts the HTML contains <html> and <body>
```

They run Vite without a Tauri backend, so they never reach the settings UI, and
they would pass against a completely broken application.

Every frontend behaviour in this repo is therefore verified by a human clicking
through a debug build, or not at all.

## Why it was left that way

The reasoning, recorded in
[the settings-UI spec](superpowers/specs/2026-08-17-shorthand-settings-ui-design.md)
and repeated in the dictation spec, was that adding vitest / jest /
testing-library means adding devDependencies to upstream's `package.json` and
`bun.lock` — permanent merge-conflict surface in a fork that merges from
upstream indefinitely.

That reasoning is sound **for those tools**. It was then over-generalised into
"we cannot test the frontend", which is not the same claim, and is false — see
below.

## What the gap actually cost

Two real defects on the dictation branch, both in the untested layer, both
found only by a reviewer reading code rather than by anything automated:

1. **`disabled` on `ShortcutInput` was cosmetic.** `SettingContainer` applies
   the prop only as `opacity-50` on text; it never reaches `{children}`. The
   recorder chip's `onClick` and the reset button had no disabled check. Result:
   one click on a greyed-out Reset button registered a global shortcut while the
   feature was switched off, and pressing it pasted into the focused window —
   a direct break of the branch's central "off by default" guarantee.
   A component test asserting "clicking Reset while disabled fires nothing"
   would have caught it in seconds. It survived eleven task-level reviews.

2. **The `dictation` store updater swallowed backend errors.** tauri-specta
   returns a backend `Err` as a *resolved* `{status: "error"}` value, not a
   rejection, so the updater never threw, the optimistic write never reverted,
   and a whole component written to detect that revert was dead code. Four
   sibling updaters in the same table handle this correctly. A store-level test
   asserting "a rejected command reverts the optimistic write" would have
   caught it.

Both shipped green: lint clean, build clean, 306 Rust tests passing.

## Playwright is already here

**`@playwright/test` is already a devDependency** (`package.json`), with
`test:playwright` scripts, a `playwright.config.ts`, and a CI workflow
(`.github/workflows/playwright.yml`) that runs on every pull request touching
`src/**`.

So the merge-conflict argument does not apply to Playwright at all. Real UI
coverage can be added **today, with zero dependency changes**, in a new
fork-only spec file that upstream will never touch.

The one genuine obstacle is that the settings UI calls Tauri commands, which do
not exist under plain Vite. That is solvable: stub `window.__TAURI_INTERNALS__`
with `page.addInitScript` before navigation and return canned responses per
command. It is ordinary work, not research.

## Options

| Option | Dependency cost | Conflict surface | Catches the two bugs above? |
| --- | --- | --- | --- |
| **Status quo** — manual checklist per feature | none | none | No. Both shipped. |
| **Playwright specs in a fork-only file** | **none — already installed** | one new file | Yes, both |
| vitest + testing-library | new devDeps in upstream's `package.json` + `bun.lock` | permanent | Yes, both, and faster to run |
| Contribute a harness upstream first | none, eventually | none | Only after upstream accepts |

## Recommendation

**Add Playwright coverage in a fork-only spec file.** It costs no new
dependencies, rides CI that already runs, and lives in a file upstream has no
opinion about.

Start with the cases that have already bitten, rather than chasing coverage:

1. A control rendered `disabled` does not act when clicked — assert it for the
   shortcut rows specifically, since that is where it broke.
2. A settings write that the backend rejects reverts in the UI and surfaces a
   message.
3. Per-mode settings do not bleed: set meeting and dictation to different paste
   methods, confirm each keeps its own.
4. The Dictation section is absent from the sidebar under the
   `show_all_settings` escape hatch, and present without it.

Items 1 and 2 are the ones with a demonstrated failure history. Item 3 is the
property the whole dictation design rests on.

Until that exists, the manual checklist in
[the dictation handoff](superpowers/plans/2026-08-20-dictation-mode-HANDOFF.md)
is the only thing standing between a frontend regression and a release, and it
depends on a human remembering to run it.

## What to stop saying

"There is no React test harness and adding one would add devDependencies to
upstream's `package.json`" appears in both specs and in every dispatch on the
dictation branch. Half of it is true. The half that matters — that the frontend
therefore cannot be tested — is not, because Playwright was already installed
the whole time. Future specs should say "no *unit* harness; use Playwright."
