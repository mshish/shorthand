import { useCallback, useLayoutEffect, useRef } from "react";
import { commands } from "@/bindings";

interface Size {
  width: number;
  height: number;
}

const sameSize = (a: Size | null, b: Size) =>
  a !== null && a.width === b.width && a.height === b.height;

/**
 * Keeps the overlay's native window the size of its card. The window is
 * transparent but still takes clicks, so any part of it that the card doesn't
 * cover blocks the app beneath. The returned ref goes on the card; its
 * border-box size (layout size, so the pop-in scale doesn't count) is sent to
 * the backend, which grows or shrinks the window around the anchored edge.
 *
 * Resizes are coalesced: an observer callback fires at most once per frame, one
 * request is in flight at a time, and only the newest size waits behind it.
 * `showCount` must change on every native show, which resets the window to its
 * fixed first size, so the card's current size is sent again even when it
 * hasn't changed.
 */
export function useOverlayFit(showCount: number) {
  const elementRef = useRef<HTMLElement | null>(null);
  const observerRef = useRef<ResizeObserver | null>(null);
  const sentRef = useRef<Size | null>(null);
  const pendingRef = useRef<Size | null>(null);
  const inFlightRef = useRef(false);
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

  const ref = useCallback(
    (element: HTMLElement | null) => {
      observerRef.current?.disconnect();
      observerRef.current = null;
      elementRef.current = element;
      measuredRef.current = null;
      if (!element) return;
      const observer = new ResizeObserver((entries) => {
        const box = entries[entries.length - 1]?.borderBoxSize?.[0];
        if (!box) return;
        measuredRef.current = { width: box.inlineSize, height: box.blockSize };
        report(measuredRef.current);
      });
      observer.observe(element, { box: "border-box" });
      observerRef.current = observer;
    },
    [report],
  );

  useLayoutEffect(() => {
    sentRef.current = null;
    const element = elementRef.current;
    if (element) {
      report(
        measuredRef.current ?? {
          width: element.offsetWidth,
          height: element.offsetHeight,
        },
      );
    }
  }, [showCount, report]);

  return ref;
}
