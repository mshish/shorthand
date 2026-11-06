import { describe, expect, test } from "bun:test";
import { windowSizeFor } from "./overlayFitPolicy";

const pill = { width: 186, height: 42 };
const working = { width: 218, height: 42 };
const panel = { width: 394, height: 158 };

describe("windowSizeFor", () => {
  test("grows to the target before the card gets there", () => {
    const { size, settled } = windowSizeFor(panel, pill, pill);
    expect(size).toEqual(panel);
    expect(settled).toBe(true);
  });

  test("a window that already fits the target does not shrink and chase the card", () => {
    // Recording to transcribing reuses the element, which still draws 174 wide.
    const stale = { width: 174, height: 42 };
    const { size } = windowSizeFor(working, working, stale);
    expect(size).toEqual(working);
  });

  test("holds the larger window while the card is still shrinking", () => {
    const mid = { width: 300, height: 100 };
    const { size, settled } = windowSizeFor(pill, panel, mid);
    expect(size).toEqual(panel);
    expect(settled).toBe(false);
  });

  test("grows one axis and holds the other", () => {
    const { size } = windowSizeFor(
      { width: 394, height: 60 },
      { width: 300, height: 158 },
      { width: 300, height: 100 },
    );
    expect(size).toEqual({ width: 394, height: 158 });
  });

  test("shrinks once the card has arrived", () => {
    const { size, settled } = windowSizeFor(pill, panel, pill);
    expect(size).toEqual(pill);
    expect(settled).toBe(true);
  });

  test("shrinks when forced, for a transition that never ends", () => {
    const { size } = windowSizeFor(pill, panel, panel, true);
    expect(size).toEqual(pill);
  });

  test("an unknown window size sends the target", () => {
    expect(windowSizeFor(pill, null, panel).size).toEqual(pill);
  });
});
