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
export const AdvancedOnly: React.FC<AdvancedOnlyProps> = ({ children }) => {
  const { advanced } = useAdvanced();
  return advanced ? <>{children}</> : null;
};
