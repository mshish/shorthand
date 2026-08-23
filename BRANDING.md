# Shorthand branding

How the fork looks different from upstream Handy, where each decision lives, and
how to regenerate the artwork.

## The direction

Handy is pink on warm grey, with a waving hand for a logo. Shorthand is **a
marked-up transcript**.

A transcript is plain until someone marks it. Almost the whole UI is paper and
ink; colour appears only on the thing that is currently live. Playful because
the mark is a surprise against an otherwise quiet page; useful because colour is
never decorative — it always means "this one".

The page has exactly two colours, because a marked-up page has two: the
blue-black ink the words are written in, and the yellow highlighter someone
swept over the part that mattered. They sit opposite each other on the wheel,
which is why highlighters are yellow and ink is blue in life — the sweep pops
against the ink instead of competing with it.

| Role                    | Light     | Dark      | Named for               |
| ----------------------- | --------- | --------- | ----------------------- |
| `--color-background`    | `#faf8f2` | `#12141a` | paper                   |
| `--color-text`          | `#12151f` | `#eceef4` | ink                     |
| `--color-logo-primary`  | `#12459e` | `#6aa9f5` | ink at writing strength |
| `--color-background-ui` | `#1e5bd6` | (same)    | ink at full strength    |
| `--brand-highlighter`   | `#e8f35c` | `#d9e84a` | highlighter (fork-only) |
| `--color-mid-gray`      | `#66697a` | `#969aab` | pencil                  |

This is the **third** accent. Teal shipped first and was rejected as too common.
Copying-pencil violet replaced it, and was rejected with the whole direction: it
said clerical and archival, which is what the pivot was moving away from.

Blue had itself been ruled out earlier as the default accent of most software
written this decade. That objection was to blue _alone_, and it is right — a lone
blue accent is generic. Blue ink under a yellow highlighter is not a palette
choice at all; it is a description of a page. The pairing is what stops it
reading as another SaaS blue.

Still ruled out: green (reads success), amber and orange (spoken for by
`--color-warning`), pink (upstream's), violet (the direction replaced).

## Two tokens, because there are two jobs

This is the part most likely to be broken by a well-meaning change.

`--color-logo-primary` is used **96 times across 34 files**, in four
incompatible roles: a light tint behind dark text (`/20`–`/30`), a solid fill
under white text, a foreground colour (`text-logo-primary`), and a focus ring.

- **It must be theme-flipping.** The foreground and focus-ring roles need
  ≥4.5:1 and ≥3:1 against the background, so it has to be dark on paper and
  light on night. `src/styles/theme.css` already resolves it from a
  `--light-` / `--dark-` pair.
- **The old rule that it "must stay a light tint in both themes" was wrong.** At
  20–30% over paper every hue blends to a pale wash, so the tint role constrains
  nothing. Only the _solid_ usages bind.

`--brand-highlighter` is new and fork-only, and exists so the sweep does not have
to be `--color-logo-primary`. "Colour appears only on the live thing" is
unachievable by substituting a token that 34 files already use for spinners,
progress bars and badges. Giving the mark its own token and quieting the general
accent is what makes the idea deliverable.

What that yields is an **accent hierarchy**, not a strict semantic rule:

- the **highlighter** means _live_, and marks nothing else;
- the **ink accent** means _set_ — a checked toggle, a primary button, a
  selected item. Conventional, and left conventional.

## Contrast, and the window that picked the blue

`--color-background-ui` has to carry white text _and_ separate from both grounds
as a non-text fill. Those pull opposite ways and leave a narrow luminance
window: at least 0.119 to clear 3:1 against the dark ground, at most 0.183 to
keep white at 4.5:1.

A deep navy cannot reach it at all. Sapphire `#0f52ba` has luminance 0.097, so
it can never hit 3:1 against _any_ dark background, however dark you make it.
Registrar `#12459e`, quink `#1749c0` and cobalt `#2050c8` all fail the same way
at 2.1–2.7:1. `#1e5bd6` sits inside the window. So "vibrant, not too bright" was
not a preference bent around the maths — it is the only band that works.

`--color-mid-gray` cannot be a single value. Clearing 4.5:1 against `#faf8f2`
needs luminance ≤0.17; clearing it against `#12141a` needs ≥0.203. There is no
overlap — an exhaustive search of all 256 greys finds none, the best worst-case
being `#777777` at 4.17:1. Upstream declares it once, so `brand/theme.css` adds
theme-selection blocks mirroring upstream's own.

Three upstream sites put white on `bg-logo-primary` and `Badge`'s `primary`
variant sets a background with no foreground. Both fail today and fail worse
against a dark accent; both are fixed with one-word edits.

## The mark

A lowercase "s" written with a pointed pen — thinning to nothing at the entry,
the waist and the exit. In the wordmark it stands in for the initial S rather
than sitting beside the word: a logo bug next to "Shorthand" would put two S's in
the lockup and say nothing, while substituting the written stroke for the
typeset letter says the product's whole idea in one move.

SVG cannot vary `stroke-width` along a path, so the visible shape has to be an
outline — the spine offset to both sides by a width profile. That is ~200
coordinates that would need re-deriving by hand every time the curve moved, so it
is generated instead.

The mark survived the pivot unchanged. It fills with `currentColor`, so only its
colour moved.

## The sweep, and the rules learned by looking

The highlighter sweep is the one motif. Three rules govern it, and all three were
found by building it and screenshotting both themes — none would have survived
review alone. They are documented in `src/shorthand/brand/marks.css` beside the
code they explain.

1. **No `mix-blend-mode: multiply`.** A highlighter is translucent, so multiply
   looks correct — and on a dark ground it is correct and fatal, because a
   highlighter over black paper deposits nothing. `#d9e84a` multiplied by
   `#12141a` resolves to about `#0f1208`, which put the dark label ink at
   ~1.05:1 against its own mark. Every dark-theme mark became a black smudge.
2. **It marks text, never a container.** On a 40px row the rotation is
   imperceptible, the gradient invisible, the radii merely "rounded" — the exact
   flat active-state fill the direction exists to avoid, with yellow substituted
   for blue.
3. **The text must be long enough.** What governs is the rendered aspect ratio:
   running text at 10.4:1 works, a tab label at 5.2:1 works, `AI cleanup` at
   3.0:1 is weak, `Modes` at 1.9:1 is a chip, `App` at 1.2:1 is a square. Below
   about 5:1 the corner radii eat the perimeter, no straight section survives,
   and the pen line detaches along the whole bottom edge — a badge with an
   underline, not a stroke.

So the sweep marks **running text and the active tab**. The sidebar marks its
selection with an accent icon and a full-weight label against dimmed neighbours,
which is quieter — right for a rail that is permanently on screen — and protects
the rule: colour means _live_, not merely _selected_.

The pen line under the sweep is not styling. `#e8f35c` against `#faf8f2` is
1.14:1; a saturated yellow on white is plainly visible to an eye, but WCAG 2.x
measures luminance alone and cannot see hue, so the sweep can never satisfy the
3:1 required of a non-text indicator. The hairline is in the accent (8.34:1
light, 7.54:1 dark) and supplies the boundary. The constraint improved the
design: a highlighter stroke with a pen line under it is what a marked-up page
actually looks like.

## Type and geometry

**Atkinson Hyperlegible Next** for the UI, **Atkinson Hyperlegible Mono** for
paths, shortcuts and logs, self-hosted through `@fontsource`. Drawn by the
Braille Institute so characters cannot be confused with one another — its whole
thesis is legibility of the written record, which is what this app produces. It
carries warmth and quirk without being a novelty face, which matters because a UI
this achromatic cannot carry personality in colour.

Self-hosting is not optional: the app works offline, and a webfont fetched at
runtime would leave the UI in a fallback face exactly when it can't reach the
network.

Ruled out: Inter and Geist (the decade's default), Nunito and Quicksand (rounded
and friendly is generic playful), Fraunces (a serif at 13px row labels is a
legibility bet a transcription app should not take).

**Geometry splits rather than scaling.** The previous direction halved the radius
scale for crisp ledger corners. "Containers are paper, marks are hand-made" wants
the opposite: containers sit near upstream's values, and the fork's own marks use
full rounding, applied in `marks.css`.

**Containers lose their borders.** `src/shorthand/ui/Sheet.tsx` replaces
upstream's `SettingsGroup` in fork sections: same children, no card. The settings
window carries roughly forty of those borders and none separates anything that a
heading and a hairline do not separate better. It is a new file rather than an
edit so upstream's own screens keep their component and a restyle upstream still
merges cleanly.

## Where it lives

Everything fork-specific is under `src/shorthand/brand/`:

| File                    | What it is                                                        |
| ----------------------- | ----------------------------------------------------------------- |
| `theme.css`             | Palette, type, radius. Token values, plus theme-selection blocks. |
| `marks.css`             | The sweep, its animation, the reduced-motion rule. All selectors. |
| `ShorthandMark.tsx`     | The mark, filled with `currentColor`.                             |
| `ShorthandWordmark.tsx` | The lockup, set in the app's own type.                            |
| `mark.generated.ts`     | Generated path data. Do not edit.                                 |
| `mark.svg`              | The same path as a standalone file, for the icon generator.       |

`theme.css` re-declares tokens upstream and Tailwind already define. It
introduces no utility or component selector — the sweep lives in `marks.css`
precisely to keep that true. That is what keeps a merge from upstream cheap: they
can add settings screens, rework the sidebar or restyle a button, and the fork's
identity follows without a conflict.

### Where it touches upstream files

- `src/App.css` and `src/overlay/RecordingOverlay.css` — two `@import`s each,
  immediately after `styles/theme.css` so the fork's values win. `marks.css` goes
  into both: the sweep marks recording state, and recording is drawn in the
  overlay.
- `src/overlay/RecordingOverlay.css` — `--s-font` reads `--brand-font-sans`,
  keeping its old stack as the fallback.
- `src/components/Sidebar.tsx` — the wordmark and the selected-row treatment.
- `src/components/onboarding/*.tsx` — the wordmark.
- `package.json` / `bun.lock` — the `@fontsource` dependencies.

Upstream's `src/components/icons/HandyHand.tsx` and `HandyTextLogo.tsx` are left
in place unused. Deleting a file upstream still maintains turns every future edit
to it into a delete/modify conflict, which is the expensive kind.

## Seeing it

`brand-preview/` is a committed harness that renders the real `ui/` primitives
against the brand layer without needing Tauri, and screenshots both themes. Every
visual decision above was made by looking at its output.

```bash
bun x vite dev --port 5199     # port 1420 is often taken; strictPort is on
node brand-preview/shot.mjs    # node, not bun — Playwright hangs under bun here
```

Two traps: flip `data-theme` and wait ~500ms before shooting, or you catch
`transition-colors` mid-tween and see contrast bugs that do not exist; and do not
gitignore the directory, because Tailwind v4 skips gitignored files when scanning
for class names.

## Regenerating the artwork

```bash
bun scripts/gen-brand-mark.ts        # only after editing the spine or width profile
node scripts/gen-brand-icons.mjs     # node, not bun
cd src-tauri && bun x tauri icon     # slices app-icon.png to every platform
```

Tray states are the mark plus a badge saying what the app is doing, always in the
same corner, so the eye learns one slot and only has to read what is in it. The
mark itself never changes — it is the only thing identifying which app the tray
item belongs to.

| State        | Badge                | Why                                        |
| ------------ | -------------------- | ------------------------------------------ |
| Idle         | none                 | the app, at rest                           |
| Recording    | solid dot            | the record symbol, unchanged since tape    |
| Transcribing | ring with a gap      | the shape every spinner uses for "working" |
| Warning      | exclamation on a dot | upstream's own convention, kept            |
