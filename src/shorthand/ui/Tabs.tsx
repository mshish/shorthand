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
 * The active tab carries a plain coral underline (`sh-tab-indicator`, see
 * brand/marks.css). Coral keeps the same meaning as before — the tab you're
 * looking at is the live one — without the hand-drawn highlighter marquee that
 * previously drew it; that motif read as visual noise on a plain tab bar.
 *
 * The indicator is never the only signal. `aria-selected` carries the state
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
      className="flex items-center gap-6 border-b border-mid-gray/20 px-1"
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
            className={`relative cursor-pointer bg-transparent border-0 pb-3 pt-2 text-sm transition-colors focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-logo-primary ${
              selected
                ? "font-semibold"
                : "font-medium text-mid-gray hover:text-text"
            }`}
          >
            {tab.label}
            {selected && (
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
    // pt-2, not pt-6. At the shipping window size the gap between the tab bar
    // and the first row was 53px of a 532px pane — 10% of the visible screen
    // doing nothing, directly beneath the one control a user has to notice to
    // understand what they are looking at.
    className="space-y-8 pt-2 focus-visible:outline-none"
  >
    {children}
  </div>
);
