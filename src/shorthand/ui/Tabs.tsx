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
            className={`cursor-pointer bg-transparent border-0 pb-2 pt-1 text-sm transition-colors focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-logo-primary ${
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
    className="space-y-8 pt-6 focus-visible:outline-none"
  >
    {children}
  </div>
);
