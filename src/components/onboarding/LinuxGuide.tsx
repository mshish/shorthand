import React from "react";

/**
 * Fork: upstream links its Linux setup guide at handy.computer/docs/linux.
 * Shorthand has no general Linux setup guide to point at (docs/troubleshooting
 * covers only Ubuntu 26.04 on GNOME Wayland), so the hint is not shown rather
 * than sending people to Handy's site.
 */
export const LinuxGuide: React.FC<{
  i18nKey: "onboarding.linux.tryIt" | "onboarding.linux.done";
}> = () => null;
