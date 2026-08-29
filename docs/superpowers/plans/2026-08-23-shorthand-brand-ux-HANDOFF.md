# Shorthand rebrand — handoff

Written 2026-08-23, at the point where the visual identity is implemented and
verified but **nothing is committed**. The next session is expected to pick this
up with the `frontend-design` skill and take the UX further.

- **Branch:** `main` (the fork's integration branch — see AGENTS.md; the old
  `shorthand` branch is gone and `upstream/main` is used as a remote-tracking
  ref rather than a local mirror)
- **Base:** `41f2626`
- **Reference doc:** [`../../../BRANDING.md`](../../../BRANDING.md) — the
  decisions, in prose, meant to survive this handoff
- **Everything is in the working tree, unstaged.** Commit or stash before
  merging upstream.

## State

Green as of the last run: `bun run lint` clean, `bun x tsc --noEmit` clean,
`bun run build` succeeds, `bun run check:branding` and
`bun run check:translations` pass, and Prettier is clean on every file this work
touched.

`bun run format:check` **fails on 86 files** — including `vite.config.ts` and
`AGENTS.md` at HEAD, which this work never touched. That is a pre-existing
CRLF-vs-LF mismatch in the working tree, not a regression. Verified by running
Prettier against `git show HEAD:AGENTS.md`. Do not "fix" it by reformatting the
repo; that would be an enormous merge-conflict surface for no gain.

Rust was not touched at all, so `cargo test` was not re-run.

## What was built

### The design direction

**Copying-pencil violet on pad stock.** The violet is the aniline-violet lead
stenographers, clerks and telegraph operators wrote with — the only pencil that
transferred through carbon paper. It is the literal colour of taking down what
someone said.

This is the **second** accent. The first pass shipped teal, and Mike rejected it
as too common. If a third pass revisits the accent, the ruling-out reasoning is:

- **red / amber** — owned by `--color-error` and `--color-warning`. An accent
  there makes the primary button read as destructive.
- **green** — reads as success.
- **teal / blue** — free, and that is the problem: the default accent of most
  software written this decade. Rejected explicitly by Mike.
- **pink** — upstream Handy's, the thing we are moving away from.

### Palette

| Token                   | Light     | Dark      | Named for            |
| ----------------------- | --------- | --------- | -------------------- |
| `--color-text`          | `#1e1a24` | `#eae7ee` | ink                  |
| `--color-background`    | `#f0efea` | `#1b181f` | pad stock            |
| `--color-logo-primary`  | `#b295d8` | `#b69be0` | diluted ink          |
| `--color-logo-stroke`   | `#2a1b3d` | `#dcccf2` | ink outline          |
| `--color-background-ui` | `#6e3d9b` | (same)    | ink at full strength |
| `--color-mid-gray`      | `#7a757f` | (same)    | pencil               |

`--color-warning` and `--color-error` are left at upstream's values.

### Type, geometry, signature

- **IBM Plex Sans Variable** (UI) + **IBM Plex Mono** (paths, shortcuts, logs),
  self-hosted via `@fontsource`. Two new dependencies in `package.json`.
- **Radius scale halved**: `--radius-sm/md/lg/xl` → `2 / 3 / 4 / 6` px. 48 of
  the 49 `rounded-*` usages in `src/` resolve through these, so this cost zero
  component edits and does more than anything else to stop the app looking like
  upstream.
- **The ruled margin** is the one flourish: the sidebar's trailing edge is drawn
  in the accent (`border-e-2 border-logo-primary`) instead of neutral grey, and
  the selected row is marked by a stroke in its own margin
  (`border-s-2 border-background-ui` + `bg-logo-primary/25`) instead of the
  filled pink pill upstream uses.

### The mark

A lowercase "s" written with a pointed pen — thinning to nothing at the entry,
the waist and the exit. In the wordmark it **replaces** the initial S rather
than sitting beside the word: `[mark]horthand`. A bug next to "Shorthand" would
put two S's in the lockup and say nothing.

SVG cannot vary `stroke-width` along a path, so the visible shape is an outline
— the spine offset to both sides by a width profile. That is ~200 coordinates,
so it is generated, not hand-authored.

### Tray icons

The mark plus a badge in a fixed corner. The mark never changes; it is the only
thing identifying which app the tray item belongs to.

| State        | Badge                | Why                                        |
| ------------ | -------------------- | ------------------------------------------ |
| Idle         | none                 | the app, at rest                           |
| Recording    | solid dot            | the record symbol, unchanged since tape    |
| Transcribing | ring with a gap      | the shape every spinner uses for "working" |
| Warning      | exclamation on a dot | upstream's own convention, kept            |

This is also a **second** pass. The first distinguished recording from
transcribing by disc-versus-ring alone; Mike rejected it because the shapes had
no connection to the states they stood for. Upstream solves this by swapping the
glyph (hand → ear → brain), which is not available with one glyph.

Verified legible at 64, 22 and 16px on both light and dark trays.

## File map

Everything fork-specific is under `src/shorthand/brand/`:

| File                    | What it is                                                                                                                 |
| ----------------------- | -------------------------------------------------------------------------------------------------------------------------- |
| `theme.css`             | Palette, type, radius. **Only re-declares tokens** upstream and Tailwind already define — no new selector, no new utility. |
| `ShorthandMark.tsx`     | The mark. Fills with `currentColor`.                                                                                       |
| `ShorthandWordmark.tsx` | The lockup, set in the app's own type (HTML, not outlines).                                                                |
| `mark.generated.ts`     | Generated path data. Do not edit.                                                                                          |
| `mark.svg`              | Same path standalone, read by the icon generator.                                                                          |
| `index.ts`              | Barrel.                                                                                                                    |

Generators (both new, both fork-only):

- `scripts/gen-brand-mark.ts` — the spine and width profile live here. Run with
  **bun**.
- `scripts/gen-brand-icons.mjs` — tray PNGs + the 1024px app-icon master. Run
  with **node**, not bun: Playwright's browser launch hangs under bun on
  Windows. This is why it is `.mjs` and reads `mark.svg` with a regex instead of
  importing `mark.generated.ts`.

Art outputs: `src-tauri/app-icon.png` (master, at the filename `tauri icon`
defaults to), `src-tauri/resources/*.png` (11 tray files), and the whole
`src-tauri/icons/` tree.

### Upstream files touched — the whole budget

Five places. Keep it that way.

1. `src/App.css` — one `@import`, after `styles/theme.css`.
2. `src/overlay/RecordingOverlay.css` — one `@import`, same position.
3. `src/overlay/RecordingOverlay.css` — `--s-font` now reads
   `var(--brand-font-sans, …)`, keeping its old stack as the fallback.
4. `src/components/Sidebar.tsx` — wordmark, ruled edge, selected-row stroke, and
   `ShorthandMark` replacing `HandyHand` as the General section icon.
5. `src/components/onboarding/{Onboarding,AccessibilityOnboarding}.tsx` — the
   wordmark.

Plus a pointer bullet in `AGENTS.md` under "Keep the diff mergeable".

## Constraints that will bite

**The `--color-logo-primary` contract.** It must stay a _light tint in both
themes_. `Sidebar` sets `--color-text` on top of it at `/25`, and
`ui/Button.tsx`'s `primary-soft` variant does the same at `/20`–`/30`. A dark
value there silently drops those to unreadable. `--color-background-ui` is the
opposite contract: it carries **white** text (`primary` variant), so it needs
≥4.5:1 against white. The current `#6e3d9b` is 7.5:1.

**Why the token overrides work at all.** Tailwind v4 emits its theme variables
inside `@layer theme`, and _unlayered_ declarations outrank any layer. So a
plain `:root { --radius-lg: … }` in `brand/theme.css` beats Tailwind's own
value without needing an `@theme` block — which matters because the overlay
window does not import Tailwind, and gets the same file.

**`i18next/no-literal-string` is enforced in JSX.** The wordmark works around it
by hoisting the brand name into module constants (`NAME`, `NAME_TAIL`) rather
than carrying `eslint-disable` comments. Do the same for any new brand string; a
proper noun is deliberately not a translation key.

**Locale files are byte-identical to upstream on purpose.** The Handy →
Shorthand rename happens at build time via `src/shorthand/vite-branding-plugin.ts`
and `src/shorthand/branding.ts`. New fork-only UI strings go in
`FORK_ONLY_STRINGS` in `branding.ts`, English only. Never edit
`src/i18n/locales/*/translation.json`.

**`HandyHand.tsx` and `HandyTextLogo.tsx` are now unused but were left in
place.** Deleting a file upstream still maintains turns every future edit to it
into a delete/modify conflict, which is the expensive kind.

## Seeing the UI

The settings window needs Tauri, so `vite dev` alone renders nothing. There is
no committed preview harness — it was scratch, and deleted. Recreate it like
this (it takes about a minute and is how every screenshot in this work was
made):

1. `mkdir brand-preview`, and inside it an `index.html` that mounts
   `./preview.tsx`.
2. `preview.tsx` imports `@/App.css` and the real `ui/` primitives
   (`Button`, `SettingsGroup`, `SettingContainer`, `ToggleSwitch`) plus
   `@/shorthand/brand`, then hand-rolls the sidebar markup — `Sidebar` itself
   pulls `useSettings`, which needs Tauri.
3. Serve on a spare port: `bun x vite dev --port 5199`. **Port 1420 is often
   already taken** on this machine and `vite.config.ts` sets `strictPort: true`.
4. Screenshot with Playwright via **node** (`import { chromium } from
"@playwright/test"`), setting `document.documentElement.dataset.theme` to
   `"light"` / `"dark"` and awaiting `document.fonts.ready` before the shot.

Two traps: Chromium may need `bun x playwright install chromium` first, and
**do not gitignore the preview directory** — Tailwind v4 skips gitignored files
when scanning for class names, so any class not already used in `src/` would
silently fail to compile.

## Regenerating art

```bash
bun scripts/gen-brand-mark.ts        # after editing the spine / width profile
node scripts/gen-brand-icons.mjs     # node, not bun
cd src-tauri && bun x tauri icon     # slices app-icon.png to every platform
```

`tauri icon` also writes `src-tauri/icons/android/{mipmap-anydpi-v26,values}/`,
which are currently untracked additions.

## Deliberately not touched — candidate next steps

Scope was "nothing major", so all of this was left alone and is fair game:

- **Layout.** The 160px sidebar, the centred single-column content area, the
  `SettingsGroup` card stack and the footer are all upstream's. Nothing about
  the _structure_ of the settings window changed.
- **`ui/Button.tsx`'s `danger` variant** is still a hardcoded `bg-red-600`,
  ignoring `--color-error`. It now shouts louder than anything else on screen.
  That is arguably correct for a destructive action, but it was a judgement call
  and never a decision — a `theme.css` comment says the intent is to migrate
  those ad-hoc reds to the token over time.
- **Copy.** No user-facing string was rewritten. `SettingsGroup` titles are
  still uppercase micro-labels; several setting descriptions are long enough
  that they only exist in a tooltip.
- **The recording overlay's own layout** (`src/overlay/RecordingOverlay.css`).
  It inherited the palette and the typeface but its geometry, motion and the
  compact-pill/Live-panel behaviour were not reconsidered. It is the surface
  users actually see most.
- **Onboarding.** Both screens got the new wordmark and nothing else.
- **Motion.** There is none beyond `transition-colors` on nav rows. A
  page-load or section-change sequence was never explored.
- **The mark at 16px in the sidebar.** It stands in for the General section
  icon next to lucide icons drawn at a heavier optical weight; it reads a little
  light in that row.

## Working preferences worth knowing

- Mike rejects choices that are the common default. Naming _what in the
  subject's world_ supplies a colour or a shape — and saying which candidates
  were ruled out and why — is what makes a proposal land.
- Follow the standard, documented path for a tool unless told otherwise, and
  say what the standard path is before deviating. `tauri icon` and `@fontsource`
  were both chosen on that basis.
- Record the _actual_ reason for a constraint in the code, not a proxy for it.
