/*
 * Screenshots the Notes section's Google Docs setup, one shot per note taker.
 *
 * Starts nothing: run the dev server yourself first, in another terminal.
 *
 *   bun x vite dev --port 5199
 *   node brand-preview/shot-notes.mjs
 *
 * Separate from `shot.mjs` so the Notes shots can be retaken without
 * rewriting the nine settings shots, which only change when the frame does.
 * Same rules as there: every state is clicked into, from the mock's default
 * settings (Obsidian, then Claude), and any console warning or error fails
 * the run, because an unhandled command is a gap in the mock that the
 * picture would otherwise hide.
 *
 *   notes-claude.png          Google Docs chosen, Claude as the note taker
 *   notes-layout-menu.png     the "Organize notes" menu open on its two
 *                             choices
 *   notes-folder.png          a doc per meeting, in the folder Shorthand made
 *   notes-agent-menu.png      the note taker menu open on its five choices
 *   notes-codex.png           ChatGPT (Codex), with its optional model
 *   notes-acp.png             another ACP app, its program still missing
 *   notes-llm.png             your own API key, model and key still missing
 *   notes-llm-advanced.png    the same on an OpenAI-compatible server, with
 *                             Advanced on, so the server URL and the
 *                             OAuth client rows show
 *   notes-llm-advanced-dark.png  the same, dark
 */

import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";
import { chromium } from "@playwright/test";

const HERE = dirname(fileURLToPath(import.meta.url));
const BASE = "http://localhost:5199/brand-preview";
// The real window's width; tall enough that no state here is cut off.
const VIEWPORT = { width: 680, height: 1100 };

const browser = await chromium.launch();
const problems = [];

const page = await browser.newPage({
  viewport: VIEWPORT,
  deviceScaleFactor: 2,
});
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
await page.goto(`${BASE}/index.html`, { waitUntil: "networkidle" });

const shoot = async (name) => {
  // Off every control, so no row is painted in its hover state, then past
  // the longest colour transition (see shot.mjs).
  await page.mouse.move(0, 0);
  await page.evaluate(() => document.fonts.ready);
  await page.waitForTimeout(500);
  const path = join(HERE, `${name}.png`);
  await page.screenshot({ path, fullPage: true });
  console.log(`wrote ${path}`);
};

/** The settings row whose title is `title`. */
const row = (title) =>
  page
    .locator(`h3:text-is("${title}")`)
    .locator("xpath=ancestor::div[.//button[@aria-expanded]][1]");

/** Opens the dropdown in the row titled `title`. */
const openMenu = (title) => row(title).locator("button[aria-expanded]").click();

/** Picks `label` from the dropdown in the row titled `title`. */
const choose = async (title, label) => {
  await openMenu(title);
  await row(title).getByRole("button", { name: label, exact: true }).click();
  await row(title)
    .locator(`button[aria-expanded="false"]:has-text("${label}")`)
    .waitFor({ timeout: 5000 });
};

/** Waits for a row to be on screen, which asserts what the choice reveals. */
const expectRow = (title) =>
  page.locator(`h3:text-is("${title}")`).first().waitFor({ timeout: 5000 });

await page.waitForSelector('nav button[aria-current="page"] >> text=Modes');
await page.locator("nav button >> text=Notes").click();
await expectRow("Write meeting notes to");

await choose("Write meeting notes to", "Google Docs");
await expectRow("Note taker");
await shoot("notes-claude");

await openMenu("Organize notes");
await row("Organize notes")
  .getByRole("button", { name: "Doc per meeting", exact: true })
  .waitFor();
await shoot("notes-layout-menu");
await row("Organize notes")
  .getByRole("button", { name: "Doc per meeting", exact: true })
  .click();
await expectRow("Meeting folder");
await page
  .getByText('Meetings go into "Shorthand Meeting Notes"')
  .waitFor({ timeout: 5000 });
await shoot("notes-folder");
// Back to one doc, so the note taker shots match the earlier ones.
await choose("Organize notes", "Tab per meeting");
await expectRow("Meeting doc");

await openMenu("Note taker");
await row("Note taker")
  .getByRole("button", { name: "Your own API key", exact: true })
  .waitFor();
await shoot("notes-agent-menu");
// Closes it again without choosing.
await openMenu("Note taker");
await row("Note taker")
  .locator('button[aria-expanded="false"]')
  .waitFor({ timeout: 5000 });

await choose("Note taker", "ChatGPT (Codex)");
await expectRow("Model");
await shoot("notes-codex");

await choose("Note taker", "Another app (ACP)");
await expectRow("Program");
await expectRow("Arguments");
await page.getByText("Add its program below").waitFor({ timeout: 5000 });
await shoot("notes-acp");

await choose("Note taker", "Your own API key");
await expectRow("Provider");
await expectRow("API key");
await page.getByText("Add a model below").waitFor({ timeout: 5000 });
await shoot("notes-llm");

await choose("Provider", "OpenAI-compatible server");
await expectRow("Server URL");
const advancedSwitch = page.getByRole("switch");
await advancedSwitch.click();
await page.waitForSelector('[role="switch"][aria-checked="true"]', {
  timeout: 5000,
});
await shoot("notes-llm-advanced");

await page.evaluate(() => {
  document.documentElement.dataset.theme = "dark";
});
await shoot("notes-llm-advanced-dark");

await browser.close();

if (problems.length > 0) {
  console.error(`\n${problems.length} console problem(s) — see above.`);
  process.exit(1);
}
