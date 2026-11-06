import { describe, expect, test } from "bun:test";
import { CURATED_ASR } from "@/lib/models/catalog";
import {
  holdsModel,
  isTableModelVisible,
  shorthandPicksFor,
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
