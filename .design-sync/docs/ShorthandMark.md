---
category: Brand
---

ShorthandMark — the bird perched on a fountain pen, reduced to the approved
one-colour silhouette. An inline SVG, so it scales without softening.

```jsx
<ShorthandMark size={96} className="text-logo-primary" />
```

## The one thing to know

It fills with `currentColor` and has no colour of its own. Set the colour on the
mark or on any ancestor — `text-logo-primary`, `text-white` on an ink panel,
`text-highlighter-ink` on coral. This is what lets one component serve as the
tray icon, the sidebar glyph and a hero graphic without a themed variant of
each.

Never hard-code a fill on it. A mark that cannot follow its ground is a mark
that will be wrong on half the sections of a page.

## Sizing

`size` sets both dimensions; `width` and `height` override individually. The
viewBox is `0 0 128 128`.

It is drawn to survive reduction — the bird and the pen stay separable down to
16px, which is why the silhouette exists at all. Use it, not the wordmark,
wherever the artwork would be smaller than about 100px wide.

## Which brand component to reach for

- **Page or section decoration, favicon, small glyph, anything that must take a
  colour from its surroundings** → `ShorthandMark`.
- **Naming the product — a header, a hero, an About panel** → `ShorthandWordmark`,
  which stacks this mark's full-colour clay artwork above the word itself.

`aria-hidden` is set by default: the mark is decoration, and the accessible name
belongs to the wordmark or to nearby text. Pass `aria-hidden={false}` with a
`role`/`aria-label` only if it is genuinely the sole carrier of meaning.
