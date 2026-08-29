# Shorthand colour tokens

These are production UI tokens derived from the approved clay artwork. The blue is sampled toward the artwork's midtones; the page background is deliberately more neutral than the rendered peach paper so it can carry long-form UI; the highlighter uses the approved darker coral `#F3673C`.

## Tokens

| Token             | Role                                        | Light theme | Dark theme |
| ----------------- | ------------------------------------------- | ----------: | ---------: |
| `text`            | Body ink                                    |   `#14202B` |  `#F6F1E8` |
| `background`      | Page                                        |   `#FAF5EA` |  `#111820` |
| `accent`          | Foreground text, icon fill, focus ring      |   `#0B5F8A` |  `#63B7D6` |
| `accent-stroke`   | Partner edge/stroke for `accent`            |   `#084A6C` |  `#92D4E7` |
| `background-ui`   | Primary button fill carrying white text     |   `#2E6F9E` |  `#2E6F9E` |
| `mid-gray`        | Secondary text                              |   `#5C6770` |  `#AAB4BE` |
| `highlighter`     | Background behind the single live-now state |   `#F3673C` |  `#F3673C` |
| `highlighter-ink` | Text placed on `highlighter`                |   `#14202B` |  `#111820` |

## Measured WCAG contrast

Ratios use WCAG 2.x relative luminance and unrounded sRGB values.

| Required pairing                           |   Light |    Dark |          Requirement | Result |
| ------------------------------------------ | ------: | ------: | -------------------: | ------ |
| `text` on its own `background`             | 15.19:1 | 15.88:1 |                  7:1 | Pass   |
| `accent` on its own `background`           |  6.40:1 |  7.89:1 | 4.5:1 text; 3:1 ring | Pass   |
| White on `background-ui`                   |  5.41:1 |  5.41:1 |                4.5:1 | Pass   |
| `background-ui` against light `background` |  4.97:1 |  4.97:1 |                  3:1 | Pass   |
| `background-ui` against dark `background`  |  3.30:1 |  3.30:1 |                  3:1 | Pass   |
| `mid-gray` on its own `background`         |  5.32:1 |  8.49:1 |                4.5:1 | Pass   |
| `highlighter-ink` on `highlighter`         |  5.36:1 |  5.80:1 |                4.5:1 | Pass   |

## Tray fallback

Use `#2E6F9E` when the tray's coloured theme must supply one uncontrolled colour. It measures **5.41:1 on white** and **3.88:1 on black**, so it clears the 3:1 non-text floor on both.

## Usage notes

- `highlighter` is a background only. Do not use it as body text, and do not place white text on it.
- Keep `accent-stroke` paired with its theme's `accent`; it is not a replacement for body ink.
- The approved raster contains lighting and material variation. These tokens intentionally describe stable UI roles rather than every highlight and shadow in the clay rendering.
