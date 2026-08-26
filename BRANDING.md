# Shorthand branding

How the fork looks different from upstream Handy, why each decision exists,
where it lives, and how to regenerate the artwork.

## The direction

Handy is pink on warm grey, with a waving hand for a logo. Shorthand turns a
fleeting voice into a written thing: **the bird carries the thought, and the
fountain pen commits it**.

The approved clay artwork makes local software feel handmade and quietly
helpful. Its small silhouette reduces the same story to a bird-and-pen contour
when the material detail disappears. The UI keeps the useful discipline of the
earlier “marked-up transcript” direction — warm paper, calm ink, one live mark —
but now takes its colour and display character from the artwork rather than from
an independent stationery metaphor.

| Role                      | Light     | Dark      | Why                                             |
| ------------------------- | --------- | --------- | ----------------------------------------------- |
| `--color-background`      | `#FAF5EA` | `#111820` | warm paper, restrained for long reading         |
| `--color-text`            | `#14202B` | `#F6F1E8` | blue-black / cream ink, never clinical extremes |
| `--color-logo-primary`    | `#0B5F8A` | `#63B7D6` | the bird’s characterful ocean blue              |
| `--color-logo-stroke`     | `#084A6C` | `#92D4E7` | the accent’s paired edge or stroke              |
| `--color-background-ui`   | `#2E6F9E` | (same)    | primary-action fill carrying white text         |
| `--color-mid-gray`        | `#5C6770` | `#AAB4BE` | secondary text                                  |
| `--brand-highlighter`     | `#F3673C` | (same)    | the one live-now background                     |
| `--brand-highlighter-ink` | `#14202B` | `#111820` | dark ink carried by the coral                   |

The page background is deliberately more neutral than the raster’s peach
lighting: the raster describes clay under light; the token has to carry an
interface for hours. The text pair measures 15.19:1 and 15.88:1 against its own
grounds, keeping long reading calm without pure black or white.

Coral is the living, tactile counterpoint already present in the bird’s wing
and the approved underline. It marks the one thing happening _now_; it is
explicitly neither a warning nor a success signal.

### Four accent directions, not one inevitable answer

Keep the sequence. Each direction was reasonable in its own context, and losing
that context is how rejected work gets proposed again.

| Direction                   | Why it was chosen                                     | What happened                                                               |
| --------------------------- | ----------------------------------------------------- | --------------------------------------------------------------------------- |
| teal                        | inherited, friendly, familiar                         | rejected as too common — the default “friendly tech” accent                 |
| copying-pencil violet       | archival, indelible, a real stationery reference      | rejected because it read clerical and bureaucratic                          |
| blue ink + rose highlighter | made “a marked-up transcript” literal and non-generic | shipped successfully; superseded by approved artwork, not killed by a fault |
| ocean blue + coral          | derived from the bird, wing and underline             | approved pack; the current system                                           |

Blue alone remains generic. What rescues the current blue is that it is not an
arbitrary software accent: its hue is shifted toward the bird’s ocean blue and
paired with the coral already living in the wing and underline.

The sweep itself also passed through chartreuse and apricot before rose, and
that history still matters. Chartreuse `#E8F35C` was sold as “the complement of
blue” but was yellow-green, fought the warm paper, and made the most predictable
possible stationery object. Apricot `#FFC48A` belonged to the paper’s hue
family so completely that it read as a tint _of_ the page rather than a mark
_on_ it. Rose solved both and shipped. Coral replaced it only because the
approved artwork supplied a stronger source.

Still ruled out by role: green reads success; amber and orange belong to
`--color-warning`; violet belongs to the rejected clerical direction.

## Two tokens, because there are two jobs

This is the part most likely to be broken by a well-meaning change.

`--color-logo-primary` is an existing, widely used accent token. It serves
foreground text, icon fill, focus rings, selected controls, solid fills and pale
tints. Its foreground and ring jobs force it to flip by theme: the current pair
measures 6.40:1 on light paper and 7.89:1 on the dark ground. Keep
`--color-logo-stroke` paired with the accent from the same theme; it is the
accent’s partner edge, not body ink.

`--brand-highlighter` is fork-only because the sweep cannot mean _live_ if it
shares a token with every spinner, progress bar, badge and selected control.
Coral is invariant; its ink flips. It is a background only — never body text,
and never a fill carrying white text. Dark ink on it measures 5.36:1 in light
and 5.80:1 in dark.

That yields an accent hierarchy, not a universal semantic system:

- the **highlighter** means _live_, and marks nothing else;
- the **ink accent** means _set_ — checked, selected, primary. Conventional,
  and deliberately left conventional.

## Contrast and theme-specific values

`--color-background-ui` must carry white text and remain a visible non-text fill
against both grounds. The invariant `#2E6F9E` measures 5.41:1 under white text,
4.97:1 against light paper, and 3.30:1 against the dark ground. It is also the
tray’s one-colour fallback: 5.41:1 on white and 3.88:1 on black.

`--color-mid-gray` cannot be a single value. Secondary text has to become darker
on light paper and lighter on the dark ground; one grey cannot move in both
directions while retaining the required 4.5:1 contrast. The pack therefore
supplies a pair, measuring 5.32:1 and 8.49:1. Upstream declares the active token
once, so `src/shorthand/brand/theme.css` mirrors upstream’s theme-selection
blocks to resolve it correctly.

## The mark and lockup

There are two renderings of the mark, used for two different jobs.

**The one-colour silhouette** (`mark.paths.ts` / `mark.svg`, rendered by
`ShorthandMark.tsx`) is a bird perched on a fountain pen reduced to its
contour: four paths, two using `fill-rule="evenodd"` to punch counters through
the bird’s body and pen barrel. It fills with `currentColor` or an explicit
colour, so it can inherit ink in the UI or be rasterised into each tray
treatment without owning a fixed palette — this is what `gen-brand-icons.mjs`
draws the app icon and every tray state from.

The previous mark was a lowercase “s” written with a pointed pen. SVG cannot
vary `stroke-width` along a path, so its visible stroke had to be an outline: a
spine offset on both sides by a width profile, producing roughly 200
machine-derived coordinates. That was why `scripts/gen-brand-mark.ts` existed.
The reason disappeared with the old mark, so the generator disappeared too.

The silhouette is approved artwork, not editable geometry. `mark.paths.ts` and
`mark.svg` are transcribed from `brand-assets/mark-silhouette.svg`. If the
artwork changes, re-transcribe it; do not hand-tune either copy into a fork of
the source of truth.

**The full-colour clay render** (`brand-assets/mark-full-colour-transparent.png`)
is what `ShorthandWordmark.tsx` actually places in the UI — the sidebar, and
the onboarding/About lockup. An initial version reused the one-colour
silhouette here too, reasoning that the word carrying the theme’s ink was
enough colour for one lockup; the raster mark replaced it because the approved
artwork’s own colour is part of what makes it read as the product’s identity
rather than a generic line icon, and the illustration’s palette (ocean blue,
coral, cream) doesn’t need to flip for the theme the way a silhouette’s ink
does. The PNG is already tightly cropped to the drawing, so `ShorthandWordmark`
sizes it by its own aspect ratio (845:498) rather than by measured bounds
inside a viewBox, the way it sizes the silhouette elsewhere.

The old `[mark]horthand` lockup substituted the written “s” for the initial S.
Its argument was coherent — a separate bug beside “Shorthand” would print two
S’s — but the approved artwork settled the question differently. The shipped
lockup stacks the coloured bird and pen above the complete word **Shorthand**,
with its coral swash beneath, matching the composition of the raster.

### The word is artwork, not type

`brand-assets/wordmark-full-colour.png` is the approved clay render of the
word and its swash, and it is what ships. `scripts/gen-brand-wordmark.mjs`
derives the two assets the UI loads into `src/shorthand/brand/`:
`wordmark-light.png` (the artwork, resized) and `wordmark-dark.png` (the same,
with the ink remapped to cream).

The word used to be live Fraunces type. That was never a type decision — it was
a workaround for one problem: the artwork’s ink is a fixed navy, and fixed navy
is invisible on the dark ground. Live text followed `--color-text` and solved
it, at the cost of the clay — texture, bevel and swash all became flat type,
and the face did not match the artwork’s letterforms anyway.

Three ways to keep the artwork were considered:

| Approach                               | Why it was rejected or chosen                                                                                                                                                                               |
| -------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| trace the render to full-colour vector | does not solve the problem — the navy ends up baked into paths instead of pixels, so dark is still broken; needs a new devDependency, and a traced clay render is typically larger than the PNG it replaces |
| outline the font, or re-set it in one  | there is no font to outline: the artwork is a rendered illustration of letterforms, and the `.svg` in the pack is a PNG in an `<svg>` wrapper — one `<image>`, no paths, no `<text>`, no `@font-face`       |
| **remap the ink per theme**            | **chosen.** Only the navy pixels move, onto a cream ramp keyed to each pixel’s own luminance, so the clay texture and bevel survive intact                                                                  |

The coral swash is left untouched by the remap. Coral is theme-invariant in
this palette and already clears contrast on both grounds, so it needs no
variant — which is also why the component no longer draws a CSS bar of its own.

Both variants are rendered into the DOM and `marks.css` picks one with
`display`. That is not a preference for CSS over a React check: the theme can
come from the OS preference _or_ an explicit `data-theme` override, and only
CSS sees both without the component subscribing to a store it otherwise has no
reason to know about. `<picture>` cannot do it either — it resolves
`prefers-color-scheme` but is blind to the attribute, so an override would show
the wrong variant.

Being artwork, the word can no longer be restyled by a type change, and the
generator has to be re-run if the approved wordmark is re-rendered.
`brand-assets/FONT.md`’s `[mark]horthand` table therefore no longer governs the
lockup at all; Fraunces remains the display face for headings and the About
lockup, but not for the word itself.

## The active-tab indicator, and the sweep it replaced

The active Transcription/Dictation tab carries a plain 2px coral bar
(`.sh-tab-indicator` in `marks.css`) under its label. Coral keeps meaning “the
one you’re looking at now” — see the accent hierarchy above — with a fade-in
rather than a directional wipe, so it needs no mirroring for the app’s RTL
locales.

That indicator replaced a hand-drawn highlighter sweep — a gradient, four
mismatched corner radii, a rotation and a companion pen line beneath, built to
carry “a marked-up transcript” onto the one live UI element that used it. Its
construction rules were real findings, earned by screenshotting both themes,
not decorative choices:

1. No `mix-blend-mode: multiply` — physically correct on paper, but multiply
   over a dark ground deposits almost nothing (`#D9E84A` over `#12141A`
   resolved to roughly `#0F1208`, a black smudge).
2. Mark text, never a container — at row scale the rotation, gradient and
   asymmetric radii all wash out into the same flat active-state rectangle the
   motif existed to avoid.
3. The label needs to be long enough to hold a stroke: below roughly a 5:1
   rendered aspect ratio the corner radii eat the whole perimeter.
4. Overshoot lopsided, about 3:1 horizontal to vertical, or the mark gets
   roomier and less like a stroke at the same time.

It shipped, and it photographed well in the component gallery — but a tab bar
in real use is ordinary UI chrome, not a piece of marked-up paper, and the
sweep read as noise there once it stopped being a curated screenshot. The
lesson worth keeping isn’t the construction rules above; it’s that a motif
proving itself in an isolated demo still has to earn its place in the actual
surface it ships on, and it hadn’t. `.sh-sweep` and the running-text
demonstration in `brand-preview/gallery.tsx` were removed with it — the
gallery no longer needs to show an idea nothing in the app implements.

The sidebar was never in scope for either version: its labels are shorter than
the 5:1 floor above (“App” renders at 1.2:1), so it marks selection with an
accent icon and full-weight label against dimmed neighbours instead. That also
protects the distinction between _live_ (the tab indicator) and merely
_selected_ (the sidebar row).

## Dependent settings: a rule, not a box

`src/shorthand/ui/Dependents.tsx` draws rows unlocked by a toggle as a 3px
accent rule in the margin with a small indent. A margin rule is how a marked-up
page says “this part belongs with that part”; another container is not.

Both alternatives were tried:

- A low-alpha accent fill becomes another neutral control surface before it
  becomes recognisably blue. Raising the alpha enough to show blue turns the
  group into a card — the object the redesign removed.
- A deep indent takes width from `SettingContainer`’s already constrained
  label/control layout. The cleanup hotkey was the failure case: 30px pushed
  its shortcut chip over the description. The shipped indent costs 13px and
  takes nothing from the right edge.

Dependents are **hidden, never disabled**. `SettingContainer`’s `disabled` prop
fades the title and stops there; a “disabled” `ShortcutInput` can still register
a live global hotkey for a feature that will not run.

## Type and geometry

**Atkinson Hyperlegible Next remains the UI text face.** The Braille Institute
drew it so characters resist confusion, which suits an application whose output
is a written record. The earlier rejection of Fraunces was specific and still
correct: a serif at 13px setting-row labels is a legibility bet a transcription
app should not take.

The approved artwork did not overturn that objection; it scoped Fraunces to the
job it is good at. **Fraunces Variable is display-only** — wordmark, headings,
and the onboarding/About lockup — using `--brand-font-display`, weight 650 and
the pack’s optical-size, softness and wonk settings. It never replaces
`--font-sans` or `--default-font-family`.

**Source Code Pro Variable** replaces Atkinson Hyperlegible Mono for transcripts
and time-aligned text through `--brand-font-mono`. All three faces are
self-hosted through Fontsource. Self-hosting is load-bearing: the app works
offline, so a runtime webfont would fail exactly when the application cannot
reach the network.

Still ruled out for UI text: Inter and Geist (the decade’s default), Nunito and
Quicksand (generic rounded friendliness), and Fraunces at setting-row size.

**Geometry splits rather than scaling.** Containers stay near upstream’s radius
values; fork-owned marks use full rounding in `marks.css`. Containers are paper,
marks are hand-made.

**Containers lose their borders.** `src/shorthand/ui/Sheet.tsx` replaces
upstream’s `SettingsGroup` in fork sections: same children, no card. The settings
window had roughly forty borders and none separated anything a heading and
hairline did not separate better. A new fork file preserves upstream’s component
and keeps future merges cheap.

## Where it lives

The approved source pack is in `brand-assets/`:

| File                                      | Authority                                                                                                                                                     |
| ----------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `direction.md`                            | the bird/pen story and why each colour exists                                                                                                                 |
| `colours.md`                              | production token values, usage rules and measured WCAG ratios                                                                                                 |
| `FONT.md`                                 | Fraunces, Source Code Pro and the surviving live-type settings                                                                                                |
| `mark-silhouette.svg`                     | source of truth for the one-colour mark                                                                                                                       |
| `mark-full-colour-transparent.png`        | source of truth for the coloured mark `ShorthandWordmark.tsx` renders                                                                                         |
| `wordmark-full-colour.png`                | source of truth for the word and its coral swash; `gen-brand-wordmark.mjs` derives both theme variants from it                                                |
| `wordmark-full-colour-no-stroke.png`      | the same word without the swash — delivered alongside it, currently unused; the shipped lockup wants the swash                                                |
| `logo-full-colour-transparent.png`/`.svg` | the full lockup as delivered, mark and word composited together — reference only; the UI stacks the two separately so each can be sized and themed on its own |

The fork-owned implementation is in `src/shorthand/brand/`:

| File                    | What it is                                                                                                                                       |
| ----------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------ |
| `theme.css`             | Palette, UI/mono/display type tokens, radius and theme selection                                                                                 |
| `marks.css`             | The active-tab indicator and the wordmark's theme selection; all brand selectors                                                                 |
| `ShorthandMark.tsx`     | Four-path silhouette component, filled with `currentColor` — used by the icon generator's reference, not currently placed directly in any screen |
| `ShorthandWordmark.tsx` | Stacked lockup: the full-colour raster mark above the raster word                                                                                |
| `wordmark-light.png`    | Generated: the approved word, resized. Do not hand-edit — re-run the generator                                                                   |
| `wordmark-dark.png`     | Generated: the same word with its ink remapped to cream. Do not hand-edit                                                                        |
| `mark.paths.ts`         | Path data transcribed from the approved silhouette                                                                                               |
| `mark.svg`              | Standalone approved silhouette read by the icon generator                                                                                        |

`theme.css` re-declares tokens upstream and Tailwind already define. It
introduces no utility or component selector; the active-tab indicator stays in
`marks.css` to keep that promise. Upstream can add screens or restyle
components and the fork’s values follow without conflict.

### Where it touches upstream files

- `src/App.css` and `src/overlay/RecordingOverlay.css` import the fork’s theme
  immediately after upstream’s so the fork values win; both also import the
  marks layer.
- `src/overlay/RecordingOverlay.css` reads `--brand-font-sans` for its local
  font variable.
- `src/components/Sidebar.tsx` and `src/components/onboarding/*.tsx` render the
  wordmark.
- `package.json` and `bun.lock` carry the Fontsource dependencies.

Upstream’s `HandyHand.tsx` and `HandyTextLogo.tsx` remain unused but present.
Deleting a file upstream still maintains turns every future upstream edit into
a delete/modify conflict. The same rule leaves `src-tauri/icons/logo.png` in
place: it is upstream’s waving hand, is referenced by nothing, and is absent
from `tauri.conf.json`’s bundle icon list.

## Fork-only strings

The tables above are the fork's visual identity. Its text content — the
Handy → Shorthand rename, and the strings upstream Handy simply doesn't
have — lives in `src/shorthand/branding.ts` and is a separate mechanism,
covered fully by `src/shorthand/locales/README.md`. The short version, so a
reader of this file isn't left assuming visual and text branding are the
same thing:

`branding.ts` walks every locale's translation object at build time
(`src/shorthand/vite-branding-plugin.ts`) and does two jobs in order —
substitution first, fork strings merged on top. Substitution finds the word
"Handy" as a whole word (handling German/Scandinavian genitive "Handys") and
replaces it with "Shorthand"; a `de`-only warning flags any match, because
"Handy" is also the everyday German word for a mobile phone. Fork strings
then merge in on top of the substituted result, which is why a fork string
may contain the literal word "Handy" and mean it — it never passes through
the substitution.

As of 2026-08-26 the strings are in `src/shorthand/locales/*.json` (translatable
fork content) and `src/shorthand/english-copy.json` (English casing rules,
merged into `en` only). `FORK_ONLY_STRINGS` remains exported as the union, for
`check-branding.ts`'s locale-independent question "is this key deliberately
ours?". Merge order is unchanged: substitution first, fork strings on top.

The same 2026-08-26 plan also found, and fixed, 32 fork-only keys that had
been written directly into `src/i18n/locales/` instead of through this
mechanism — the exact thing this file's "never edit the locale files" rule
exists to prevent. `bun run check:locale-drift` now fails the build if that
happens again.

## Seeing it

`brand-preview/` is a committed harness that renders real UI primitives against
the brand layer without Tauri and screenshots both themes. The palette and
stacked lockup were checked there.

```bash
bun x vite dev --port 5199     # port 1420 is often taken; strictPort is on
node brand-preview/shot.mjs    # node, not bun — Playwright hangs under bun here
```

Two traps: flip `data-theme` and wait about 500ms before shooting, or
`transition-colors` is caught mid-tween; and do not gitignore the preview,
because Tailwind v4 skips gitignored files when scanning for class names.

## Regenerating the artwork

The mark itself is not generated. Change the source pack, then re-transcribe
`mark.svg` and `mark.paths.ts` from `brand-assets/mark-silhouette.svg`.

```bash
node scripts/gen-brand-icons.mjs     # node, not bun — app icon + every tray state
node scripts/gen-brand-wordmark.mjs  # node, not bun — both wordmark variants
cd src-tauri && bun x tauri icon     # slices app-icon.png to every platform
```

The icon generator reads every path and its fill rule from `mark.svg`. Both
scripts rasterise through Playwright’s Chromium, which is already a
devDependency — deliberately avoiding a native image toolchain in a fork whose
`package.json` has to stay mergeable.

Re-run `gen-brand-wordmark.mjs` whenever `brand-assets/wordmark-full-colour.png`
is re-rendered. It re-measures the artwork’s own ink range each run rather than
assuming fixed values, so a re-render maps correctly without anyone editing
constants — but it will fail loudly if the ink stops being blue-dominant, since
the dark variant would silently come out identical to the light one. If the word
is re-rendered at a different size, the ratios in `ShorthandWordmark.tsx` need
re-measuring too; they are noted there with the numbers they came from.

Every tray state draws the mark **84 units wide in a 64-unit frame,
left-aligned, bleeding off the right edge.** That is deliberate, and it is the
only way to make the tray icon bigger.

The mark is 1.4:1 landscape, and its bounds are tight — `MARK_BOUNDS` was
verified against the rendered paths, so there is no padding to reclaim. Fitted
whole inside a square tray slot it can only fill 69% of the height, and the
leftover sits above and below as empty frame, which is exactly what made it
read small beside neighbouring tray icons. Cropping to the bird alone is worse,
not better: with its wing and tail flourishes the bird measures 1.61:1.

So the mark is scaled until its height nearly fills the frame and the overflow
is spent off a single edge. Left-aligned rather than centred because the nib,
head and eye are what identify the mark and all sit at the left; the cut then
lands on the wing and tail, which read as continuing past the frame rather than
as damage. 84 units (91% of frame height) is the ceiling — past roughly there
the wing's feathers slice into a flat vertical edge that stops reading as a
bleed.

This is tuned for 16–24px, where a tray icon actually lives (32px at 200% DPI
is the ceiling). Inspected large, the flat cut edge is visible; nothing renders
it that way. The installed app icon is unaffected — `appIcon()` still centres
the whole mark inside its tile.

The mark is line art: the bird and the pen are each an `evenodd` shape whose
inner subpath leaves a hollow interior. **State is carried by filling those
interiors, not by changing the outline.** The line art itself stays paper or
ink in every state, so the tray item never stops looking like Shorthand:

| State        | Fill                                                    | Why                                                                                                                            |
| ------------ | ------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------ |
| Idle         | none — open line art                                    | the app, at rest                                                                                                               |
| Recording    | bird body, `--brand-highlighter` coral, theme-invariant | coral already means “happening now” everywhere else                                                                            |
| Transcribing | bird body, `--color-logo-primary` ocean blue, flipped   | the app’s conventional “working” accent                                                                                        |
| Warning      | bird body **and pen**, `--color-warning` amber, flipped | a second filled region reads as different-in-kind before the hue even registers, and amber is the one hue reserved for warning |

Two rejected versions are worth not re-proposing. The first shrank the mark and
top-aligned it to leave a strip for a status badge in a learned bottom-right
slot — costing roughly a third of the mark's visible size for a signal a colour
carries at a glance. The second recoloured the entire silhouette, pen included;
that fixed the size, but a fully coral bird-and-pen reads as a _different logo_
rather than the same one in a different state. Filling only an interior keeps
the outline constant, which is what makes the state legible as a state.

The warning's exclamation-on-a-disc badge went with the first version. Filling
the pen replaces it: it distinguishes warning from the one-region ambient
states without spending any of the mark's size, and unlike a badge it survives
16px without a dedicated antialiasing gutter.

Upstream solves the same problem by swapping the glyph itself
(hand → ear → brain). That path was considered and rejected: Shorthand has one
glyph, and spending it on status would cost the identity a colour can carry
just as well.

The macOS menu bar uses template mode for icons whose colour is meant to be
discarded and replaced by the OS. Only plain idle qualifies now — it has no
second colour to lose — so it alone renders templated and blends in like any
other menu bar glyph. Every other state tints an interior and is rendered
non-templated (see `tray.rs::change_tray_icon`), which is what makes the fill
visible at all; template mode would otherwise flatten each one back to the
same alpha-only silhouette as idle. The coloured Linux tray theme uses
`#2E6F9E` line art — the pack’s uncontrolled-background fallback — with the
same interior fills as everywhere else.
