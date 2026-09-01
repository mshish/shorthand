# design-sync notes — shorthand-app

## What this repo is, for design-sync's purposes

Not a design system package. It is the Tauri application fork, and it has no
component library build — `dist/` is the Vite app bundle. Everything below
exists to bridge that gap, and none of it is how design-sync normally works.

The sync's audience is a **marketing landing page**, not the app UI. That is why
the scope is three components and a large token/utility layer rather than the
whole of `src/components/ui`.

## The two fork-only inputs

- **`.design-sync/ds-entry.tsx`** — passed as `--entry`. The converter's
  no-`dist` fallback is `export *` over every `.tsx` under `src/`, which drags
  the Tauri shell, the Zustand stores and the i18n runtime into the bundle. This
  file is the deliberate export surface instead. **Adding a component to the
  sync means adding it here and to `cfg.componentSrcMap`** — both, or the
  converter and the bundle disagree.
- **`.design-sync/build-css.mjs`** → `.design-sync/build/shorthand-ds.css`, which
  `cfg.cssEntry` points at. **Run it before every converter build**; the output
  is gitignored, so a fresh clone has no stylesheet until it runs.

## The build, in order

```sh
node .design-sync/build-css.mjs
node .ds-sync/package-build.mjs --config .design-sync/config.json \
  --node-modules ./node_modules --entry .design-sync/ds-entry.tsx --out ./ds-bundle
node .ds-sync/package-validate.mjs ./ds-bundle
```

`--node-modules ./node_modules` is the repo root; there is no package-local one.

## Why the stylesheet is compiled rather than scraped

The app's colour utilities (`bg-background-ui`, `text-logo-primary`) only exist
where Tailwind found them in the app's own markup. A landing page needs
utilities the app never used — `text-6xl`, `lg:grid-cols-3`, `py-32`. So
`tailwind-entry.css` mirrors the token half of `src/App.css` and adds an
`@source inline(...)` safelist for the vocabulary a page needs. That safelist is
the file's whole reason to exist; the alternative — pointing `cssEntry` at
`dist/assets/*.css` — ships only what the settings window happened to use.

Two things it adds beyond App.css, both bridged into `@theme inline` so they
become real utilities: `--color-highlighter` (the app only ever uses the coral
as a raw `var()` in marks.css) and a `font-display` `@utility` that carries
Fraunces' variation settings, since a bare family utility renders it at defaults
and loses the wonk.

Tailwind is pinned to **4.1.16** in `.ds-sync`, matching the app's own. A newer
minor renames utilities, and a class vocabulary the app's build does not have is
one that cannot be pasted back into the app.

Tailwind inlines `@import`ed stylesheets but does **not** rebase `url()` inside
them, so the Fontsource `@font-face` rules come out still pointing at
`./files/*.woff2`. `build-css.mjs` copies those exact files next to the output
so the paths resolve again. Rewriting them to absolute node_modules paths was
the alternative and is worse — it bakes one machine's layout into the artifact.

## Known render warns

Both are expected; a warn *not* listed here is new.

- **`[FONT_MISSING]` for "Atkinson Hyperlegible Next", "Fraunces", "Source Code
  Pro"** — false positive, and **not** an accepted substitution. The stacks in
  `brand/theme.css` lead with the *Variable* family names, and those three
  (`Atkinson Hyperlegible Next Variable`, `Fraunces Variable`, `Source Code Pro
  Variable`) do ship as `@font-face` in `fonts/`. The flagged names are the
  static-family fallbacks listed *after* them. Nothing renders in a system font.
  Verified by comparing `fonts/fonts.css` family names against the `--brand-font-*`
  stacks.
- **`tokens: 1 missing`** — below the converter's own threshold, unchanged since
  the first build.

## Preview decisions

- **No dark-ground story for `ShorthandWordmark`.** Its cream-inked variant is
  chosen by CSS from `:root[data-theme]` / `prefers-color-scheme`
  (`.sh-wordmark-*` in `brand/marks.css`), so a dark panel *inside* one card
  would still show the navy variant and misrepresent the component. Documented
  in its `.md` instead. To see it, switch the whole preview to dark.
- `Button`'s variant sweep is a fixed `grid-cols-4`, not `flex-wrap` — seven
  items wrapped 6+1 and read as an accident.
- `.design-sync/docs/*.md` exist because the synthesized `.prompt.md` spliced
  each example onto the *next* export's JSDoc. Their frontmatter `category` also
  supplies the groups (Brand, Actions); without them everything lands in
  `general`.
- `cfg.dtsPropsFor` is not optional here. With no `.d.ts` tree to extract from,
  every props interface came out as `[key: string]: unknown` — the design agent
  would not have known `variant` or `size` exist.

## Re-sync risks

- **`ds-entry.tsx` and `componentSrcMap` drift.** Nothing checks that they agree,
  and nothing checks either against `src/`. If a component is renamed or moved
  in the app, the build fails on the entry import — read that error as "the app
  moved", not as a converter fault.
- **`dtsPropsFor` is a hand-copied snapshot of three components' props.** It
  cannot notice when `Button.tsx` gains a variant. Re-read
  `src/components/ui/Button.tsx` on any re-sync and reconcile.
- **`conventions.md` names specific hex values and utility class names.** All
  were verified against `_ds_bundle.css` at sync time. Re-verify on re-sync —
  the palette lives in `brand/theme.css` and has been revised before.
- **`tailwind-entry.css` duplicates App.css's `@theme inline` block.** If the app
  adds a colour token to that block, this file will not learn about it and the
  utility will be missing from the landing page's vocabulary.
- **Safelist coverage is a judgement call, not a guarantee.** A design agent
  reaching for a utility outside the `@source inline` list gets nothing. Add the
  family to the safelist and rebuild rather than working around it.
- The `[FONT_MISSING]` warn will keep firing every run. Do not "fix" it with
  `cfg.extraFonts` — the fonts are already there.

## Upload state

Synced to `https://claude.ai/design/p/17aeefc2-32c3-499e-8957-131771d2a69c` —
38 files, 3 components, 10 graded cells, all `good`. `projectId` is pinned in
`config.json`, so a re-sync fetches `_ds_sync.json` from that project and only
re-verifies what actually changed.

`DesignSync` needs design-system authorization, which a non-interactive session
cannot grant. If a future run fails on that, the fix is `/design-login` once from
an interactive Claude Code terminal; the authorization is then reused headlessly.

**The brand rasters inline into the bundle as data URIs.** The wordmark artwork
(`brand-assets/mark-full-colour-transparent.png` and the `MARK_ASPECT_RATIO`
constant in `ShorthandWordmark.tsx`) changed mid-run, and only a full rebuild
picked it up — esbuild's `.png` loader is `dataurl`, so the image lives inside
`_ds_bundle.js`. Any change to those files means rebuild **and** re-upload;
nothing downstream will notice on its own.
