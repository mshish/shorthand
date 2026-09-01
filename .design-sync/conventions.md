# Shorthand — building with this design system

Shorthand is an offline speech-to-text desktop app. Its identity is a clay bird
carrying a fountain pen: a fleeting voice, committed to the page. Ink blue is
the written-confidence colour, coral is the one thing happening *now*, and the
ground is warm paper rather than white — the interface should feel written-on,
not displayed-on.

This system is **three components and a strong token layer**. Most of what you
build will be your own markup styled with the vocabulary below; the components
are the brand marks and one button.

## Setup: no provider, one stylesheet

There is no context provider and no theme wrapper. Import the stylesheet and the
components work:

```jsx
// _ds/<folder>/styles.css — imports fonts/fonts.css and _ds_bundle.css
<ShorthandWordmark height={44} />
```

Light and dark are resolved by CSS from `prefers-color-scheme`, or forced by
setting `data-theme="light"` / `data-theme="dark"` on `<html>`. Nothing
subscribes to a store; there is no theme prop anywhere.

## The styling idiom: Tailwind utilities over brand tokens

Style with **Tailwind v4 utility classes**. The brand palette is bridged into
Tailwind, so `bg-`, `text-`, `border-` and `ring-` take these token names —
and only these. There is no `bg-blue-600` in this system's vocabulary; reaching
for a stock Tailwind palette colour is how a page stops looking like Shorthand.

| Class root | Token | What it is |
| --- | --- | --- |
| `background` | `--color-background` | The paper. `#faf5ea` light, `#111820` night. Page and section grounds. |
| `text` | `--color-text` | Body ink. Blue-black `#14202b` / cream `#f6f1e8`. 15.19:1 on paper. |
| `mid-gray` | `--color-mid-gray` | Secondary text. Theme-flipping, and cleared for 4.5:1 in both. |
| `logo-primary` | `--color-logo-primary` | Ocean-blue accent. `#0b5f8a` light, `#63b7d6` night. Foreground, focus ring, and tints (`/20`, `/30`). |
| `logo-stroke` | `--color-logo-stroke` | The accent's partner edge. Pair it with `logo-primary` from the same theme; never use it as body ink. |
| `background-ui` | `--color-background-ui` | Ink at full strength, `#2e6f9e`, theme-invariant. The primary action fill. **Carries white text only.** |
| `highlighter` | `--brand-highlighter` | Coral `#f3673c`. Means "happening now" — never a warning, never a success. |
| `highlighter-ink` | `--brand-highlighter-ink` | The only text colour permitted on coral. |
| `warning` / `error` | `--color-warning` / `--color-error` | Semantic status. Not brand emphasis. |

Opacity modifiers work as usual (`bg-logo-primary/20`, `border-mid-gray/20`) and
are how this system builds tints and hairlines.

**Coral is a background only.** Never body text, never a fill under white text.
It marks the live thing and loses all its meaning the moment it decorates
something merely important. On a page, that usually means at most one coral
element per view.

## Type

Three faces, all self-hosted — no network fetch, and each has a real fallback
stack.

- `font-sans` — **Atkinson Hyperlegible Next**. Body and UI. Letterforms built to
  resist confusion; this is the default and most text should stay in it.
- `font-display` — **Fraunces**, and it already carries the brand's exact point
  in the variable space (`opsz 72, SOFT 75, WONK 1`, weight 650). Display only:
  headlines and the product lockup. Its soft, wonky old-style forms are what
  echo the clay artwork. Do not use it below about `text-2xl` — the wonk needs
  size to read as character rather than as a mistake.
- `font-mono` — **Source Code Pro**. Transcripts, time-aligned text, keyboard
  shortcuts, file paths.

Radii are `rounded-sm|md|lg|xl` (0.25 → 0.75rem) for containers. `rounded-full`
is reserved for the fork's own marks — pills, chips, the active indicator.

The layout utilities you would expect are all compiled in: flex and grid,
`grid-cols-1..12`, spacing 0–16 plus 20/24/28/32/40/48/56/64, the full type
scale to `text-9xl`, `max-w-*`, borders, shadows, opacity, transitions, and the
`sm:` `md:` `lg:` `xl:` `hover:` `focus:` variants over them.

## Components

- **`Button`** — seven semantic variants, three sizes. Pair one `primary` with a
  `ghost`, never two primaries. See its doc for the variant table.
- **`ShorthandMark`** — the inline-SVG bird-and-pen silhouette. Fills with
  `currentColor`, so it takes the colour of whatever it sits on. Use it wherever
  the artwork would be under ~100px wide.
- **`ShorthandWordmark`** — the full clay lockup. `height` is the **cap height of
  the word**, and the element renders roughly 3× taller than that. Its ink is
  chosen by CSS, so it cannot sit on a dark panel inside a light page.

## Where the truth lives

- `_ds/<folder>/styles.css` and the `_ds_bundle.css` it imports — every token
  definition and every compiled utility, with the reasoning kept in comments.
- `guidelines/BRANDING.md` and `guidelines/BRAND_BRIEF.md` — why the palette is
  what it is, including the contrast contract each token has to satisfy.
- `components/<group>/<Name>/<Name>.prompt.md` — per-component usage.

## One idiomatic section

```jsx
<section className="bg-background px-6 py-24">
  <div className="mx-auto max-w-3xl text-center">
    <ShorthandMark size={72} className="mx-auto text-logo-primary" />
    <h2 className="mt-8 font-display text-5xl leading-tight text-text">
      Say it once.
    </h2>
    <p className="mx-auto mt-4 max-w-xl text-lg leading-relaxed text-mid-gray">
      Shorthand listens while you talk and hands back the written thing —
      offline, on your own machine.
    </p>
    <div className="mt-10 flex flex-wrap justify-center gap-3">
      <Button size="lg">Download for macOS</Button>
      <Button size="lg" variant="ghost">See how it works</Button>
    </div>
  </div>
</section>
```
