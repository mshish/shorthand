import { useCallback, useLayoutEffect, useRef } from "react";
import { commands } from "@/bindings";
import { windowSizeFor, type Size } from "./overlayFitPolicy";

const sameSize = (a: Size | null, b: Size) =>
  a !== null && a.width === b.width && a.height === b.height;

/** How long a window held larger than its card waits for the card to arrive. */
const SETTLE_TIMEOUT_MS = 700;

/**
 * Marks a subtree whose changes never resize the card. The waveform's bars are
 * restyled about 30 times a second; measuring the card for each one copies and
 * lays out the whole card, transcript included, at that rate.
 */
export const FIT_IGNORE_ATTR = "data-fit-ignore";

const ignored = (node: EventTarget | null) => {
  if (!(node instanceof Node)) return false;
  const element = node instanceof Element ? node : node.parentElement;
  return !!element?.closest(`[${FIT_IGNORE_ATTR}]`);
};

/**
 * The size the card is heading for. Its drawn size is mid-transition, so this
 * measures a copy of it, inserted beside it with its own transitions and
 * animation off: a copy has no earlier style to transition from, so it lays
 * out at the end state at once. It is inserted and removed in one task and
 * never painted.
 */
function measureTarget(element: HTMLElement): Size | null {
  const parent = element.parentElement;
  if (!parent) return null;
  const copy = element.cloneNode(true) as HTMLElement;
  Object.assign(copy.style, {
    transition: "none",
    animation: "none",
    transform: "none",
    position: "absolute",
    visibility: "hidden",
    pointerEvents: "none",
  });
  parent.appendChild(copy);
  try {
    const box = copy.getBoundingClientRect();
    return { width: box.width, height: box.height };
  } finally {
    copy.remove();
  }
}

/**
 * Keeps the overlay's native window the size of its card. The window is
 * transparent but still takes clicks, so any part of it that the card doesn't
 * cover blocks the app beneath. The returned ref goes on the card; the backend
 * grows or shrinks the window around the anchored edge.
 *
 * The card morphs over several hundred milliseconds, so the window follows its
 * target size, not its drawn size (see `windowSizeFor`): it grows at once, as
 * soon as a class, style or child change sets a new target, and shrinks once
 * the card has arrived, which a `transitionend` or the observer reports. A
 * timeout covers a transition that never ends. While the card shrinks the
 * window is briefly larger than it, which is harmless; a clipped card is not.
 * At rest the window equals the drawn card, so clicks outside it pass through.
 *
 * Requests are coalesced: one is in flight at a time, and only the newest size
 * waits behind it. `showCount` must change on every native show, which resets
 * the window to its fixed first size, so the target is sent again even when it
 * hasn't changed.
 */
export function useOverlayFit(showCount: number) {
  const elementRef = useRef<HTMLElement | null>(null);
  const cleanupRef = useRef<(() => void) | null>(null);
  const sentRef = useRef<Size | null>(null);
  const pendingRef = useRef<Size | null>(null);
  const inFlightRef = useRef(false);
  const timerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  // The observer's latest exact size for the current element; offsetWidth and
  // offsetHeight round, which can undercut a fractional card and clip it.
  const measuredRef = useRef<Size | null>(null);

  const flush = useCallback(() => {
    const next = pendingRef.current;
    if (!next || inFlightRef.current) return;
    pendingRef.current = null;
    if (sameSize(sentRef.current, next)) return;
    sentRef.current = next;
    inFlightRef.current = true;
    commands
      .fitOverlay(next.width, next.height)
      .catch(() => {
        // The window keeps its last size; the next change sends again.
        sentRef.current = null;
      })
      .finally(() => {
        inFlightRef.current = false;
        flush();
      });
  }, []);

  const report = useCallback(
    (size: Size) => {
      pendingRef.current = size;
      flush();
    },
    [flush],
  );

  const clearTimer = () => {
    if (timerRef.current !== null) clearTimeout(timerRef.current);
    timerRef.current = null;
  };

  const evaluate = useCallback(
    (force = false) => {
      const element = elementRef.current;
      if (!element) return;
      const target = measureTarget(element) ?? measuredRef.current;
      if (!target) return;
      const { size, settled } = windowSizeFor(
        target,
        pendingRef.current ?? sentRef.current,
        measuredRef.current,
        force,
      );
      report(size);
      if (settled) {
        clearTimer();
      } else if (timerRef.current === null) {
        // The card has not reached its target and no event may say when it
        // does (reduced motion, a cancelled transition): stop holding.
        timerRef.current = setTimeout(() => {
          timerRef.current = null;
          evaluate(true);
        }, SETTLE_TIMEOUT_MS);
      }
    },
    [report],
  );

  const ref = useCallback(
    (element: HTMLElement | null) => {
      cleanupRef.current?.();
      cleanupRef.current = null;
      elementRef.current = element;
      measuredRef.current = null;
      if (!element) return;
      const resize = new ResizeObserver((entries) => {
        const box = entries[entries.length - 1]?.borderBoxSize?.[0];
        if (!box) return;
        measuredRef.current = { width: box.inlineSize, height: box.blockSize };
        evaluate();
      });
      resize.observe(element, { box: "border-box" });
      // A new state sets a new target before the card moves toward it.
      const mutation = new MutationObserver((records) => {
        if (records.some((record) => !ignored(record.target))) evaluate();
      });
      mutation.observe(element, {
        attributes: true,
        childList: true,
        subtree: true,
        characterData: true,
      });
      const onTransitionEnd = (event: Event) => {
        if (!ignored(event.target)) evaluate();
      };
      element.addEventListener("transitionend", onTransitionEnd);
      element.addEventListener("transitioncancel", onTransitionEnd);
      cleanupRef.current = () => {
        resize.disconnect();
        mutation.disconnect();
        element.removeEventListener("transitionend", onTransitionEnd);
        element.removeEventListener("transitioncancel", onTransitionEnd);
        clearTimer();
      };
    },
    [evaluate],
  );

  useLayoutEffect(() => {
    // The window is back at its fixed first size, which is not what was sent.
    sentRef.current = null;
    pendingRef.current = null;
    evaluate();
  }, [showCount, evaluate]);

  return ref;
}
