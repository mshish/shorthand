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

/**
 * The installed app icon is the full-colour clay render, not the silhouette —
 * the tray is where the mark has to survive 16px as one flat colour, and the
 * app icon is not. It is a raster, so it is embedded as a data URI rather than
 * drawn from paths like everything else in this file.
 *
 * `mark-full-colour-centred.png` is a second delivery of the mark, composed for
 * a square frame: the bird sits centred over the pen rather than beside it, so
 * the drawing is 1.55:1 instead of the lockup mark's 1.76:1 and a much larger
 * share of a square tile is bird. Do not substitute
 * `mark-full-colour-transparent.png` here — that one is composed for the
 * stacked lockup and reads noticeably smaller in a tile.
 */
const COLOUR_MARK_FILE = "brand-assets/mark-full-colour-centred.png";
const COLOUR_MARK_DATA_URI = `data:image/png;base64,${fs.readFileSync(COLOUR_MARK_FILE).toString("base64")}`;

/**
 * The artwork's own canvas, and the drawing's alpha bounds within it. Measured
 * from the delivered file; the slack around the drawing is not padding we want,
 * so placement fits these bounds rather than the canvas, exactly as MARK_BOUNDS
 * does for the silhouette. Re-measure if the artwork is re-delivered.
 */
const COLOUR_MARK_CANVAS = { width: 1377, height: 942 };
const COLOUR_MARK_BOUNDS = { minX: 32, minY: 23, width: 1326, height: 858 };
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

/**
 * The bird's body and the pen's barrel, as fillable regions.
 *
 * The mark is line art: the bird (path 0) and the pen (path 2) are each drawn
 * `evenodd` with two subpaths, so the outer silhouette minus the inner one
 * leaves a hollow interior with a stroke-like ring around it. Painting that
 * inner subpath *behind* the mark fills the interior without touching the
 * ring, the wings, the tail or the nib — which is how the tray states carry
 * their colour.
 *
 * Derived from mark.svg rather than duplicated, so a re-transcription of the
 * approved artwork moves these with it. The shape is asserted because the
 * whole technique depends on those two paths staying two-subpath evenodd
 * shapes: if a future transcription flattens or splits either, this must fail
 * loudly rather than quietly filling the wrong region.
 */
function counterOf(index, name) {
  const path = MARK_PATHS[index];
  const subpaths = path?.d.split(/(?=M)/).filter(Boolean) ?? [];
  if (path?.fillRule !== "evenodd" || subpaths.length !== 2) {
    throw new Error(
      `Expected mark.svg path ${index} to be the ${name}: an evenodd shape ` +
        `with 2 subpaths (outer silhouette + inner counter), got fill-rule=` +
        `${path?.fillRule} with ${subpaths.length} subpath(s). The tray ` +
        `state fills depend on that shape — re-check the transcription.`,
    );
  }
  return subpaths[1];
}

const BIRD_BODY_PATH = counterOf(0, "bird");
const PEN_BARREL_PATH = counterOf(2, "pen");

/** Paper and ink, matching src/shorthand/brand/theme.css. */
const PAPER = "#FAF5EA";
/** The foot of the app icon tile's paper gradient. Not a theme token — it
 *  exists only to keep the tile from reading as a flat rectangle. */
const PAPER_SHADE = "#EEE5D4";
const INK = "#14202B";
/**
 * The tray's "Colored" theme sits on an uncontrolled menu bar. This fallback
 * measures 5.41:1 on white and 3.88:1 on black, clearing the 3:1 non-text floor
 * on both rather than assuming a known background.
 */
const TRAY_FALLBACK = "#2E6F9E";

const ACCENT = "#0B5F8A";
const ACCENT_STROKE = "#084A6C";

/**
 * Status colours for the ambient tray states (idle aside). Both match
 * src/shorthand/brand/theme.css exactly, so the tray reads the same "live" /
 * "working" vocabulary as the rest of the UI.
 *
 * Recording is `--brand-highlighter`, theme-invariant like the token itself —
 * coral already means "happening now" everywhere else in the app.
 * Transcribing is `--color-logo-primary`, and *is* theme-dependent (the accent
 * flips for contrast against light vs dark), so it needs the same light/dark
 * pair as PAPER/INK below.
 */
const RECORDING = "#F3673C";
const TRANSCRIBING_ON_DARK_TRAY = "#63B7D6";
const TRANSCRIBING_ON_LIGHT_TRAY = ACCENT;

/**
 * Warning fills the pen as well as the bird, in `--color-warning` — upstream's
 * token, whose light/dark pair is used here against the light/dark tray. Amber
 * is the one hue BRANDING.md reserves for warning and refuses the highlighter,
 * so it cannot be confused with the coral "recording" state.
 *
 * Filling both regions rather than one is what separates warning from the
 * ambient states at a glance: recording and transcribing tint the bird alone,
 * so a mark whose pen has also gone colour is unambiguous even before the hue
 * registers.
 */
const WARNING_ON_DARK_TRAY = "#FBBF24";
const WARNING_ON_LIGHT_TRAY = "#D97706";

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
 *
 * `fills` optionally paints the hollow interiors behind the line art — `body`
 * for the bird, `pen` for the barrel; see BIRD_BODY_PATH / PEN_BARREL_PATH.
 * They go first so the mark's own rings, eye and nib all draw over the top of
 * them and keep their `fill` colour.
 */
function mark(box, scale, fill, dx = 0, dy = 0, fills = {}) {
  const visibleSize = box * scale;
  const s = visibleSize / Math.max(MARK_BOUNDS.width, MARK_BOUNDS.height);
  const offX = (box - MARK_BOUNDS.width * s) / 2 - MARK_BOUNDS.minX * s + dx;
  const offY = (box - MARK_BOUNDS.height * s) / 2 - MARK_BOUNDS.minY * s + dy;
  const interiors = [
    [BIRD_BODY_PATH, fills.body],
    [PEN_BARREL_PATH, fills.pen],
  ]
    .filter(([, colour]) => colour)
    .map(([d, colour]) => `<path d="${d}" fill="${colour}"/>`)
    .join("");
  const paths = MARK_PATHS.map(({ d, fillRule }) => {
    const fillRuleAttribute = fillRule ? ` fill-rule="${fillRule}"` : "";
    return `<path d="${d}" fill="${fill}"${fillRuleAttribute}/>`;
  }).join("");
  return `<g transform="translate(${offX} ${offY}) scale(${s})">${interiors}${paths}</g>`;
}

function svg(box, body) {
  return `<svg xmlns="http://www.w3.org/2000/svg" width="${box}" height="${box}" viewBox="0 0 ${box} ${box}">${body}</svg>`;
}

/**
 * Idle, Recording and Transcribing are the mark at full size. The line art
 * itself stays theme ink in every state; what changes is the bird's *body*,
 * which fills with the status colour — see RECORDING / TRANSCRIBING_ON_*_TRAY
 * and BIRD_BODY_PATH above.
 *
 * Two earlier versions are worth not re-proposing. The first shrank the mark
 * to 62/64 units and put a status badge in the freed strip, on the theory that
 * the badge's shape would read the way upstream's hand -> ear -> brain glyph
 * swap does; it cost the mark most of its size for a signal a colour carries
 * at a glance. The second recoloured the entire silhouette, pen included,
 * which fixed the size but made the whole mark change identity on every state
 * change — and a fully coral bird-and-pen reads as a different logo rather
 * than the same one, busy.
 *
 * Filling only the body keeps the mark's outline constant, so the tray item
 * stays recognisably Shorthand while the body carries the state. It also
 * survives the 16px menu-bar size, where the body is the largest single
 * enclosed area in the drawing and therefore the one that still shows colour.
 *
 * Warning fills the pen as well, in amber — see WARNING_ON_*_TRAY. It used to
 * shrink the mark and add an exclamation badge in the freed strip; that is
 * gone for the same reason the status badges are, and because filling a second
 * region already distinguishes it from the one-region ambient states without
 * costing the mark any size.
 */
const TRAY_BOX = 64;

/**
 * The tray mark is drawn WIDER than its frame and allowed to bleed off the
 * right edge. That is deliberate, and it is the only way to make it bigger.
 *
 * The mark is 1.4:1 landscape (measured: 112x80 of a 128 canvas, and those
 * bounds are tight — there is no padding to reclaim). Fitted whole inside a
 * square tray slot it can only ever fill 69% of the height, and the leftover
 * sits above and below as empty frame, which is what makes it read small
 * against neighbouring icons. Cropping to the bird alone does not help: with
 * its wing and tail flourishes the bird is 1.61:1, wider still.
 *
 * So the mark is scaled until its HEIGHT nearly fills the frame and the
 * overflow is spent off one edge. Left-aligned, not centred: the nib, head and
 * eye are what identify the mark, and they all live at the left, so the cut
 * lands on the wing and tail — shapes that read as continuing past the frame
 * rather than as damage.
 *
 * 84 units wide (91% of frame height) is the ceiling. Past roughly this the
 * wing's feathers slice into a flat vertical edge that stops reading as a
 * bleed. It is chosen for 16-24px, which is where a tray icon actually lives
 * (32px at 200% DPI is the ceiling); the flat edge is visible if the SVG is
 * inspected large, but nothing renders it that way.
 */
const TRAY_MARK_WIDTH = 84;
const TRAY_MARK_SCALE = TRAY_MARK_WIDTH / TRAY_BOX;
/** Keeps the nib a hair clear of the frame so it never clips on the left. */
const TRAY_MARK_LEFT_MARGIN = 1;

/**
 * Every tray state: the mark at TRAY_MARK_WIDTH, left-aligned and bleeding off
 * the right, its line art drawn in `color`. `fills` tints the hollow interiors
 * — `{ body }` for the ambient states, `{ body, pen }` for warning, and nothing
 * at all for idle.
 */
function stateMark(color, fills = {}) {
  // `mark()` centres by default; shift left by however much that centring
  // would have inset the (now oversized) drawing.
  const s = TRAY_MARK_WIDTH / MARK_BOUNDS.width;
  const dx = TRAY_MARK_LEFT_MARGIN - (TRAY_BOX - MARK_BOUNDS.width * s) / 2;
  return svg(TRAY_BOX, mark(TRAY_BOX, TRAY_MARK_SCALE, color, dx, 0, fills));
}

/**
 * The installed app icon: the full-colour mark on a paper tile.
 *
 * **The tile is paper, not ink.** The bird's body is the same ocean blue as
 * `ACCENT`, so on the ink tile the head and back dissolve into the background
 * and the icon reads as a coral wing floating on blue. Paper separates every
 * part of the drawing, and it is the ground the artwork was rendered against —
 * its baked-in ambient shadow sits naturally on cream and reads as a halo on a
 * dark tile.
 *
 * **The mark is contained, not bled.** The tray bleeds its mark off one edge
 * because that mark is 1.76:1 and would otherwise sit in a square with empty
 * bands above and below. This artwork is a different delivery composed for a
 * square frame — the bird is centred over the pen rather than beside it, at
 * 1.55:1 — so it fills the tile at `APP_ICON_MARK_SCALE` without any of the
 * slack that made bleeding worth its cost. Bleeding it as well was tried and
 * simply cropped the tail and nib for nothing.
 *
 * Below roughly 24px the clay detail turns to mush — that is inherent to a
 * photographic render, and is why the tray still uses the flat silhouette,
 * which is the artwork that actually has to survive 16px.
 */
const APP_ICON_MARK_SCALE = 0.94;

function appIcon(box) {
  const inset = box * 0.09;
  const tile = box - inset * 2;
  // Fit by the drawing's alpha bounds, then scale past 1 so the width bleeds.
  const width = tile * APP_ICON_MARK_SCALE;
  const height = (width * COLOUR_MARK_BOUNDS.height) / COLOUR_MARK_BOUNDS.width;
  const s = width / COLOUR_MARK_BOUNDS.width;
  const x = inset + (tile - width) / 2 - COLOUR_MARK_BOUNDS.minX * s;
  const y = inset + (tile - height) / 2 - COLOUR_MARK_BOUNDS.minY * s;
  return svg(
    box,
    `<defs><linearGradient id="g" x1="0" y1="0" x2="0" y2="1">` +
      `<stop offset="0" stop-color="${PAPER}"/><stop offset="1" stop-color="${PAPER_SHADE}"/>` +
      `</linearGradient>` +
      // The mark is contained at the current scale, but it is clipped to the
      // tile anyway: the margin outside the tile has to stay transparent for
      // the platform slicers, and a future scale bump should crop rather than
      // silently paint into it.
      `<clipPath id="tile"><rect x="${inset}" y="${inset}" width="${tile}" height="${tile}" rx="${tile * 0.22}"/></clipPath>` +
      `</defs>` +
      `<g clip-path="url(#tile)">` +
      `<rect x="${inset}" y="${inset}" width="${tile}" height="${tile}" rx="${tile * 0.22}" fill="url(#g)"/>` +
      `<image href="${COLOUR_MARK_DATA_URI}" x="${x}" y="${y}" ` +
      `width="${COLOUR_MARK_CANVAS.width * s}" height="${COLOUR_MARK_CANVAS.height * s}"/>` +
      `</g>`,
  );
}

const TARGETS = [
  // Menu bar / system tray. `*_dark.png` is upstream's name for the *dark
  // glyph* shown on a light tray, not for dark mode — so the plain files get
  // PAPER line art and the `_dark` ones get INK, in every state.
  //
  // Idle is line art alone; the other states tint an interior. Every one of
  // them except idle therefore carries a real second colour, and so has to be
  // rendered non-templated on macOS — see tray.rs.
  { path: "src-tauri/resources/tray_idle.png", box: 64, svg: stateMark(PAPER) },
  {
    path: "src-tauri/resources/tray_idle_dark.png",
    box: 64,
    svg: stateMark(INK),
  },
  {
    path: "src-tauri/resources/tray_recording.png",
    box: 64,
    // Coral is theme-invariant, matching `--brand-highlighter`, so both tray
    // backgrounds get the same body fill and differ only in the line art.
    svg: stateMark(PAPER, { body: RECORDING }),
  },
  {
    path: "src-tauri/resources/tray_recording_dark.png",
    box: 64,
    svg: stateMark(INK, { body: RECORDING }),
  },
  {
    path: "src-tauri/resources/tray_transcribing.png",
    box: 64,
    svg: stateMark(PAPER, { body: TRANSCRIBING_ON_DARK_TRAY }),
  },
  {
    path: "src-tauri/resources/tray_transcribing_dark.png",
    box: 64,
    svg: stateMark(INK, { body: TRANSCRIBING_ON_LIGHT_TRAY }),
  },
  {
    path: "src-tauri/resources/tray_idle_warning.png",
    box: 64,
    svg: stateMark(PAPER, {
      body: WARNING_ON_DARK_TRAY,
      pen: WARNING_ON_DARK_TRAY,
    }),
  },
  {
    path: "src-tauri/resources/tray_idle_warning_dark.png",
    box: 64,
    svg: stateMark(INK, {
      body: WARNING_ON_LIGHT_TRAY,
      pen: WARNING_ON_LIGHT_TRAY,
    }),
  },
  // The tray's "Colored" theme (Linux): the fallback blue for line art on an
  // uncontrolled background, with the same status fills as elsewhere.
  {
    path: "src-tauri/resources/handy.png",
    box: 64,
    svg: stateMark(TRAY_FALLBACK),
  },
  {
    path: "src-tauri/resources/recording.png",
    box: 64,
    svg: stateMark(TRAY_FALLBACK, { body: RECORDING }),
  },
  {
    path: "src-tauri/resources/transcribing.png",
    box: 64,
    svg: stateMark(TRAY_FALLBACK, { body: TRANSCRIBING_ON_DARK_TRAY }),
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
