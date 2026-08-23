import React, { useRef } from "react";

export interface TabSpec<T extends string> {
  id: T;
  label: string;
}

interface TabsProps<T extends string> {
  tabs: readonly TabSpec<T>[];
  active: T;
  onChange: (id: T) => void;
  /** Labels the tablist for screen readers. */
  label: string;
}

/**
 * The Transcription/Dictation tab bar.
 *
 * Implements the WAI-ARIA tabs pattern properly — roles, `aria-selected`,
 * `aria-controls`, roving `tabIndex`, and Left/Right/Home/End. Upstream's
 * sidebar rows are bare clickable `<div>`s with no role, no `tabIndex` and no
 * keyboard handler, and copying that here would have added a second
 * unreachable-by-keyboard navigation control rather than one.
 *
 * The active tab carries the sweep (`sh-sweep`, see brand/marks.css), which
 * wraps the *label* rather than the button: the mark degrades into a chip below
 * roughly a 5:1 aspect ratio, and a padded button box is well under that. Tab
 * labels are long enough to hold a stroke; that is why the sweep is used here
 * and not in the sidebar.
 *
 * The sweep is never the only signal. `aria-selected` carries the state
 * programmatically and the label goes semibold, so selection survives
 * greyscale, a screen reader, and `prefers-reduced-motion`.
 */
export function Tabs<T extends string>({
  tabs,
  active,
  onChange,
  label,
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

  return (
    <div
      role="tablist"
      aria-label={label}
      onKeyDown={onKeyDown}
      // gap-8 and px-3, not gap-6 and px-1. The sweep overshoots its label by
      // ~0.5em a side (about 15px total at 14px) so the mark does not stop
      // exactly at the word; the tab list has to leave room for that or
      // neighbouring marks crowd each other and the first one clips the pane
      // edge.
      className="flex items-center gap-8 border-b border-mid-gray/20 px-3"
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
            aria-controls={`tabpanel-${tab.id}`}
            tabIndex={selected ? 0 : -1}
            onClick={() => onChange(tab.id)}
            // pt-2 pb-3, not pt-1 pb-2. The sweep extends 0.18em above the
            // label and its pen line sits 0.26em below, so at pt-1 the mark
            // was clipped by the button's own top edge and the pen line
            // collided with the tab list's bottom border. The padding is what
            // gives the mark somewhere to be.
            className={`cursor-pointer bg-transparent border-0 pb-3 pt-2 text-sm transition-colors focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-logo-primary ${
              selected
                ? "font-semibold"
                : "font-medium text-mid-gray hover:text-text"
            }`}
          >
            {selected ? (
              <span className="sh-sweep">
                <span>{tab.label}</span>
              </span>
            ) : (
              tab.label
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
    // pt-2, not pt-6. At the shipping window size the gap between the tab bar
    // and the first row was 53px of a 532px pane — 10% of the visible screen
    // doing nothing, directly beneath the one control a user has to notice to
    // understand what they are looking at.
    className="space-y-8 pt-2 focus-visible:outline-none"
  >
    {children}
  </div>
);
