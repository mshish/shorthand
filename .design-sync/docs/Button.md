---
category: Actions
---

Button — the only interactive primitive in this design system, and the carrier of
its two most load-bearing colour roles.

```jsx
<Button size="lg">Download for macOS</Button>
<Button size="lg" variant="ghost">See how it works</Button>
```

## Variants

The axis is **semantic, not decorative**. Pick by what the action means.

| Variant | Fill | Use for |
| --- | --- | --- |
| `primary` | `--color-background-ui` solid, white label | The one action a section is asking for. One per view. |
| `primary-soft` | `logo-primary/20` tint, body-colour label | A second affirmative action that must not compete with `primary`. |
| `secondary` | `mid-gray/10` with a hairline border | Neutral, the default for anything ordinary. |
| `ghost` | Transparent, inherits `currentColor` | Sits beside a `primary` as its quiet companion; also toolbar actions. |
| `warning` | Neutral at rest, amber on hover/focus | Only on a warning surface. Borrows `--color-warning`, deliberately not the brand accent, so it does not read as brand emphasis. |
| `danger` | Solid red, white label | Destructive and confirmed. |
| `danger-ghost` | Red label, transparent | Destructive but recoverable, or repeated in a list. |

Two `primary` buttons side by side is the failure this component's variant set
exists to prevent — pair `primary` with `ghost` instead.

## Sizes

`sm` / `md` (default) / `lg`. Only padding and type scale change. `lg` is the
marketing-page size; `md` is the in-app default.

## Notes

- `disabled` drops the whole button to 50% opacity and blocks the cursor. It is
  one rule shared by every variant, so `ghost` disabled is very faint — that is
  correct, not a broken render.
- Every remaining `<button>` attribute passes straight through, `onClick` and
  `type` included.
- `className` is appended last, so a utility class beats the variant's own.
