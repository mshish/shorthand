---
category: Brand
---

ShorthandWordmark — the full product lockup: the clay bird-and-pen artwork
stacked above the word "Shorthand" and its coral swash.

```jsx
<ShorthandWordmark height={44} />
```

## `height` is the cap height, not the image height

This is the prop people get wrong. `height` is the cap height of the **word** in
px; the mark above it and the swash below both scale from that. The rendered
element is roughly 3.25× taller and 4.9× wider than the number you pass,
because the artwork carries the swash and its surrounding air.

So: size it by how big the word should read, then leave room around it. Do not
try to fit it to a container height.

Useful values: `20`–`22` in a page header, `44` for a hero, `64` where the
lockup is the whole composition.

## It is artwork, not type

Both halves are the real clay render — not traced, not re-set in a typeface.
Consequences worth respecting:

- Its ink does not follow `--color-text`. Two variants ship (navy for paper,
  cream for night) and **CSS** picks one from `:root[data-theme]` or
  `prefers-color-scheme`. Nothing you put on a wrapper changes it, so do not
  place the wordmark on a dark panel inside a light page — it will render navy
  on dark. Use `ShorthandMark` there instead, which does follow its ground.
- Do not recolour, rotate, outline, or add effects to it.
- The word stays left-to-right in every locale.

`alt` carries "Shorthand" on whichever variant is visible, and the hidden one is
removed from the accessibility tree, so the name is announced exactly once. It
does not need a caption repeating the product name.
