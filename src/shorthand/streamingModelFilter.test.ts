/**
 * Fork-only. Bun coverage for the streaming-chip decisions in
 * `streamingModelFilter.ts`. See that file's header for why these two
 * functions exist separately from `modelVisibility.ts`.
 */

import { describe, expect, test } from "bun:test";
import {
  isStreamingFilterExempt,
  resolveStreamingFilter,
  type StreamingFilterModel,
} from "./streamingModelFilter";

describe("resolveStreamingFilter", () => {
  test("no override, hatch off -> filter on", () => {
    expect(resolveStreamingFilter(null, false)).toBe(true);
  });

  test("no override, hatch on -> filter off", () => {
    expect(resolveStreamingFilter(null, true)).toBe(false);
  });

  test("explicit override true wins with hatch off", () => {
    expect(resolveStreamingFilter(true, false)).toBe(true);
  });

  test("explicit override false wins with hatch off", () => {
    expect(resolveStreamingFilter(false, false)).toBe(false);
  });

  test("explicit override true wins with hatch on", () => {
    expect(resolveStreamingFilter(true, true)).toBe(true);
  });

  test("explicit override false wins with hatch on", () => {
    expect(resolveStreamingFilter(false, true)).toBe(false);
  });
});

describe("isStreamingFilterExempt", () => {
  const base: StreamingFilterModel = {
    id: "some-model",
    is_downloaded: false,
    is_downloading: false,
    is_custom: false,
  };

  test("the current model is exempt", () => {
    expect(isStreamingFilterExempt(base, "some-model")).toBe(true);
  });

  test("a downloading model is exempt", () => {
    expect(
      isStreamingFilterExempt({ ...base, is_downloading: true }, null),
    ).toBe(true);
  });

  test("a custom model is exempt", () => {
    expect(isStreamingFilterExempt({ ...base, is_custom: true }, null)).toBe(
      true,
    );
  });

  test("an unrelated, complete, non-custom model is not exempt", () => {
    expect(isStreamingFilterExempt(base, "other-model")).toBe(false);
  });

  test("is_downloaded alone does not make a model exempt", () => {
    expect(
      isStreamingFilterExempt({ ...base, is_downloaded: true }, null),
    ).toBe(false);
  });
});
