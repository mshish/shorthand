import { describe, expect, test } from "bun:test";
import { TAIL_KEEP, TAIL_MAX, tailStart } from "./overlayTail";

const words = (chars: number) => "word ".repeat(Math.ceil(chars / 5));

describe("tailStart", () => {
  test("keeps short text whole", () => {
    expect(tailStart(words(TAIL_MAX - 5), 0)).toBe(0);
  });

  test("cuts long text back to the tail, at a word start", () => {
    const text = words(TAIL_MAX + 500);
    const start = tailStart(text, 0);
    expect(text.length - start <= TAIL_KEEP).toBe(true);
    expect(text.length - start > TAIL_KEEP - 40).toBe(true);
    expect(text[start - 1]).toBe(" ");
    expect(text[start]).toBe("w");
  });

  test("holds the start between cuts, so the visible lines don't re-wrap", () => {
    let text = words(TAIL_MAX + 500);
    const start = tailStart(text, 0);
    text += words(TAIL_MAX - TAIL_KEEP - 100);
    expect(tailStart(text, start)).toBe(start);
    text += words(200);
    expect(tailStart(text, start) > start).toBe(true);
  });

  test("starts over when the text is shorter than the last start", () => {
    expect(tailStart("a new capture", 2500)).toBe(0);
  });

  test("cuts text without spaces, but not inside a surrogate pair", () => {
    const text = "𠮷".repeat(TAIL_MAX);
    const start = tailStart(text, 0);
    expect(text.length - start <= TAIL_KEEP).toBe(true);
    expect(text.codePointAt(start)).toBe("𠮷".codePointAt(0));
  });
});
