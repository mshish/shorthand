import { describe, expect, test } from "bun:test";
import { CURATED_ASR } from "@/lib/models/catalog";
import {
  DEFAULT_MODEL_FILTERS,
  hiddenByFilters,
  holdsModel,
  isTableModelVisible,
  passesModelFilters,
  shorthandPicksFor,
  yoursSection,
} from "./modelVisibility";

const streams = (m: { capabilities: { streaming: boolean } }) =>
  m.capabilities.streaming;
const everything = () => true;

describe("shorthandPicksFor", () => {
  // The languages the onboarding table covers: upstream's own picks for es,
  // fr, de, ja, zh and en+es were non-streaming, and en had a non-streaming
  // extra pick.
  const cases: string[][] = [
    ["en"],
    ["hi"],
    ["es"],
    ["fr"],
    ["de"],
    ["ja"],
    ["zh"],
    ["en", "es"],
  ];
  for (const languages of cases) {
    test(`${languages.join("+")}: every pick streams`, () => {
      const { picks, liveNotes } = shorthandPicksFor(
        languages,
        everything,
        streams,
      );
      expect(liveNotes).toBe(true);
      expect(picks.length > 0).toBe(true);
      for (const p of picks) expect(p.model.capabilities.streaming).toBe(true);
      expect(picks.some((p) => p.role === "pick")).toBe(true);
    });
  }

  test("th: no streaming model covers it, so falls back and warns", () => {
    const { picks, liveNotes } = shorthandPicksFor(
      "th".split(","),
      everything,
      streams,
    );
    expect(liveNotes).toBe(false);
    expect(picks.length > 0).toBe(true);
    expect(picks.every((p) => !p.model.capabilities.streaming)).toBe(true);
  });

  // The streaming model is only rated unsupported (hu, et) or experimental
  // (the rest) for these, so it is not offered as the recommendation.
  for (const language of [
    "hu",
    "et",
    "da",
    "ro",
    "bg",
    "cs",
    "fi",
    "hr",
    "sk",
    "sv",
  ]) {
    test(`${language}: a streaming model rated too weak falls back and warns`, () => {
      const { picks, liveNotes } = shorthandPicksFor(
        [language],
        everything,
        streams,
      );
      expect(liveNotes).toBe(false);
      expect(
        picks.every(
          (p) => p.status !== "unsupported" && p.status !== "experimental",
        ),
      ).toBe(true);
    });
  }

  test("respects what the app offers", () => {
    const streaming = CURATED_ASR.filter(streams);
    const only = streaming[0];
    const { picks } = shorthandPicksFor(
      ["en"],
      (m) => m.id === only.id,
      streams,
    );
    expect(picks.every((p) => p.model.id === only.id)).toBe(true);
  });
});

describe("isTableModelVisible", () => {
  const quiet = { streaming: false };
  test("hides a non-streaming model the user does not hold", () => {
    expect(isTableModelVisible(quiet, false, false)).toBe(false);
  });
  test("shows a streaming model", () => {
    expect(isTableModelVisible({ streaming: true }, false, false)).toBe(true);
  });
  test("never hides a model that is held, streaming or not", () => {
    expect(isTableModelVisible(quiet, false, true)).toBe(true);
  });
  test("show_all_settings shows everything", () => {
    expect(isTableModelVisible(quiet, true, false)).toBe(true);
  });
});

describe("passesModelFilters", () => {
  const live = { streaming: true, translate: false };
  const quiet = { streaming: false, translate: false };
  const on = { preview: true, translate: false };
  const off = { preview: false, translate: false };

  test("Settings > Model starts with Live preview pressed and no search", () => {
    expect(DEFAULT_MODEL_FILTERS).toEqual({
      query: "",
      preview: true,
      translate: false,
    });
  });
  test("Live preview hides a non-streaming model the user does not hold", () => {
    expect(passesModelFilters(quiet, on, false)).toBe(false);
  });
  test("Live preview keeps a streaming model", () => {
    expect(passesModelFilters(live, on, false)).toBe(true);
  });
  test("Live preview never hides a held model: on disk, downloading, current, custom", () => {
    expect(passesModelFilters(quiet, on, true)).toBe(true);
  });
  test("turning Live preview off shows everything", () => {
    expect(passesModelFilters(quiet, off, false)).toBe(true);
    expect(passesModelFilters(live, off, false)).toBe(true);
  });
  test("Translate is strict, held or not", () => {
    const filters = { preview: false, translate: true };
    expect(passesModelFilters(quiet, filters, true)).toBe(false);
    expect(
      passesModelFilters({ ...quiet, translate: true }, filters, false),
    ).toBe(true);
  });
  test("both pressed: a held non-translating model still goes", () => {
    expect(
      passesModelFilters(live, { preview: true, translate: true }, true),
    ).toBe(false);
  });
});

describe("holdsModel", () => {
  test("on disk, downloading and verifying are held", () => {
    expect(holdsModel({ state: "ready" })).toBe(true);
    expect(holdsModel({ state: "checking" })).toBe(true);
    expect(
      holdsModel({
        state: "downloading",
        progress: 0,
        etaSeconds: null,
        downloaded: 0,
        total: 0,
        bytesPerSec: null,
      }),
    ).toBe(true);
  });
  test("nothing and a failed download are not", () => {
    expect(holdsModel({ state: "none" })).toBe(false);
    expect(holdsModel({ state: "failed" })).toBe(false);
  });
});

describe("hiddenByFilters", () => {
  const whisper = { streaming: false, translate: true };
  const parakeet = { streaming: true, translate: false };
  const notHeld = () => false;
  const all = () => true;

  test("names Live preview when it hides a match", () => {
    expect(
      hiddenByFilters([whisper], DEFAULT_MODEL_FILTERS, notHeld, all),
    ).toEqual({ preview: true, translate: false });
  });

  test("a held model is not hidden by Live preview", () => {
    expect(
      hiddenByFilters([whisper], DEFAULT_MODEL_FILTERS, () => true, all),
    ).toBe(null);
  });

  test("names Translates when it hides a match", () => {
    expect(
      hiddenByFilters(
        [parakeet],
        { ...DEFAULT_MODEL_FILTERS, preview: false, translate: true },
        () => true,
        all,
      ),
    ).toEqual({ preview: false, translate: true });
  });

  test("models the search does not match are not counted", () => {
    expect(
      hiddenByFilters([whisper], DEFAULT_MODEL_FILTERS, notHeld, () => false),
    ).toBe(null);
  });
});

describe("yoursSection at rest", () => {
  test("shows its rows", () => {
    expect(yoursSection(3, 3)).toBe("rows");
  });

  test("says no models yet only when none are held", () => {
    expect(yoursSection(0, 0)).toBe("empty");
  });

  test("Translates with only non-translating models held drops the section", () => {
    const held = [{ streaming: true, translate: false }];
    const shown = held.filter((e) =>
      passesModelFilters(
        e,
        { ...DEFAULT_MODEL_FILTERS, translate: true },
        true,
      ),
    );
    expect(yoursSection(held.length, shown.length)).toBe("hidden");
  });
});
