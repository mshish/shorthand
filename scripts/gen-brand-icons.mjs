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
 * Read the approved standalone SVG rather than importing `mark.paths.ts`, so
 * this stays plain ESM that node can run without a TypeScript loader. Capture
 * every path and its fill rule: the even-odd counters in the bird and pen are
 * part of the silhouette, not optional rendering detail.
 */
const MARK_SVG = fs.readFileSync("src/shorthand/brand/mark.svg", "utf8");
const MARK_PATH_ELEMENT_COUNT = [...MARK_SVG.matchAll(/<path\b/g)].length;
const MARK_PATH_ELEMENTS = [...MARK_SVG.matchAll(/<path\b[^>]*\/?>/g)].map(
  ([element]) => element,
);

if (MARK_PATH_ELEMENTS.length !== MARK_PATH_ELEMENT_COUNT) {
  throw new Error(
    `Could not parse every path in mark.svg: found ${MARK_PATH_ELEMENT_COUNT} path tags but parsed ${MARK_PATH_ELEMENTS.length}`,
  );
}
if (MARK_PATH_ELEMENTS.length === 0) {
  throw new Error("mark.svg contains no paths");
}

function attribute(element, name) {
  return new RegExp(`\\b${name}="([^"]+)"`).exec(element)?.[1];
}

const MARK_PATHS = MARK_PATH_ELEMENTS.map((element, index) => {
  const d = attribute(element, "d");
  if (!d) {
    throw new Error(`Path ${index + 1} in mark.svg has no d attribute`);
  }
  return { d, fillRule: attribute(element, "fill-rule") };
});

/** Paper and ink, matching src/shorthand/brand/theme.css. */
const PAPER = "#FAF5EA";
const INK = "#14202B";
/**
 * The tray's "Colored" theme sits on an uncontrolled menu bar. This fallback
 * measures 5.41:1 on white and 3.88:1 on black, clearing the 3:1 non-text floor
 * on both rather than assuming a known background.
 */
const TRAY_FALLBACK = "#2E6F9E";

const ACCENT = "#0B5F8A";
const ACCENT_STROKE = "#084A6C";

// Measured from the approved artwork, whose 128-unit canvas has deliberate
// slack around a landscape drawing. Placement fits and centres these visible
// bounds instead of centring the square canvas.
const MARK_BOUNDS = {
  minX: 8,
  minY: 20,
  width: 112,
  height: 80,
};

/**
 * The mark, scaled and centred by its drawn bounds inside a `box`-sized square.
 * `scale` is the fraction of the box its longest visible side should occupy.
 */
function mark(box, scale, fill, dx = 0, dy = 0) {
  const visibleSize = box * scale;
  const s = visibleSize / Math.max(MARK_BOUNDS.width, MARK_BOUNDS.height);
  const offX = (box - MARK_BOUNDS.width * s) / 2 - MARK_BOUNDS.minX * s + dx;
  const offY = (box - MARK_BOUNDS.height * s) / 2 - MARK_BOUNDS.minY * s + dy;
  const paths = MARK_PATHS.map(({ d, fillRule }) => {
    const fillRuleAttribute = fillRule ? ` fill-rule="${fillRule}"` : "";
    return `<path d="${d}" fill="${fill}"${fillRuleAttribute}/>`;
  }).join("");
  return `<g transform="translate(${offX} ${offY}) scale(${s})">${paths}</g>`;
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
const BADGE_X = 54;
// One unit lower than the bounding-box estimate: the pen has lower geometry
// near x=54, and y=55 gives its gutter real clearance while the badge still
// ends exactly at the 64-unit frame.
const BADGE_Y = 55;
const TRAY_BOX = 64;
const TRAY_MARK_WIDTH = 62;
const TRAY_MARK_SCALE = TRAY_MARK_WIDTH / TRAY_BOX;
const TRAY_MARK_HEIGHT =
  TRAY_MARK_WIDTH * (MARK_BOUNDS.height / MARK_BOUNDS.width);
const TRAY_MARK_TOP_OFFSET = -(TRAY_BOX - TRAY_MARK_HEIGHT) / 2;

/**
 * A landscape mark in a square frame leaves a strip below it rather than an
 * empty corner beside it, so the badge belongs in that strip. Fit the visible
 * width to 62 of 64 units and top-align it: the mark occupies about x=1..63,
 * y=0..44.3, leaving the badge room below without sacrificing the dimension
 * along which the silhouette reads. Protecting that width matters most at the
 * mark's primary 16px menu-bar size. Idle uses this same placement, so only the
 * badge changes between states.
 */
function trayMark(fill) {
  return mark(TRAY_BOX, TRAY_MARK_SCALE, fill, 0, TRAY_MARK_TOP_OFFSET);
}

/**
 * Black ring behind the badge. The mark is geometrically clear of it; the ring
 * remains so antialiasing can never bridge mark and badge at 16px.
 */
function badgeGutter() {
  return `<circle cx="${BADGE_X}" cy="${BADGE_Y}" r="10.5" fill="black"/>`;
}

function badged(color, badge) {
  return svg(
    64,
    `<mask id="b"><rect width="64" height="64" fill="black"/>` +
      trayMark("white") +
      badgeGutter() +
      badge +
      `</mask>` +
      `<rect width="64" height="64" fill="${color}" mask="url(#b)"/>`,
  );
}

function trayIdle(color) {
  return svg(64, trayMark(color));
}

/** Recording: a solid dot. The record symbol, unchanged since tape. */
function trayRecording(color) {
  return badged(
    color,
    `<circle cx="${BADGE_X}" cy="${BADGE_Y}" r="9" fill="white"/>`,
  );
}

/**
 * Transcribing: a ring with a gap at the top — the shape every spinner in every
 * toolkit uses for "working". Static here, because a tray icon is a still PNG.
 */
function trayTranscribing(color) {
  const r = 7;
  // Endpoints of a 270-degree clockwise sweep, leaving a quarter gap at the
  // top — wide enough to still read as open at 16px.
  const x0 = BADGE_X + r * Math.cos((-45 * Math.PI) / 180);
  const y0 = BADGE_Y + r * Math.sin((-45 * Math.PI) / 180);
  const x1 = BADGE_X + r * Math.cos((225 * Math.PI) / 180);
  const y1 = BADGE_Y + r * Math.sin((225 * Math.PI) / 180);
  return badged(
    color,
    `<path d="M${x0.toFixed(2)} ${y0.toFixed(2)} A${r} ${r} 0 1 1 ${x1.toFixed(2)} ${y1.toFixed(2)}" ` +
      `fill="none" stroke="white" stroke-width="4" stroke-linecap="round"/>`,
  );
}

/** Warning: an exclamation, reversed out of a disc. */
function trayWarning(color) {
  return badged(
    color,
    `<circle cx="${BADGE_X}" cy="${BADGE_Y}" r="9" fill="white"/>` +
      `<rect x="52.4" y="48" width="3.2" height="7" rx="1.6" fill="black"/>` +
      `<circle cx="54" cy="59" r="1.8" fill="black"/>`,
  );
}

/** The installed app icon: the mark reversed out of an ink tile. */
function appIcon(box) {
  const inset = box * 0.09;
  const tile = box - inset * 2;
  return svg(
    box,
    `<defs><linearGradient id="g" x1="0" y1="0" x2="0" y2="1">` +
      `<stop offset="0" stop-color="${ACCENT}"/><stop offset="1" stop-color="${ACCENT_STROKE}"/>` +
      `</linearGradient></defs>` +
      `<rect x="${inset}" y="${inset}" width="${tile}" height="${tile}" rx="${tile * 0.22}" fill="url(#g)"/>` +
      `<g transform="translate(${inset} ${inset})">${mark(tile, 0.56, PAPER)}</g>`,
  );
}

const TARGETS = [
  // Menu bar / system tray. `*_dark.png` is upstream's name for the *dark
  // glyph* shown on a light tray, not for dark mode. Every tray SVG paints one
  // requested colour only; badge geometry is built in a black/white alpha mask,
  // so its holes never introduce a second painted colour. That keeps idle and
  // badged PNGs valid for macOS template mode.
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
  // The tray's "Colored" theme: one fallback blue for every background.
  {
    path: "src-tauri/resources/handy.png",
    box: 64,
    svg: trayIdle(TRAY_FALLBACK),
  },
  {
    path: "src-tauri/resources/recording.png",
    box: 64,
    svg: trayRecording(TRAY_FALLBACK),
  },
  {
    path: "src-tauri/resources/transcribing.png",
    box: 64,
    svg: trayTranscribing(TRAY_FALLBACK),
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
