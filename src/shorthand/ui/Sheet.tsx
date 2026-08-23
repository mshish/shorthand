import React from "react";

interface SheetProps {
  /** Sentence-case heading. Omit for an unheaded run of rows. */
  title?: string;
  /** One line saying what the group is for, in the user's terms. */
  description?: string;
  children: React.ReactNode;
}

/**
 * Fork-only replacement for `components/ui/SettingsGroup`.
 *
 * Same shape, same children, one difference: there is no card. Upstream draws
 * every group as a bordered, rounded box; this draws it as part of a page.
 *
 * That is the largest single reduction in the redesign — the settings window
 * carries roughly forty of those borders, and none of them separates anything
 * that whitespace and a heading do not separate better. It is also the reason
 * this is a new file rather than an edit to `SettingsGroup`: upstream's own
 * screens keep their own component, so a restyle upstream still merges without
 * a conflict, and the fork's "prefer additive changes" rule is honoured at the
 * one place it would otherwise have been broken.
 *
 * Headings are sentence case, not the uppercase letter-spaced micro-label
 * upstream uses. A micro-label is a decoration that costs legibility at the
 * exact size where legibility is scarcest, and this app's whole subject is the
 * legibility of a written record.
 *
 * Rows are still separated by a hairline, because a settings list genuinely is
 * a list of discrete things and scanning it needs a rhythm. Removing the box is
 * not the same as removing all structure.
 */
export const Sheet: React.FC<SheetProps> = ({
  title,
  description,
  children,
}) => {
  return (
    <section className="space-y-1">
      {title && (
        <div className="px-1 pb-1">
          <h2 className="text-sm font-semibold tracking-normal">{title}</h2>
          {description && (
            <p className="text-xs text-mid-gray mt-0.5">{description}</p>
          )}
        </div>
      )}
      <div className="divide-y divide-mid-gray/15">{children}</div>
    </section>
  );
};
