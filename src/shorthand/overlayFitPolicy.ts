/** A card or window size in CSS pixels. */
export interface Size {
  width: number;
  height: number;
}

/** Layout rounding noise, not a real difference in size. */
const TOLERANCE = 0.5;

const same = (a: Size, b: Size) =>
  Math.abs(a.width - b.width) <= TOLERANCE &&
  Math.abs(a.height - b.height) <= TOLERANCE;

/**
 * The window size to request while the card changes.
 *
 * The card morphs over several hundred milliseconds, so its drawn size trails
 * its target by that long. A window that follows the drawn size clips the card
 * for as long as the report and the resize take. So the window grows to the
 * target at once, and shrinks only once the card has reached it (`live` is the
 * card's drawn size now), or when `force` says the wait is over.
 *
 * `current` is the size last asked for, or null when the window's size is not
 * known (just shown, or the last request failed), which sends the target.
 * `settled` is false while the window is held larger than the target, which is
 * when the caller should arm a timeout for a transition that never ends.
 */
export function windowSizeFor(
  target: Size,
  current: Size | null,
  live: Size | null,
  force = false,
): { size: Size; settled: boolean } {
  if (force || !current || !live || same(live, target)) {
    return { size: target, settled: true };
  }
  const size = {
    width: Math.max(target.width, current.width),
    height: Math.max(target.height, current.height),
  };
  return { size, settled: same(size, target) };
}
