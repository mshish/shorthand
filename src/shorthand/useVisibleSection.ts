import { useEffect, useMemo, useState, type ComponentType } from "react";
import { SECTIONS_CONFIG, type SidebarSection } from "@/components/Sidebar";
import { getVisibleSectionIds } from "./visibility";

/**
 * Owns which sidebar section App.tsx currently shows: the initial/fallback
 * section, the correction that keeps the active section from getting stranded
 * when visibility changes out from under it, and resolving that section id to
 * the component App.tsx should render.
 *
 * Section visibility no longer depends on `show_all_settings` — that now
 * reveals rows in place rather than swapping section trees, so the only thing
 * that moves a section in or out is its own `enabled` predicate (post-processing
 * being on for either mode, or debug mode). The stranding correction below is
 * still needed for exactly those cases.
 */
export function useVisibleSection(settings: any) {
  // The first currently-visible section, used both as the initial section
  // below and as the fallback for ActiveComponent. Computed rather than
  // hardcoded because the fork-only sections are spread into SECTIONS_CONFIG
  // first, so which id lands here depends on the registry in
  // src/shorthand/sections.ts rather than on anything stated locally.
  const visibleSectionIds = useMemo(
    () => getVisibleSectionIds(SECTIONS_CONFIG, settings),
    [settings],
  );
  const firstVisibleSection = (visibleSectionIds[0] ??
    (Object.keys(SECTIONS_CONFIG)[0] as SidebarSection)) as SidebarSection;
  const [currentSection, setCurrentSection] =
    useState<SidebarSection>(firstVisibleSection);

  // Correct a stranded section: if whatever the user is currently looking at
  // stops being visible (e.g. post-processing is switched off for both modes
  // while the AI cleanup section is open, or the initial settings load resolves
  // differently from the pre-load default used to seed currentSection above),
  // move to a section that is
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

  // Falls back to the first visible section's component rather than a
  // hardcoded one: sections can be gated off (post-processing, debug), so the
  // fallback must be computed from the currently visible sections (see
  // getVisibleSectionIds above), not hardcoded to a section that may be
  // hidden. SidebarSection is a closed union and currentSection is only
  // ever set from a real key (its initial value above, or Sidebar's
  // onSectionChange, itself only ever called with an id drawn from the
  // visible list), so SECTIONS_CONFIG[currentSection] is never actually
  // undefined at runtime -- the ?? below is a defensive fallback, not a
  // live path.
  const ActiveComponent: ComponentType =
    SECTIONS_CONFIG[currentSection]?.component ??
    SECTIONS_CONFIG[firstVisibleSection].component;

  return { currentSection, setCurrentSection, ActiveComponent };
}
