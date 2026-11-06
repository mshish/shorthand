import { describe, expect, test } from "bun:test";
import forkEn from "./locales/en.json";
import { historySourceLabelKey } from "./historySource";

describe("historySourceLabelKey", () => {
  test("a dropped file has its own label", () => {
    expect(historySourceLabelKey("file")).toBe("settings.history.source.file");
  });
  test("the capture modes keep theirs", () => {
    expect(historySourceLabelKey("dictation")).toBe(
      "settings.history.source.dictation",
    );
    expect(historySourceLabelKey("assisted_notes")).toBe(
      "settings.history.source.assistedNotes",
    );
    expect(historySourceLabelKey("meeting")).toBe(
      "settings.history.source.meeting",
    );
  });
  test("rows from before this value, or from a newer version, read as Meeting", () => {
    expect(historySourceLabelKey("")).toBe("settings.history.source.meeting");
    expect(historySourceLabelKey("from_the_future")).toBe(
      "settings.history.source.meeting",
    );
  });
  test("every label has a fork string", () => {
    const keys = ["file", "dictation", "assisted_notes", "meeting"].map(
      historySourceLabelKey,
    );
    for (const key of keys)
      expect(typeof (forkEn as Record<string, string>)[key]).toBe("string");
  });
});
