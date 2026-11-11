/** How much of a lane's committed text the Live panel keeps after a cut. */
export const TAIL_KEEP = 2000;
/** The shown text is cut back to `TAIL_KEEP` once it grows past this. */
export const TAIL_MAX = 4000;

/** How far past the cut to look for a word boundary before cutting mid-word. */
const WORD_SEARCH = 40;

const isLowSurrogate = (code: number) => code >= 0xdc00 && code <= 0xdfff;

/**
 * Where the Live panel's copy of a lane's committed text starts.
 *
 * A lane's committed text grows for the whole capture, and the panel shows a
 * few lines of it. Laying out all of it on every update costs more the longer
 * a meeting runs, so the panel keeps only the tail.
 *
 * The start moves in steps, not with every update: the paragraph wraps from
 * its first character, so each move re-wraps the visible lines. `start` is the
 * value returned for this lane last time, 0 for a new capture.
 */
export function tailStart(text: string, start: number): number {
  // Shorter than the last start: a new capture, or a rewritten prefix.
  if (start > text.length) start = 0;
  if (text.length - start <= TAIL_MAX) return start;
  const cut = text.length - TAIL_KEEP;
  const space = text.indexOf(" ", cut);
  if (space !== -1 && space - cut < WORD_SEARCH) return space + 1;
  // No spaces nearby (CJK): cut here, but not between a surrogate pair.
  return isLowSurrogate(text.charCodeAt(cut)) ? cut + 1 : cut;
}
