# Fork-only translation catalogues

Strings that exist only in Shorthand, not in upstream Handy.

Upstream's catalogues live in `src/i18n/locales/<lang>/translation.json`.
They are supposed to stay **byte-identical to upstream** so
`git merge upstream/main` never conflicts on them — and, as of this file
existing, a permanent gate (`bun run check:locale-drift`) checks that on every
commit. That gate did not always exist: an earlier feature branch wrote 32
fork-only keys directly into all 24 of those files before anyone noticed, and
nothing caught it until this catalogue split was built. Fork strings cannot go
there. They live here instead and are merged into the bundle at build time by
`src/shorthand/vite-branding-plugin.ts` — after the Handy→Shorthand
substitution, which is why a string here may say "Handy" and mean it.

Two build steps merge these files back in, and both need to keep seeing the
same result. `src/shorthand/vite-branding-plugin.ts` does it for the
frontend bundle. `src-tauri/build.rs` does it a second time, independently,
for the handful of these keys (currently `tray.*` and `transcript.*`) the
Rust backend also needs — it generates `TrayStrings`/`TranscriptStrings` from
whatever `en.json` defines under those prefixes, the same way the frontend's
schema comes from `en.json`. That second consumer is easy to forget, because
nothing about editing a `.json` file looks like it touches Rust: the 32-key
migration that created this split ran only frontend gates and broke `cargo
build` for exactly that reason. Run the Rust gates
(`cargo build`, `cargo fmt --check`) too whenever a fork string file here, or
`english-copy.json`, changes.

One locale is out of scope for that byte-identity guarantee today:
`src/i18n/locales/tr/translation.json` carries 8 pre-existing keys whose
Turkish wording drifted from upstream's after an upstream key rename. That is
a translation-quality question, not fork content in the wrong place, and
`check:locale-drift` deliberately checks key *presence* only, so it does not
flag it. See `docs/superpowers/plans/2026-08-26-fork-only-translation-catalogues.md`
for the full accounting.

## Adding a language

The process is upstream's, from [CONTRIBUTING_TRANSLATIONS.md](../../../CONTRIBUTING_TRANSLATIONS.md):

1. Copy `en.json` to `<lang>.json`, matching a locale directory name under
   `src/i18n/locales/`.
2. Translate the values. Leave the keys alone.
3. Run `bun run check:fork-translations`.
4. Open a pull request.

Translate this file **and** upstream's `src/i18n/locales/<lang>/translation.json`
— together they are the whole UI.

Every key in `en.json` must be present. An untranslated key renders in English
rather than failing, but the gate still requires it: silent English in an
otherwise translated UI is a bug nobody reports.

`forkStringsFor(locale)` (`src/shorthand/branding.ts`) is what does the
merging: it always starts from `en.json` as a base, then layers the
requested locale's own file on top, so a partial catalogue still renders a
complete UI while `check:fork-translations` refuses to let it stay partial.
That base-then-layer order is also why an unrecognised or missing locale
file falls back to English rather than to a raw key path.

## Adding a *new* string — read this first

Before adding a key here, check whether upstream already has it:

```bash
bun scripts/audit-fork-strings.ts
```

If upstream has the same string and you only dislike its wording or
capitalisation, **do not add it here.** A fork string overrides that key in
every language, so an English preference silently replaces real translations
in all 23 of them. That happened to 44 keys before this directory existed.

- Purely an English capitalisation preference → `../english-copy.json`, which
  reaches English only.
- Genuinely new, or the fork's own terminology → here, and it needs
  translating like anything else.

**Never add a fork-only string directly to a file under `src/i18n/locales/`.**
That is the mistake `bun run check:locale-drift` exists to catch — it fails
the build on any key present there that upstream does not have. It happened
once, for 32 keys across all 24 locales, before the check existed.

Keys are flat and dotted (`"settings.modes.heading"`), unlike upstream's
nested catalogues. Both are valid i18next.
