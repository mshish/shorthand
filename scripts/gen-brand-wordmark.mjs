/**
 * Fork-only. Derives the two wordmark assets the UI actually loads from the
 * approved artwork in `brand-assets/wordmark-full-colour.png`.
 *
 * The approved wordmark is a clay render: the word in navy with texture, a
 * bevel and a drop shadow, over a coral swash. That navy is fixed, and fixed
 * navy is invisible on the dark theme's near-black ground — which is the one
 * reason the artwork could not simply be dropped into the app as-is.
 *
 * Tracing it to vector does not fix that (the navy would be baked into paths
 * instead of pixels) and setting the word in a real typeface loses the clay.
 * So instead this remaps *only* the navy pixels onto a cream ramp, keyed to
 * each pixel's own luminance. The clay texture, the bevel and the highlights
 * all survive, because they are luminance variation and the ramp preserves it.
 *
 * The coral swash is deliberately left alone: coral is theme-invariant in this
 * palette (see BRANDING.md) and already clears contrast on both grounds.
 *
 * Rasterising is done with Playwright's Chromium, which is already a
 * devDependency — the same reason `gen-brand-icons.mjs` uses it, and the same
 * avoidance of a native image toolchain in a fork whose `package.json` has to
 * stay mergeable.
 *
 * Run with node, not bun: `node scripts/gen-brand-wordmark.mjs`.
 */

import { chromium } from "@playwright/test";
import fs from "node:fs";

const SOURCE = "brand-assets/wordmark-full-colour.png";
const OUT_LIGHT = "src/shorthand/brand/wordmark-light.png";
const OUT_DARK = "src/shorthand/brand/wordmark-dark.png";

/**
 * Output width. Nothing renders the wordmark above a 40px cap height (the
 * onboarding lockup; the sidebar uses 24), so 600px covers 3x that on the
 * highest-DPI display and still cuts the 1004px source down substantially.
 * Raise this if the wordmark ever gets a larger home.
 */
const OUT_WIDTH = 600;

/**
 * Cream ramp endpoints for the dark theme, in the same family as
 * `--dark-color-text` (#F6F1E8). The shadow end is what the clay's own
 * shadowed facets become; the highlight end is its lit facets. The spread
 * between them is what keeps the render looking moulded rather than flat.
 */
const CREAM_SHADOW = [196, 185, 163];
const CREAM_HIGHLIGHT = [255, 253, 246];

if (!fs.existsSync(SOURCE)) {
  throw new Error(`Missing approved wordmark artwork at ${SOURCE}`);
}
const SOURCE_URI =
  "data:image/png;base64," + fs.readFileSync(SOURCE).toString("base64");

const browser = await chromium.launch();
try {
  const page = await browser.newPage({ viewport: { width: 64, height: 64 } });
  const result = await page.evaluate(
    async ({ uri, outWidth, shadow, highlight }) => {
      const img = new Image();
      img.src = uri;
      await img.decode();

      const scale = outWidth / img.width;
      const w = outWidth;
      const h = Math.round(img.height * scale);

      const draw = () => {
        const canvas = document.createElement("canvas");
        canvas.width = w;
        canvas.height = h;
        const ctx = canvas.getContext("2d");
        ctx.imageSmoothingQuality = "high";
        ctx.drawImage(img, 0, 0, w, h);
        return { canvas, ctx };
      };

      // The word is blue-dominant; the swash is red-dominant. One channel
      // comparison separates them without needing a full HSL conversion, and
      // it also leaves the neutral drop shadow untouched — which is correct,
      // since a shadow should not be recoloured into the ink.
      const isNavy = (r, g, b) => b > r + 25;

      const luminance = (r, g, b) =>
        (0.2126 * r + 0.7152 * g + 0.0722 * b) / 255;

      const light = draw();
      const lightUri = light.canvas.toDataURL("image/png");

      const dark = draw();
      const image = dark.ctx.getImageData(0, 0, w, h);
      const d = image.data;

      // Pass 1: the navy's own luminance range, so the ramp is keyed to the
      // artwork rather than to assumed values. Re-measuring per run means a
      // re-rendered wordmark maps correctly without anyone editing constants.
      let lo = 1;
      let hi = 0;
      let navyPixels = 0;
      for (let i = 0; i < d.length; i += 4) {
        if (d[i + 3] < 25) continue;
        if (!isNavy(d[i], d[i + 1], d[i + 2])) continue;
        navyPixels++;
        const L = luminance(d[i], d[i + 1], d[i + 2]);
        if (L < lo) lo = L;
        if (L > hi) hi = L;
      }

      // Pass 2: remap onto the cream ramp, preserving each pixel's position
      // within that range.
      const span = Math.max(hi - lo, 1e-4);
      for (let i = 0; i < d.length; i += 4) {
        if (d[i + 3] < 25) continue;
        if (!isNavy(d[i], d[i + 1], d[i + 2])) continue;
        const t = Math.min(
          1,
          Math.max(0, (luminance(d[i], d[i + 1], d[i + 2]) - lo) / span),
        );
        for (let c = 0; c < 3; c++) {
          d[i + c] = shadow[c] + (highlight[c] - shadow[c]) * t;
        }
      }
      dark.ctx.putImageData(image, 0, 0);

      return {
        light: lightUri,
        dark: dark.canvas.toDataURL("image/png"),
        w,
        h,
        navyPixels,
      };
    },
    {
      uri: SOURCE_URI,
      outWidth: OUT_WIDTH,
      shadow: CREAM_SHADOW,
      highlight: CREAM_HIGHLIGHT,
    },
  );

  if (result.navyPixels === 0) {
    throw new Error(
      "Found no navy pixels to recolour — the artwork's ink is not blue-dominant " +
        "any more, so the dark-theme variant would be identical to the light one.",
    );
  }

  for (const [path, uri] of [
    [OUT_LIGHT, result.light],
    [OUT_DARK, result.dark],
  ]) {
    fs.writeFileSync(path, Buffer.from(uri.split(",")[1], "base64"));
    console.log(
      `wrote ${path} (${result.w}x${result.h}, ${Math.round(fs.statSync(path).size / 1024)}kB)`,
    );
  }
} finally {
  await browser.close();
}
