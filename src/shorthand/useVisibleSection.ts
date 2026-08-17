import { useEffect, useMemo, useState } from "react";
import { SECTIONS_CONFIG, type SidebarSection } from "@/components/Sidebar";
import { getVisibleSectionIds } from "./visibility";

/**
 * Owns which sidebar section App.tsx currently shows, including the
 * fork-only show_all_settings-aware initial/fallback section and the
 * correction that keeps the active section from getting stranded when
 * visibility changes out from under it.
 */
export function useVisibleSection(settings: any) {
  // The first currently-visible section, used both as the initial section
  // below and as renderSettingsContent's fallback. Computed rather than
  // hardcoded because which section that is depends on show_all_settings and
  // the registry in src/shorthand/visibility.ts (e.g. "history" today,
  // "capture" once Task 5 registers the fork-only sections).
  const visibleSectionIds = useMemo(
    () => getVisibleSectionIds(SECTIONS_CONFIG, settings),
    [settings],
  );
  const firstVisibleSection = (visibleSectionIds[0] ??
    (Object.keys(SECTIONS_CONFIG)[0] as SidebarSection)) as SidebarSection;
  const [currentSection, setCurrentSection] =
    useState<SidebarSection>(firstVisibleSection);

  // Correct a stranded section: if whatever the user is currently looking at
  // stops being visible (e.g. show_all_settings flips and hides it, or the
  // initial settings load resolves to a different mode than the pre-load
  // default used to seed currentSection above), move to a section that is
  // still visible instead of leaving the sidebar and content pane
  // disagreeing about what's showing. Never fires from the user's own
  // navigation: Sidebar only ever calls setCurrentSection with an id drawn
  // from this same visible list, so the condition below is already false
  // immediately afterwards.
  useEffect(() => {
    if (!visibleSectionIds.includes(currentSection)) {
      setCurrentSection(firstVisibleSection);
    }
  }, [visibleSectionIds, currentSection, firstVisibleSection]);

  return { currentSection, setCurrentSection, firstVisibleSection };
}
