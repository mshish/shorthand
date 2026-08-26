/**
 * Fork-only. Proves that reorganising the fork-string mechanism does not
 * change a rendered string in any locale — except where a task deliberately
 * changes one and regenerates the golden file.
 *
 * `bun test` is the runner deliberately: it ships inside the Bun binary this
 * repo already requires, so unlike vitest it adds nothing to upstream's
 * package.json or bun.lock. See docs/FRONTEND_TESTING.md.
 */

import fs from "fs";
import path from "path";
import { describe, expect, test } from "bun:test";
import golden from "./branding.golden.json";
import { applyBranding, forkStringsFor } from "./branding";
import englishCopy from "./english-copy.json";
import { hashLocale, localeNames } from "../../scripts/write-branding-golden";

describe("applyBranding", () => {
  test("every upstream locale is present in the golden file", () => {
    expect(localeNames().sort()).toEqual(Object.keys(golden).sort());
  });

  for (const locale of localeNames()) {
    test(`${locale} renders byte-identically to the golden hash`, () => {
      expect(hashLocale(locale)).toBe(
        (golden as Record<string, string>)[locale],
      );
    });
  }
});

const read = (locale: string) =>
  JSON.parse(
    fs.readFileSync(
      path.join(
        import.meta.dir,
        "..",
        "i18n",
        "locales",
        locale,
        "translation.json",
      ),
      "utf8",
    ),
  );

const get = (obj: unknown, dotted: string): unknown =>
  dotted
    .split(".")
    .reduce<unknown>(
      (node, key) =>
        node && typeof node === "object"
          ? (node as Record<string, unknown>)[key]
          : undefined,
      obj,
    );

describe("English copy rules", () => {
  test("apply to English", () => {
    const { translation } = applyBranding(read("en"), "en");
    expect(get(translation, "theme.title")).toBe("Application theme");
  });

  test("do not overwrite a German translation", () => {
    const { translation } = applyBranding(read("de"), "de");
    expect(get(translation, "theme.title")).toBe("Anwendungsdesign");
    expect(get(translation, "appLanguage.title")).toBe("Anwendungssprache");
  });

  test("no locale but en receives any english-copy value", () => {
    for (const locale of localeNames()) {
      if (locale === "en") continue;
      const raw = read(locale);
      const { translation } = applyBranding(raw, locale);
      for (const [key, englishValue] of Object.entries(englishCopy)) {
        const rawValue = get(raw, key);
        const rendered = get(translation, key);
        // Compare against upstream's own (pre-merge) value, not mere string
        // equality with englishValue: da's own upstream translation of
        // settings.debug.liveLogs.title was never localised and happens to
        // already read "Live logs", identical to our sentence-case override,
        // with no merge involved. Flagging that as "overwritten" would be a
        // false positive — the real signature of an overwrite is upstream
        // having had its own distinct value that then turned English.
        if (
          rawValue !== undefined &&
          rawValue !== englishValue &&
          rendered === englishValue
        ) {
          throw new Error(
            `${locale}:${key} was overwritten with the English "${englishValue}"`,
          );
        }
      }
    }
  });

  test("no english-copy value reintroduces the upstream brand name", () => {
    for (const value of Object.values(englishCopy)) {
      expect(value).not.toMatch(/\bHandy\b/);
    }
  });

  test("the transcribe shortcut's fork description is restored for en only", () => {
    const en = applyBranding(read("en"), "en").translation;
    expect(
      get(en, "settings.general.shortcut.bindings.transcribe.description"),
    ).toBe("The keyboard shortcut to record and transcribe a meeting or note.");

    const de = applyBranding(read("de"), "de").translation;
    expect(
      get(de, "settings.general.shortcut.bindings.transcribe.description"),
    ).not.toBe("The keyboard shortcut to record and transcribe a meeting or note.");
  });
});

describe("forkStringsFor", () => {
  test("returns the English catalogue for en", () => {
    expect(forkStringsFor("en")["sidebar.modes"]).toBe("Modes");
  });

  test("falls back to English for a locale with no catalogue", () => {
    expect(forkStringsFor("de")["sidebar.modes"]).toBe("Modes");
  });

  test("every locale gets the full English key set", () => {
    const en = Object.keys(forkStringsFor("en")).sort();
    expect(Object.keys(forkStringsFor("ja")).sort()).toEqual(en);
  });
});
