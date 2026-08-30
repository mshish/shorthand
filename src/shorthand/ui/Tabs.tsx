import React, { useRef } from "react";

export interface TabSpec<T extends string> {
  id: T;
  label: string;
}

/**
 * How the bar is drawn. The two variants are deliberately unlike each other,
 * because they are used one inside the other — see the component doc below.
 */
export type TabsVariant = "underline" | "segmented";

interface TabsProps<T extends string> {
  tabs: readonly TabSpec<T>[];
  active: T;
  onChange: (id: T) => void;
  /** Labels the tablist for screen readers. */
  label: string;
  /** Defaults to `underline`, the top-level bar. */
  variant?: TabsVariant;
}

/**
 * The settings tab bar, in two variants.
 *
 * Implements the WAI-ARIA tabs pattern properly — roles, `aria-selected`,
 * `aria-controls`, roving `tabIndex`, and Left/Right/Home/End. Upstream's
 * sidebar rows are bare clickable `<div>`s with no role, no `tabIndex` and no
 * keyboard handler, and copying that here would have added a second
 * unreachable-by-keyboard navigation control rather than one.
 *
 * Both variants share every one of those behaviours; they differ only in how
 * they are painted. That is the point of a variant rather than a second
 * component: there is one keyboard implementation to get right, not two.
 *
 * ## `underline` — the top-level bar
 *
 * The active tab carries a plain coral underline (`sh-tab-indicator`, see
 * brand/marks.css). Coral keeps the same meaning as before — the tab you're
 * looking at is the live one — without the hand-drawn highlighter marquee that
 * previously drew it; that motif read as visual noise on a plain tab bar.
 *
 * ## `segmented` — a bar nested inside a panel of the one above
 *
 * `ModesSettings` nests a second tablist inside the Notetaking panel. Drawn in
 * the `underline` variant it was indistinguishable from its own parent: same
 * size, same weight, same coral rule, same full-bleed hairline, one directly
 * beneath the other. Two controls that look identical and mean different
 * things is the defect this variant exists to fix.
 *
 * So the nested bar is a different *kind* of control rather than a smaller
 * copy of the same one — the conventional segmented control: a filled track
 * with the active item raised out of it as a chip. Nesting an underline inside
 * an underline asks the user to read a size difference; a track and a chip
 * cannot be mistaken for a row of underlined tabs at any size.
 *
 * It is deliberately not coral. brand/marks.css states the rule — one meaning,
 * one motif; `.sh-tab-indicator` marks the active tab and nothing else — and
 * putting the same mark on both levels would have restored the ambiguity by a
 * different route. The chip is drawn in `--color-background`, so the active
 * segment reads as the page showing through the track. That works in both
 * themes without a second token: in light the chip is lighter than its track,
 * in dark it is darker, and either way it is the surface the content below is
 * already on.
 *
 * In neither variant is the paint the only signal. `aria-selected` carries the
 * state programmatically and the label goes semibold, so selection survives
 * greyscale, a screen reader, and `prefers-reduced-motion`.
 */
export function Tabs<T extends string>({
  tabs,
  active,
  onChange,
  label,
  variant = "underline",
}: TabsProps<T>) {
  const refs = useRef<Record<string, HTMLButtonElement | null>>({});

  const move = (delta: number) => {
    const i = tabs.findIndex((tab) => tab.id === active);
    const next = tabs[(i + delta + tabs.length) % tabs.length];
    onChange(next.id);
    refs.current[next.id]?.focus();
  };

  const onKeyDown = (event: React.KeyboardEvent) => {
    switch (event.key) {
      case "ArrowLeft":
        event.preventDefault();
        move(-1);
        break;
      case "ArrowRight":
        event.preventDefault();
        move(1);
        break;
      case "Home":
        event.preventDefault();
        onChange(tabs[0].id);
        refs.current[tabs[0].id]?.focus();
        break;
      case "End": {
        event.preventDefault();
        const last = tabs[tabs.length - 1];
        onChange(last.id);
        refs.current[last.id]?.focus();
        break;
      }
    }
  };

  const segmented = variant === "segmented";

  // `w-fit`, because the track has to hug its labels: stretched to the width of
  // the pane the fill stops reading as a control and starts reading as a
  // banner. It stays a block-level flex row rather than becoming `inline-flex`,
  // so it does not pick up the inline formatting context's baseline alignment
  // and whitespace.
  //
  // The track's `/10` is load-bearing for contrast, not taste. The inactive
  // labels are `--color-mid-gray` sitting *on* this fill, and mid-gray on the
  // page clears AA at 5.33:1 with little to spare; at `/10` the labels are
  // still at 4.74:1, and around `/12` they fall through 4.5:1. Darken the track
  // and you have to stop using mid-gray for the labels.
  const tablistClass = segmented
    ? "flex w-fit items-center gap-1 rounded-lg bg-mid-gray/10 p-1"
    : "flex items-center gap-6 border-b border-mid-gray/20 px-1";

  // Focus is the one place the two variants must not diverge: same ring, same
  // offset, same token, so a keyboard user is not asked to learn two controls.
  const focusRing =
    "focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-logo-primary";

  const itemClass = (selected: boolean) => {
    if (!segmented) {
      return `relative cursor-pointer bg-transparent border-0 pb-3 pt-2 text-sm transition-colors ${focusRing} ${
        selected ? "font-semibold" : "font-medium text-mid-gray hover:text-text"
      }`;
    }
    // `ring` and not `border`: a border on the selected chip only would shift
    // every label by a pixel as the selection moves, which is visible as a
    // twitch when you arrow along the bar.
    //
    // The ring is the whole state indicator, and it is drawn at `/80` rather
    // than the hairline this started as because of WCAG 1.4.11. The chip's
    // *fill* cannot carry the job: it is `--color-background`, and its own
    // track is that same colour plus 10% mid-gray, so chip against track is
    // about 1.1:1 in light and 1.2:1 in dark — the faintest edge on the pane
    // doing the most important thing on it. At `/80` the ring measures 3.2:1
    // against the track in light and 5.2:1 in dark, clearing the 3:1 that
    // non-text contrast asks for. The semibold label and `aria-selected` are
    // still there, but a weight change is not accepted as a substitute for a
    // visible boundary, and a user scanning rather than reading gets nothing
    // from either.
    //
    // No drop shadow. One was tried, and it was wrong twice over: `box-shadow`
    // is not covered by `transition-colors`, so the chip's edge snapped while
    // its fill cross-faded, and in dark the chip is *recessed* relative to its
    // track — a shadow under a surface that sits below its surroundings paints
    // a lift that is not there. It would also have been the only `shadow-sm`
    // in the app.
    return `cursor-pointer border-0 rounded-md px-3 py-1 text-sm transition-colors ${focusRing} ${
      selected
        ? "bg-background text-text font-semibold ring-1 ring-mid-gray/80"
        : "bg-transparent font-medium text-mid-gray hover:text-text"
    }`;
  };

  return (
    <div
      role="tablist"
      aria-label={label}
      onKeyDown={onKeyDown}
      className={tablistClass}
    >
      {tabs.map((tab) => {
        const selected = tab.id === active;
        return (
          <button
            key={tab.id}
            ref={(el) => {
              refs.current[tab.id] = el;
            }}
            role="tab"
            id={`tab-${tab.id}`}
            aria-selected={selected}
            // Only while the panel is actually in the DOM. Panels here are
            // conditionally mounted, so an unconditional `aria-controls` on
            // every tab names an id that does not exist — which axe reports as
            // a violation, and which the nesting doubles: two tablists means
            // two dangling references at all times rather than one.
            aria-controls={selected ? `tabpanel-${tab.id}` : undefined}
            tabIndex={selected ? 0 : -1}
            onClick={() => onChange(tab.id)}
            className={itemClass(selected)}
          >
            {tab.label}
            {selected && !segmented && (
              <span className="sh-tab-indicator" aria-hidden="true" />
            )}
          </button>
        );
      })}
    </div>
  );
}

interface TabPanelProps {
  id: string;
  children: React.ReactNode;
}

export const TabPanel: React.FC<TabPanelProps> = ({ id, children }) => (
  <div
    role="tabpanel"
    id={`tabpanel-${id}`}
    aria-labelledby={`tab-${id}`}
    tabIndex={0}
    // pt-4, not pt-6 — and not the pt-2 this used to carry either. The 2 was
    // the right instinct answering the wrong measurement: at the shipping
    // window size the gap between the tab bar and the first row was 53px of a
    // 532px pane, 10% of the visible screen doing nothing directly beneath the
    // one control a user has to notice to understand what they are looking at.
    // But the panel is a *sibling* of its tab bar, so a `space-y` on whatever
    // holds the two was landing on this element as well and the 8px never
    // existed on screen. With that gone this is the whole gap, and 8px sat a
    // tabbed panel's content hard against the bar that switches it.
    //
    // No `space-y` here on purpose. A panel that holds one group needs no
    // internal rhythm, and the one panel that holds two children holds a
    // nested tab bar and the panel it switches — which want to be adjacent,
    // not separated by a gap meant for unrelated groups of settings. Panels
    // that genuinely stack groups should say so themselves.
    className="pt-4 focus-visible:outline-none"
  >
    {children}
  </div>
);
