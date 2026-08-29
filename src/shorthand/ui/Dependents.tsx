import React from "react";

interface DependentsProps {
  /** The parent control's current value. */
  on: boolean;
  children: React.ReactNode;
}

/**
 * The rows a toggle unlocks, drawn as belonging to it.
 *
 * Settings that only mean something once another setting is on were previously
 * indistinguishable from their neighbours: the cleanup hotkey and the prompt
 * picker appeared in the flat run of rows with nothing tying them to the
 * cleanup toggle two rows up, and the sound rows were greyed out with no
 * indication of what would un-grey them. Both failures are the same failure —
 * a dependency the layout does not express, so the user has to discover it by
 * flipping switches.
 *
 * The mark is a **margin rule**: a rule drawn down the left of a passage, in
 * the ink accent, with the passage indented beside it. That is what someone
 * marking up a page does to say "this part goes with that part", and it is the
 * reason this is a rule rather than a box. The redesign removed roughly forty
 * card borders from this window (see `Sheet`); re-introducing one here to
 * solve a grouping problem would undo that on the first screen a user reaches.
 * The tint is at 5% for the same reason — enough to bind the rows into one
 * object at a glance, not enough to read as a surface. Only the right side is
 * rounded, so the block stays open-ended toward the rule rather than closing
 * into a container.
 *
 * Renders nothing when `on` is false. Hidden, never disabled — and not only
 * for tidiness. `SettingContainer`'s `disabled` prop fades the title text and
 * stops there: it never reaches a `ShortcutInput`'s key recorder, so a
 * "disabled" shortcut row still registers a live global hotkey for a feature
 * that will not run. Several call sites were already hiding rather than
 * disabling for exactly that reason; this makes it the rule instead of a
 * workaround repeated in comments.
 *
 * Deliberately not used for a whole mode's worth of rows. `dictation.enabled`
 * gates almost everything in its tab, and a rule wrapping an entire panel
 * conveys nothing — there is no sibling content for it to be distinguished
 * from. That case just hides its rows. The rule is for a dependent block
 * sitting *inside* a list of independent ones.
 */
export const Dependents: React.FC<DependentsProps> = ({ on, children }) => {
  if (!on) return null;

  return (
    <div className="my-1 ml-2 border-l-[3px] border-logo-primary/60 pl-2">
      {/* The rule is the whole treatment. There is no fill, and the two
          rejected alternatives are worth recording because both look obvious
          on paper.

          A tinted fill cannot work here. `--color-logo-primary` is dark ink in
          the light theme, so `bg-logo-primary` at any alpha low enough not to
          read as a surface produces grey, not blue: at 7% over the #faf8f2
          paper it resolves to #eaebec — a warm grey indistinguishable from the
          fill on a Dropdown or a shortcut chip. The nested block therefore read
          as another control rather than as a marked passage, which is exactly
          backwards. Getting an actually blue tint needs ~15%, and at 15% it is
          a card — the thing this redesign spent forty removed borders getting
          away from (see `Sheet`).

          A deeper indent cannot work either. `SettingContainer`'s horizontal
          layout gives the label `max-w-2/3` and the control no `shrink-0`, so a
          long description beside a wide control has no reserve: at full width
          the cleanup hotkey's "Ctrl + Shift + Space" chip already wraps to two
          lines, and a first attempt at ml-4/mr-2 — 30px — pushed it far enough
          to overlap the description text. Indentation that breaks the rows it
          is indenting is not worth having, and widening the upstream component
          to absorb it would mean editing a file upstream edits (AGENTS.md,
          merge budget). What is left costs a row 13px and nothing on the right.

          So: a 3px accent rule at 60%, and a modest indent beside it. That is a
          rule drawn down the margin next to a passage, which is both what the
          brand is about and the one option that survives having tried the other
          two. */}
      {/* Repeats `Sheet`'s own divider so the rows inside keep the scanning
          rhythm of the rows outside. Without it the nested block reads as one
          undifferentiated slab, which is the opposite of the point. */}
      <div className="divide-y divide-mid-gray/15">{children}</div>
    </div>
  );
};
