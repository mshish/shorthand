# Typeface and live lockup

## Primary: Fraunces

Use **Fraunces Variable**, a soft, expressive old-style serif whose rounded terminals and adjustable softness echo the approved retro clay wordmark without requiring the wordmark to remain an image.

- Project and font sources: <https://github.com/undercasetype/Fraunces>
- Licence: [SIL Open Font License 1.1](https://github.com/undercasetype/Fraunces/blob/master/OFL.txt)
- Offline/self-hosting: **Yes.** Bundle the variable `.ttf` with the desktop application and a subsetted `.woff2` with any local web UI. No runtime request to Google Fonts or another CDN is permitted.
- Recommended axes: weight `650`; optical size `72`; `SOFT` `75`; `WONK` `1`.
- Fallback stack: `Fraunces, Georgia, "Times New Roman", serif`.

## Live `[mark]horthand` lockup

Let **H** be the measured cap height of live Fraunces at the chosen size.

| Property                       |                  Specification |
| ------------------------------ | -----------------------------: |
| Mark height                    |                        `1.12H` |
| Mark-to-`h` horizontal kerning |     `-0.08H` (optical overlap) |
| Mark baseline nudge            |              `+0.04H` downward |
| Letter-spacing for `horthand`  |                     `-0.015em` |
| Font weight                    |                          `650` |
| Font axes                      | `opsz 72`, `SOFT 75`, `WONK 1` |

Use `mark-silhouette.svg` as the initial S and set only `horthand` in live type. Scale and align from rendered cap height, not from the CSS line box. At very small UI sizes (below 18px cap height), reduce the mark to `1.06H`, set `SOFT` to `60`, and remove the negative kerning to prevent the pen/S junction from crowding the `h`.

## Monospace companion

Fraunces has no native monospace companion. Use **Source Code Pro Variable** for transcripts and time-aligned text: it is calm, highly legible, self-hostable, available in variable builds, and licensed under the SIL Open Font License 1.1.

- Project: <https://github.com/adobe-fonts/source-code-pro>
- Licence: <https://github.com/adobe-fonts/source-code-pro/blob/release/LICENSE.md>
- Recommended transcript settings: weight `450`, line-height `1.5`, letter-spacing `0`, tabular numerals enabled.
