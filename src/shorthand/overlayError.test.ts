import { describe, expect, test } from "bun:test";
import i18next from "i18next";
import upstreamEn from "../i18n/locales/en/translation.json";
import { applyBranding } from "./branding";
import forkEn from "./locales/en.json";
import { OVERLAY_ERROR_KINDS, overlayErrorKeys } from "./overlayError";

describe("overlayErrorKeys", () => {
  test("a restart message keeps its second line", () => {
    expect(overlayErrorKeys("mic_failed", false)).toEqual([
      "overlay.error.micFailed.message",
      "overlay.error.micFailed.action",
    ]);
  });
  test("a one-line message has no second line", () => {
    expect(overlayErrorKeys("mic_missing", false)).toEqual([
      "overlay.error.micMissing",
    ]);
  });
  test("a saved recording adds the History line last", () => {
    expect(overlayErrorKeys("transcribe_failed", true)).toEqual([
      "overlay.error.transcribeFailed",
      "overlay.error.savedInHistory",
    ]);
  });
  test("an unknown kind gets the generic line", () => {
    expect(overlayErrorKeys("something_new", false)).toEqual([
      "overlay.error.generic",
    ]);
  });
});

// Fork keys are flat but are expanded into upstream's nested catalogue, so a
// key that is both a string and a parent of another key loses its string.
// Resolve every key through the same merge the bundle uses and real i18next.
describe("overlay error copy", () => {
  test("every key resolves to its en.json string", async () => {
    const { translation } = applyBranding(upstreamEn, "en");
    const i18n = i18next.createInstance();
    await i18n.init({ lng: "en", resources: { en: { translation } } });
    const fork = forkEn as Record<string, string>;

    for (const kind of [...OVERLAY_ERROR_KINDS, "something_new"]) {
      for (const saved of [false, true]) {
        for (const key of overlayErrorKeys(kind, saved)) {
          expect(typeof fork[key]).toBe("string");
          expect(i18n.t(key)).toBe(fork[key]);
        }
      }
    }
  });
});
