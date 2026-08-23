import React from "react";
import { useAdvanced } from "../useAdvanced";

interface AdvancedOnlyProps {
  children: React.ReactNode;
}

/**
 * Renders its children only when advanced settings are revealed.
 *
 * Unmounts rather than hides. That matters for more than tidiness: a
 * `ShortcutInput` that is merely styled-away still registers a live global
 * shortcut, because `SettingContainer`'s `disabled` prop only fades the label
 * and never reaches the key recorder or the Reset button. Anything wrapped here
 * is genuinely absent.
 *
 * Deliberately does not animate. Revealing a dozen rows with a height
 * transition draws the eye to the mechanism instead of to what appeared, and
 * the one motion primitive in this fork is spent on the sweep.
 */
/**
 * Marks the revealed block so the switch can scroll the first one into view.
 *
 * `display: contents` rather than a real box, because `Sheet` puts its
 * `divide-y` on direct children: a wrapping `<div>` would collapse every
 * advanced row into one child and lose the hairlines between them. Contents
 * elements have no box of their own, so the anchor is the first child.
 */
export const ADVANCED_ANCHOR_ATTR = "data-advanced-anchor";

export const AdvancedOnly: React.FC<AdvancedOnlyProps> = ({ children }) => {
  const { advanced } = useAdvanced();
  if (!advanced) return null;
  return (
    <div className="contents" {...{ [ADVANCED_ANCHOR_ATTR]: "" }}>
      {children}
    </div>
  );
};
