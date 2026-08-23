/*
 * Screenshots the preview pages.
 *
 * Starts nothing: run the dev server yourself first, in another terminal.
 *
 *   bun x vite dev --port 5199
 *   node brand-preview/shot.mjs
 *
 * Run this with node, not bun — Playwright's browser launch hangs under bun on
 * Windows. If chromium is missing: bun x playwright install chromium
 *
 * Seven shots, from two pages:
 *
 *   light.png / dark.png            the component gallery (gallery.html)
 *   settings-light.png              the real settings window, Modes, Advanced off
 *   settings-dark.png               the same, dark
 *   settings-advanced.png           the same, Advanced on
 *   settings-dictation.png          the same, Dictation tab
 *   settings-cancel.png             the same, Meetings tab, Advanced on, with
 *                                   dictation switched off
 *
 * The settings states are reached by clicking the page — the advanced switch,
 * the tab, the toggle — rather than by seeding state. A screenshot of state
 * that was injected proves the renderer works; a screenshot of state that was
 * clicked into proves the control does.
 */

import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";
import { chromium } from "@playwright/test";

const HERE = dirname(fileURLToPath(import.meta.url));
const BASE = "http://localhost:5199/brand-preview";

// Width is the real main window's (680, from `lib.rs`), so line lengths,
// wrapping and the sidebar's share of the window are the ones users get.
// Height is not: the window opens at 570, but every settings state here is
// taller than that, and four shots each cut off at a different row compares
// scroll positions rather than designs. 1000 clears the tallest state, so what
// differs between these four is the content. Roughly the top 570px of each is
// what is on screen before scrolling.
const SETTINGS_VIEWPORT = { width: 680, height: 1000 };
// Wide enough for the gallery's two-column Now/Proposed comparison.
const GALLERY_VIEWPORT = { width: 1400, height: 1000 };

const browser = await chromium.launch();

const problems = [];

const openPage = async (viewport, path) => {
  const page = await browser.newPage({ viewport, deviceScaleFactor: 2 });
  page.on("console", (message) => {
    if (message.type() === "error" || message.type() === "warning") {
      problems.push(message.text());
      console.error(`[page] ${message.type()}: ${message.text()}`);
    }
  });
  page.on("pageerror", (error) => {
    problems.push(error.message);
    console.error("[page]", error.message);
  });
  await page.goto(`${BASE}/${path}`, { waitUntil: "networkidle" });
  return page;
};

const shoot = async (page, name) => {
  // Park the cursor off every control first. Playwright leaves the mouse where
  // it last clicked, and the advanced switch and the sidebar rows both have
  // hover backgrounds — a shot taken straight after a click paints one row
  // differently for a reason that has nothing to do with the design.
  await page.mouse.move(0, 0);
  await page.evaluate(() => document.fonts.ready);
  // Flipping the theme changes colours that half the components animate:
  // `transition-colors` is on the sidebar rows, the tabs, every Button variant
  // and the Select. Screenshot within a frame of the flip and they are caught
  // mid-tween, which reads as a contrast bug that isn't there. 500ms clears the
  // longest transition in the app (150ms) with room to spare, and also lets the
  // gallery's swatch strip re-read its custom properties. The same wait covers
  // a click that reveals rows, for the same reason.
  await page.waitForTimeout(500);
  const path = join(HERE, `${name}.png`);
  await page.screenshot({ path, fullPage: true });
  console.log(`wrote ${path}`);
};

const setTheme = (page, theme) =>
  page.evaluate((value) => {
    document.documentElement.dataset.theme = value;
  }, theme);

// --- The component gallery -------------------------------------------------

const gallery = await openPage(GALLERY_VIEWPORT, "gallery.html");
await gallery.waitForSelector("#root >> text=Buttons");
for (const theme of ["light", "dark"]) {
  await setTheme(gallery, theme);
  await shoot(gallery, theme);
}
await gallery.close();

// --- The real settings window ----------------------------------------------

const settings = await openPage(SETTINGS_VIEWPORT, "index.html");
// The app opens on Modes, the first registered section — asserted rather than
// assumed, since the shots below are all of that section.
await settings.waitForSelector('nav button[aria-current="page"] >> text=Modes');
await settings.getByRole("tab", { name: "Meetings" }).waitFor();

const advancedSwitch = settings.getByRole("switch");
// Waits for the state rather than reading it once. A single read straight
// after the click is a race with React's next paint — it passed for a while
// and then failed intermittently once the reveal started doing more work on
// click. The wait still fails loudly (and quickly) if the control genuinely
// does not toggle, which is the assertion that was wanted.
const expectAdvanced = (expected) =>
  settings.waitForSelector(`[role="switch"][aria-checked="${expected}"]`, {
    timeout: 5000,
  });

await expectAdvanced(false);
for (const theme of ["light", "dark"]) {
  await setTheme(settings, theme);
  await shoot(settings, `settings-${theme}`);
}

await setTheme(settings, "light");
await advancedSwitch.click();
await expectAdvanced(true);
await shoot(settings, "settings-advanced");

// Back off before the Dictation shot: that one is about the tab, and leaving
// advanced on would make it about both at once.
await advancedSwitch.click();
await expectAdvanced(false);
await settings.getByRole("tab", { name: "Dictation" }).click();
await shoot(settings, "settings-dictation");

// --- Cancel, which only exists with dictation switched off -----------------
//
// Cancel is hidden while *either* mode has push-to-talk on (`anyPushToTalk`
// in `ModesSettings.tsx`), and dictation ships with push_to_talk true. The
// mock deliberately keeps `dictation.enabled` true so the Dictation shot
// above photographs live rows instead of a column of disabled ones — which
// means that in every shot before this one, Cancel is suppressed by a mode a
// default install has switched off. Without this shot the harness cannot
// photograph the row at all.
//
// It runs last because clicking that toggle mutates the mock's SETTINGS, and
// every shot above wants dictation on.
//
// The previous shot left us on the Dictation tab, which is where the toggle
// is.
const enableDictation = settings
  // The row's title is a sibling <h3> rather than a <label> wrapping the
  // input, so the checkbox has no accessible name and `getByRole("checkbox",
  // { name })` cannot reach it. Anchor on the visible title and walk to the
  // checkbox in the same row.
  .locator('h3:text-is("Enable dictation")')
  .locator('xpath=ancestor::div[.//input[@type="checkbox"]][1]')
  .locator('input[type="checkbox"]');
// `force` because `ToggleSwitch` renders its input `sr-only` — a 1px clipped
// box, which Playwright reads as not visible. The click still lands on the
// real control, which is what makes this a shot of state that was clicked
// into rather than seeded.
await enableDictation.click({ force: true });
// Waits for the state rather than reading it once, for the same reason
// `expectAdvanced` does.
await settings.waitForFunction(() => {
  const title = [...document.querySelectorAll("h3")].find(
    (node) => node.textContent === "Enable dictation",
  );
  return title?.closest("div.px-4")?.querySelector("input")?.checked === false;
});

await settings.getByRole("tab", { name: "Meetings" }).click();
await advancedSwitch.click();
await expectAdvanced(true);
// The row this shot exists for, asserted before it is photographed. Without
// this a regression in the gate would quietly produce a picture of its own
// absence, which is the failure this whole shot was added to catch.
await settings.getByText("Shared by both modes").waitFor({ timeout: 5000 });
await shoot(settings, "settings-cancel");

await settings.close();
await browser.close();

if (problems.length > 0) {
  console.error(`\n${problems.length} console problem(s) — see above.`);
  process.exit(1);
}
