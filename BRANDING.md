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

The mark is the approved one-colour outline of a bird perched on a fountain pen.
The detailed clay artwork tells the story at large sizes; the silhouette keeps
the bird-and-pen contour when detail disappears. It has four paths, and two use
`fill-rule="evenodd"` to punch counters through the bird’s body and pen barrel.
It fills with `currentColor`, so one component can inherit ink in the UI and be
rasterised into each tray treatment without owning a themed colour.

The previous mark was a lowercase “s” written with a pointed pen. SVG cannot
vary `stroke-width` along a path, so its visible stroke had to be an outline: a
spine offset on both sides by a width profile, producing roughly 200
machine-derived coordinates. That was why `scripts/gen-brand-mark.ts` existed.
The reason disappeared with the old mark, so the generator disappeared too.

The new mark is approved artwork, not editable geometry. `mark.paths.ts` and
`mark.svg` are transcribed from `brand-assets/mark-silhouette.svg`. If the
artwork changes, re-transcribe it; do not hand-tune either copy into a fork of
the source of truth.

The old `[mark]horthand` lockup substituted the written “s” for the initial S.
Its argument was coherent — a separate bug beside “Shorthand” would print two
S’s — but the approved artwork settled the question differently. The shipped
lockup stacks the bird and pen above the complete word **Shorthand**, with a
coral sweep beneath, matching the composition of the raster.

The word remains live type rather than outlines, so it stays crisp at any size,
follows the theme’s ink, and never needs re-exporting. `brand-assets/FONT.md`’s
`[mark]horthand` table now governs only the type decisions that survived:
weight 650, `opsz 72`, `SOFT 75`, `WONK 1`, and `-0.015em` tracking. Its old
mark-height, kerning and baseline-nudge measurements do not apply to the stacked
lockup.

## The sweep, and the rules learned by looking

The coral sweep is the motif carried forward from “a marked-up transcript”. Its
four rules were found by building it and screenshotting both themes; none is a
decorative preference.

1. **No `mix-blend-mode: multiply`.** A physical highlighter is translucent,
   but multiply over a dark ground deposits almost nothing. In the then-yellow
   experiment, `#D9E84A` multiplied by `#12141A` became about `#0F1208` and put
   the label at roughly 1.05:1 against its own mark. The dark-theme sweep became
   a black smudge.
2. **Mark text, never a container.** On a 40px row, rotation disappears,
   gradient unevenness disappears, and asymmetric radii become merely rounded.
   The result is the same flat rectangular active fill the motif exists to
   avoid, only coral.
3. **The text must be long enough.** Rendered aspect ratio governs the result:
   running text at 10.4:1 works; a tab at 5.2:1 works; `AI cleanup` at 3.0:1 is
   weak; `Modes` at 1.9:1 is a chip; `App` at 1.2:1 is a square. Below roughly
   5:1 the radii consume the perimeter, no straight section survives, and the
   pen line detaches into a second object.
4. **Overshoot is lopsided, about 3:1 horizontal to vertical.** Growing both
   axes makes the mark roomier but reduces its aspect ratio. Height is the
   denominator; spend the overshoot on width. A hand also overshoots the first
   and last letter, not the line height.

Running text and active tabs are eligible; the current implementation uses the
sweep on the active tab. The sidebar instead uses an accent icon and full-weight
label against dimmed neighbours. That is quieter for a permanent rail and
protects the distinction between _live_ and merely _selected_.

The pen hairline below the sweep is not styling. Coral against light paper is
2.83:1, short of the 3:1 non-text floor, so the hairline remains load-bearing in
light mode. Coral against the dark ground is 5.80:1 and clears the floor on its
own. The line stays in both themes because one motif that changes construction
with the theme is worse than one that is belt-and-braces on one side.

The line sits below the sweep with about a pixel of daylight. Dark-theme accent
on coral is only 1.36:1, so tucking the line under the sweep’s edge would make it
disappear into the mark. Separated, it is measured against the page, where the
accent holds 7.89:1. It also looks more like a highlighter stroke with a pen line
under it than one stroke with a darker edge.

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

| File                  | Authority                                                      |
| --------------------- | -------------------------------------------------------------- |
| `direction.md`        | the bird/pen story and why each colour exists                  |
| `colours.md`          | production token values, usage rules and measured WCAG ratios  |
| `FONT.md`             | Fraunces, Source Code Pro and the surviving live-type settings |
| `mark-silhouette.svg` | source of truth for the one-colour mark                        |

The fork-owned implementation is in `src/shorthand/brand/`:

| File                    | What it is                                                            |
| ----------------------- | --------------------------------------------------------------------- |
| `theme.css`             | Palette, UI/mono/display type tokens, radius and theme selection      |
| `marks.css`             | The sweep, its animation and reduced-motion rule; all brand selectors |
| `ShorthandMark.tsx`     | Four-path mark component, filled with `currentColor`                  |
| `ShorthandWordmark.tsx` | Stacked live-type lockup                                              |
| `mark.paths.ts`         | Path data transcribed from the approved silhouette                    |
| `mark.svg`              | Standalone approved silhouette read by the icon generator             |

`theme.css` re-declares tokens upstream and Tailwind already define. It
introduces no utility or component selector; the sweep stays in `marks.css` to
keep that promise. Upstream can add screens or restyle components and the fork’s
values follow without conflict.

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
node scripts/gen-brand-icons.mjs     # node, not bun
cd src-tauri && bun x tauri icon     # slices app-icon.png to every platform
```

The icon generator reads every path and its fill rule from `mark.svg`. It
rasterises the 1024px app-icon master and tray PNGs through Playwright’s Chromium
without adding a native image dependency.

Tray states keep one mark and add one badge in a learned bottom-right slot. The
landscape mark in a square frame leaves a strip rather than an empty corner, so
it takes 62 of 64 units of width, top-aligned, and the badge sits in the strip
below. Width is protected because the silhouette reads along its length and
16px in a menu bar is its primary home. Idle and badged states use identical
mark placement; the previous generator claimed that rule while quietly scaling
them differently.

| State        | Badge                 | Why                                        |
| ------------ | --------------------- | ------------------------------------------ |
| Idle         | none                  | the app, at rest                           |
| Recording    | solid dot             | the record symbol, unchanged since tape    |
| Transcribing | ring with a gap       | the shape every spinner uses for “working” |
| Warning      | exclamation on a disc | upstream’s convention, kept                |

The macOS menu bar uses template mode, so tray art is alpha-only. Every tray SVG
paints one requested colour; its black/white badge mask changes alpha rather than
introducing a second visible tone. The coloured tray theme uses `#2E6F9E`, the
pack’s uncontrolled-background fallback.
