/*
 * Screenshots the preview page in both themes.
 *
 * Starts nothing: run the dev server yourself first, in another terminal.
 *
 *   bun x vite dev --port 5199
 *   node brand-preview/shot.mjs
 *
 * Run this with node, not bun — Playwright's browser launch hangs under bun on
 * Windows. If chromium is missing: bun x playwright install chromium
 */

import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";
import { chromium } from "@playwright/test";

const HERE = dirname(fileURLToPath(import.meta.url));
const URL = "http://localhost:5199/brand-preview/index.html";
const THEMES = ["light", "dark"];

const browser = await chromium.launch();
const page = await browser.newPage({
  // Wide enough for the two-column Now/Proposed comparison plus the sidebar.
  viewport: { width: 1400, height: 1000 },
  deviceScaleFactor: 2,
});

page.on("console", (message) => {
  if (message.type() === "error") console.error("[page]", message.text());
});
page.on("pageerror", (error) => console.error("[page]", error.message));

await page.goto(URL, { waitUntil: "networkidle" });
await page.waitForSelector("#root >> text=Buttons");

for (const theme of THEMES) {
  await page.evaluate((value) => {
    document.documentElement.dataset.theme = value;
  }, theme);
  await page.evaluate(() => document.fonts.ready);
  // Flipping the theme changes colours that half the components animate:
  // `transition-colors` is on the sidebar rows, every Button variant and the
  // Select. Screenshot within a frame of the flip and they are caught
  // mid-tween, which reads as a contrast bug that isn't there. 500ms clears
  // the longest transition in the app (150ms) with room to spare, and also
  // lets the swatch strip's MutationObserver re-read and re-render.
  await page.waitForTimeout(500);

  const path = join(HERE, `${theme}.png`);
  await page.screenshot({ path, fullPage: true });
  console.log(`wrote ${path}`);
}

await browser.close();
