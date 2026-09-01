/**
 * Fork-only. Derives the two lockup assets the UI loads from the approved
 * artwork in `brand-assets/logo-full-colour-transparent.png`.
 *
 * The source is the complete lockup as delivered: the pen's nib flows into the
 * S, the bird perches on the barrel, and the coral swash underlines the word.
 * That interlock is the whole composition, and it is why the lockup ships as
 * one image. An earlier version drew the mark and the word as two stacked
 * rasters sized against each other — it could not reproduce the connection at
 * all, and read as a bug hovering above a word.
 *
 * The one reason the artwork cannot simply be dropped in as-is: the word's ink
 * is a fixed navy, and fixed navy is invisible on the dark theme's near-black
 * ground. So this remaps *only the word's* navy pixels onto a cream ramp, keyed
 * to each pixel's own luminance. The clay texture, the bevel and the highlights
 * all survive, because they are luminance variation and the ramp preserves it.
 *
 * "Only the word's" is the part that needs care. The bird is blue too, and the
 * blue-dominance test that isolates ink from the coral swash cannot tell the
 * bird's body from the word's letterforms — recolouring by colour alone turns
 * the bird cream. The two are cleanly separated vertically instead: the bird's
 * navy ends well above where the word's begins, with a band of rows carrying
 * neither in between. That seam is measured per run rather than hard-coded, and
 * the script fails loudly if it is not clearly there.
 *
 * The coral swash and the pen are deliberately left alone. Coral is
 * theme-invariant in this palette (see BRANDING.md) and already clears contrast
 * on both grounds; the pen's black-and-gold is illustration, not ink.
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

const SOURCE = "brand-assets/logo-full-colour-transparent.png";
const OUT_LIGHT = "src/shorthand/brand/logo-light.png";
const OUT_DARK = "src/shorthand/brand/logo-dark.png";

/**
 * Output width for the trimmed lockup. The largest home the lockup has is the
 * onboarding panel at a 40px cap height, which this artwork renders at roughly
 * 175px wide; 700px covers 4x that on the highest-DPI display and still cuts
 * the ~1450px source down substantially. Raise it if the lockup gets a larger
 * home than onboarding.
 */
const OUT_WIDTH = 700;

/**
 * Cream ramp endpoints for the dark theme, in the same family as
 * `--dark-color-text` (#F6F1E8). The shadow end is what the clay's own
 * shadowed facets become; the highlight end is its lit facets. The spread
 * between them is what keeps the render looking moulded rather than flat.
 */
const CREAM_SHADOW = [196, 185, 163];
const CREAM_HIGHLIGHT = [255, 253, 246];

if (!fs.existsSync(SOURCE)) {
  throw new Error(`Missing approved lockup artwork at ${SOURCE}`);
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

      // -- 1. Trim to the drawing. The delivered artwork carries generous
      // transparent padding, which would otherwise become invisible margin the
      // component has to size around.
      const probe = document.createElement("canvas");
      probe.width = img.width;
      probe.height = img.height;
      const pctx = probe.getContext("2d");
      pctx.drawImage(img, 0, 0);
      const p = pctx.getImageData(0, 0, probe.width, probe.height).data;

      let minX = Infinity;
      let minY = Infinity;
      let maxX = -1;
      let maxY = -1;
      for (let y = 0; y < probe.height; y++) {
        for (let x = 0; x < probe.width; x++) {
          if (p[(y * probe.width + x) * 4 + 3] < 25) continue;
          if (x < minX) minX = x;
          if (x > maxX) maxX = x;
          if (y < minY) minY = y;
          if (y > maxY) maxY = y;
        }
      }
      const srcW = maxX - minX + 1;
      const srcH = maxY - minY + 1;

      const w = outWidth;
      const h = Math.round((srcH / srcW) * w);
      const draw = () => {
        const canvas = document.createElement("canvas");
        canvas.width = w;
        canvas.height = h;
        const ctx = canvas.getContext("2d");
        ctx.imageSmoothingQuality = "high";
        ctx.drawImage(img, minX, minY, srcW, srcH, 0, 0, w, h);
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

      // -- 2. Find the seam between the bird's blue and the word's navy.
      // Both pass isNavy, so the split has to come from geometry. Rows are
      // scored by navy coverage; the quietest run of rows in the middle of the
      // image is the gap between the two, and the seam is its centre.
      const rowNavy = new Array(h).fill(0);
      for (let y = 0; y < h; y++) {
        for (let x = 0; x < w; x++) {
          const i = (y * w + x) * 4;
          if (d[i + 3] < 25) continue;
          if (isNavy(d[i], d[i + 1], d[i + 2])) rowNavy[y]++;
        }
      }
      const peak = Math.max(...rowNavy);
      // "Quiet" is relative to the busiest row, so the test survives a
      // re-render at a different size or density.
      const quiet = peak * 0.02;
      let best = null;
      let runStart = -1;
      for (let y = Math.round(h * 0.25); y <= Math.round(h * 0.75); y++) {
        const isQuiet = y < h && rowNavy[y] <= quiet;
        if (isQuiet && runStart < 0) runStart = y;
        if ((!isQuiet || y === Math.round(h * 0.75)) && runStart >= 0) {
          const run = { start: runStart, end: y - 1 };
          run.len = run.end - run.start + 1;
          if (!best || run.len > best.len) best = run;
          runStart = -1;
        }
      }
      if (!best || best.len < 3) {
        return { failed: "no-seam", peak, rowNavy };
      }
      const seam = Math.round((best.start + best.end) / 2);

      // -- 3. Remap the word's navy only. Pass one measures its own luminance
      // range so the ramp is keyed to the artwork rather than to assumed
      // values; pass two remaps onto the cream ramp, preserving each pixel's
      // position within that range.
      let lo = 1;
      let hi = 0;
      let navyPixels = 0;
      for (let y = seam; y < h; y++) {
        for (let x = 0; x < w; x++) {
          const i = (y * w + x) * 4;
          if (d[i + 3] < 25) continue;
          if (!isNavy(d[i], d[i + 1], d[i + 2])) continue;
          navyPixels++;
          const L = luminance(d[i], d[i + 1], d[i + 2]);
          if (L < lo) lo = L;
          if (L > hi) hi = L;
        }
      }
      const span = Math.max(hi - lo, 1e-4);
      for (let y = seam; y < h; y++) {
        for (let x = 0; x < w; x++) {
          const i = (y * w + x) * 4;
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
      }
      dark.ctx.putImageData(image, 0, 0);

      // -- 4. Measure the word's cap height, which is the unit the component
      // sizes by. The top is the first row of the word cluster; the baseline
      // is the last row still carrying most of the word, which excludes the
      // S's descending flourish below it.
      const wordRows = rowNavy.slice(seam);
      const wordPeak = Math.max(...wordRows);
      let capTop = -1;
      let baseline = -1;
      for (let y = 0; y < wordRows.length; y++) {
        if (wordRows[y] > wordPeak * 0.02 && capTop < 0) capTop = seam + y;
        if (wordRows[y] > wordPeak * 0.25) baseline = seam + y;
      }

      return {
        light: lightUri,
        dark: dark.canvas.toDataURL("image/png"),
        w,
        h,
        seam,
        seamRun: best,
        navyPixels,
        capHeight: baseline - capTop,
        capTop,
        baseline,
        trimmed: { minX, minY, srcW, srcH },
      };
    },
    {
      uri: SOURCE_URI,
      outWidth: OUT_WIDTH,
      shadow: CREAM_SHADOW,
      highlight: CREAM_HIGHLIGHT,
    },
  );

  if (result.failed === "no-seam") {
    throw new Error(
      "Found no clear gap between the bird's blue and the word's navy. The " +
        "lockup's composition has changed, so recolouring by row would bleed " +
        "into the illustration — re-check the artwork before regenerating.",
    );
  }
  if (result.navyPixels === 0) {
    throw new Error(
      "Found no navy pixels below the seam to recolour — the word's ink is not " +
        "blue-dominant any more, so the dark-theme variant would be identical " +
        "to the light one.",
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

  // These two ratios are the contract with ShorthandWordmark.tsx, which sizes
  // the lockup from the word's cap height. Print them so a re-render that
  // shifts the composition shows up as a number to reconcile rather than as a
  // silently mis-scaled logo.
  console.log(
    `\nseam row ${result.seam} (quiet run ${result.seamRun.start}-${result.seamRun.end}), ` +
      `word cap height ${result.capHeight}px (rows ${result.capTop}-${result.baseline})`,
  );
  console.log("ShorthandWordmark.tsx constants:");
  console.log(
    `  LOCKUP_WIDTH_IN_CAP_HEIGHTS  = ${(result.w / result.capHeight).toFixed(4)}   // ${result.w} / ${result.capHeight}`,
  );
  console.log(
    `  LOCKUP_HEIGHT_IN_CAP_HEIGHTS = ${(result.h / result.capHeight).toFixed(4)}   // ${result.h} / ${result.capHeight}`,
  );
} finally {
  await browser.close();
}
