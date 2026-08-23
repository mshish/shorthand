/**
 * Fork-only. Renders the Shorthand app icon and the tray artwork from the mark
 * in `src/shorthand/brand/mark.svg`.
 *
 * The tray needs PNGs (Tauri hands the file straight to the platform tray API),
 * and the app icon needs a 1024px master for `tauri icon` to slice. Both are
 * derived from one SVG here rather than drawn in an image editor, so moving the
 * mark moves every icon with it.
 *
 * Rasterising is done with Playwright's Chromium, which is already a
 * devDependency for the smoke tests — this deliberately avoids adding a native
 * image toolchain (sharp / resvg) to a fork that has to keep `package.json`
 * mergeable.
 *
 * Run with node, not bun: `node scripts/gen-brand-icons.mjs`. Playwright's
 * browser launch hangs under bun on Windows, and this is the only script in the
 * repo that drives a browser.
 *
 * Then: `cd src-tauri && bun x tauri icon` to reslice the app icons
 * (`app-icon.png` is the filename that command looks for by default).
 */

import { chromium } from "@playwright/test";
import fs from "node:fs";

/**
 * Read the mark straight out of the generated SVG rather than importing
 * `mark.generated.ts`, so this stays plain ESM that node can run without a
 * TypeScript loader.
 */
const MARK_PATH = /<path d="([^"]+)"/.exec(
  fs.readFileSync("src/shorthand/brand/mark.svg", "utf8"),
)[1];

/** Paper and ink, matching src/shorthand/brand/theme.css. */
const PAPER = "#f0efea";
const INK = "#2a1b3d";
/**
 * The tray's "Colored" theme sits on a menu bar that may be light or dark, so
 * this is the one violet in the palette chosen to hold up against both rather
 * than against a known background.
 */
const VIOLET = "#7645ad";

/**
 * The mark, scaled and centred inside a `box`-sized square.
 * `scale` is the fraction of the box the 64-unit glyph should occupy.
 */
function mark(box, scale, fill, dx = 0, dy = 0) {
  const s = (box * scale) / 64;
  const off = (box - box * scale) / 2;
  return `<g transform="translate(${off + dx} ${off + dy}) scale(${s})"><path d="${MARK_PATH}" fill="${fill}"/></g>`;
}

function svg(box, body) {
  return `<svg xmlns="http://www.w3.org/2000/svg" width="${box}" height="${box}" viewBox="0 0 ${box} ${box}">${body}</svg>`;
}

/**
 * Tray states are the mark plus a badge saying what the app is doing, always in
 * the same corner, so the eye learns one slot and only has to read what is in
 * it. The mark never changes, because it is the only thing identifying which
 * app the tray item belongs to.
 *
 * Each badge is the symbol its status already owns everywhere else: a solid dot
 * for recording, a gapped ring for work in progress, an exclamation for the
 * warning. Solid / hollow / punctuated stay apart from each other down to 16px,
 * which the previous disc-versus-ring pair did not do as well and, more to the
 * point, said nothing about the state it stood for.
 *
 * Upstream solves this by swapping the glyph itself (hand -> ear -> brain).
 * That is not available here: Shorthand has one glyph, and spending it on
 * status would cost the identity.
 */
const BADGE_X = 48;
const BADGE_Y = 48;

/**
 * The mark, shrunk into the top-left so the badge sits in empty canvas rather
 * than on top of it. An "s" leaves its own bottom-right corner blank, which is
 * exactly where the badge goes, so the two can share the square without the
 * badge's gutter having to cut the stroke — which it did at every larger size
 * tried, amputating the tail and leaving the glyph reading as a hook.
 */
function badgedMark() {
  return mark(64, 0.62, "white", -11.5, -11.5);
}

/**
 * Thin black ring behind the badge. With the mark pulled clear it has almost
 * nothing to cut, and is kept only so antialiasing on the glyph's edge can
 * never bleed into the badge at 16px.
 */
function badgeGutter() {
  return `<circle cx="${BADGE_X}" cy="${BADGE_Y}" r="15.8" fill="black"/>`;
}

function badged(color, badge) {
  return svg(
    64,
    `<mask id="b"><rect width="64" height="64" fill="black"/>` +
      badgedMark() +
      badgeGutter() +
      badge +
      `</mask>` +
      `<rect width="64" height="64" fill="${color}" mask="url(#b)"/>`,
  );
}

function trayIdle(color) {
  return svg(64, mark(64, 0.82, color));
}

/** Recording: a solid dot. The record symbol, unchanged since tape. */
function trayRecording(color) {
  return badged(
    color,
    `<circle cx="${BADGE_X}" cy="${BADGE_Y}" r="14" fill="white"/>`,
  );
}

/**
 * Transcribing: a ring with a gap at the top — the shape every spinner in every
 * toolkit uses for "working". Static here, because a tray icon is a still PNG.
 */
function trayTranscribing(color) {
  const r = 11.5;
  // Endpoints of a 270-degree clockwise sweep, leaving a quarter gap at the
  // top — wide enough to still read as open at 16px.
  const x0 = BADGE_X + r * Math.cos((-45 * Math.PI) / 180);
  const y0 = BADGE_Y + r * Math.sin((-45 * Math.PI) / 180);
  const x1 = BADGE_X + r * Math.cos((225 * Math.PI) / 180);
  const y1 = BADGE_Y + r * Math.sin((225 * Math.PI) / 180);
  return badged(
    color,
    `<path d="M${x0.toFixed(2)} ${y0.toFixed(2)} A${r} ${r} 0 1 1 ${x1.toFixed(2)} ${y1.toFixed(2)}" ` +
      `fill="none" stroke="white" stroke-width="5.5" stroke-linecap="round"/>`,
  );
}

/** Warning: an exclamation, reversed out of a disc. */
function trayWarning(color) {
  return badged(
    color,
    `<circle cx="${BADGE_X}" cy="${BADGE_Y}" r="14" fill="white"/>` +
      `<rect x="45.8" y="40.5" width="4.4" height="9.5" rx="2.2" fill="black"/>` +
      `<circle cx="48" cy="54.2" r="2.4" fill="black"/>`,
  );
}

/** The installed app icon: the mark reversed out of an ink tile. */
function appIcon(box) {
  const inset = box * 0.09;
  const tile = box - inset * 2;
  return svg(
    box,
    `<defs><linearGradient id="g" x1="0" y1="0" x2="0" y2="1">` +
      `<stop offset="0" stop-color="#7a48b0"/><stop offset="1" stop-color="#4e2a77"/>` +
      `</linearGradient></defs>` +
      `<rect x="${inset}" y="${inset}" width="${tile}" height="${tile}" rx="${tile * 0.22}" fill="url(#g)"/>` +
      `<g transform="translate(${inset} ${inset})">${mark(tile, 0.56, PAPER)}</g>`,
  );
}

const TARGETS = [
  // Menu bar / system tray. `*_dark.png` is upstream's name for the *dark
  // glyph* shown on a light tray, not for dark mode.
  { path: "src-tauri/resources/tray_idle.png", box: 64, svg: trayIdle(PAPER) },
  {
    path: "src-tauri/resources/tray_idle_dark.png",
    box: 64,
    svg: trayIdle(INK),
  },
  {
    path: "src-tauri/resources/tray_recording.png",
    box: 64,
    svg: trayRecording(PAPER),
  },
  {
    path: "src-tauri/resources/tray_recording_dark.png",
    box: 64,
    svg: trayRecording(INK),
  },
  {
    path: "src-tauri/resources/tray_transcribing.png",
    box: 64,
    svg: trayTranscribing(PAPER),
  },
  {
    path: "src-tauri/resources/tray_transcribing_dark.png",
    box: 64,
    svg: trayTranscribing(INK),
  },
  {
    path: "src-tauri/resources/tray_idle_warning.png",
    box: 64,
    svg: trayWarning(PAPER),
  },
  {
    path: "src-tauri/resources/tray_idle_warning_dark.png",
    box: 64,
    svg: trayWarning(INK),
  },
  // The tray's "Colored" theme: one violet for every background.
  { path: "src-tauri/resources/handy.png", box: 64, svg: trayIdle(VIOLET) },
  {
    path: "src-tauri/resources/recording.png",
    box: 64,
    svg: trayRecording(VIOLET),
  },
  {
    path: "src-tauri/resources/transcribing.png",
    box: 64,
    svg: trayTranscribing(VIOLET),
  },
  // Master for `tauri icon`, at the filename that command defaults to.
  { path: "src-tauri/app-icon.png", box: 1024, svg: appIcon(1024) },
];

const browser = await chromium.launch();
try {
  for (const target of TARGETS) {
    const page = await browser.newPage({
      viewport: { width: target.box, height: target.box },
    });
    await page.setContent(
      `<html><body style="margin:0">${target.svg}</body></html>`,
    );
    await page.screenshot({ path: target.path, omitBackground: true });
    await page.close();
    console.log(`wrote ${target.path}`);
  }
} finally {
  await browser.close();
}
