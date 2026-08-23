import React from "react";
import { useTranslation } from "react-i18next";
import { FlaskConical } from "lucide-react";
import { ShorthandWordmark } from "@/shorthand/brand";
import { AdvancedSwitch } from "@/shorthand/ui/AdvancedSwitch";
import { useSettings } from "../hooks/useSettings";
import { getVisibleSectionIds } from "@/shorthand/visibility";
import { SHORTHAND_SECTIONS } from "@/shorthand/sections";
import { DebugSettings } from "./settings";

export type SidebarSection = keyof typeof SECTIONS_CONFIG;

interface IconProps {
  width?: number | string;
  height?: number | string;
  size?: number | string;
  className?: string;
  [key: string]: any;
}

interface SectionConfig {
  labelKey: string;
  icon: React.ComponentType<IconProps>;
  component: React.ComponentType;
  enabled: (settings: any) => boolean;
}

// The fork's sections are the whole registry now, not an addition to
// upstream's. General, Models, Advanced and Post-processing are deliberately
// not registered: their rows live in the fork's sections instead, reachable by
// default or behind the Advanced switch. History and About are fork-owned too,
// so they can lose the card and hold rows upstream's versions have no home for.
//
// The unregistered components are NOT deleted. Deleting a file upstream still
// maintains turns every future edit to it into a delete/modify conflict, which
// is the expensive kind. `tests/settings-coverage.spec.ts` is what makes
// leaving them unregistered safe: it fails if any leaf setting control stops
// being reachable.
//
// Debug is the one upstream section kept as-is. It holds diagnostics rather
// than preferences, it is already gated behind `debug_mode`, and nothing in the
// redesign has an opinion about it.
export const SECTIONS_CONFIG = {
  ...SHORTHAND_SECTIONS,
  debug: {
    labelKey: "sidebar.debug",
    icon: FlaskConical,
    component: DebugSettings,
    enabled: (settings) => settings?.debug_mode ?? false,
  },
} as const satisfies Record<string, SectionConfig>;

interface SidebarProps {
  activeSection: SidebarSection;
  onSectionChange: (section: SidebarSection) => void;
}

export const Sidebar: React.FC<SidebarProps> = ({
  activeSection,
  onSectionChange,
}) => {
  const { t } = useTranslation();
  const { settings } = useSettings();

  const availableSections = getVisibleSectionIds(SECTIONS_CONFIG, settings).map(
    (id) => ({
      id: id as SidebarSection,
      ...SECTIONS_CONFIG[id as SidebarSection],
    }),
  );

  // The rail is deliberately the quietest surface in the app. It carries no
  // highlighter mark: the sweep degrades into a chip below roughly a 5:1 aspect
  // ratio, and nav labels ("App", "Modes") are far under that — see
  // shorthand/brand/marks.css. Selection is an accent icon and a full-weight
  // label against dimmed neighbours, which also protects the rule the whole
  // direction rests on: colour means live, not merely selected.
  return (
    <div className="flex flex-col w-40 h-full border-e border-mid-gray/20 items-center px-2">
      <ShorthandWordmark height={24} className="m-4" />
      <nav
        aria-label={t("sidebar.general")}
        className="flex flex-col w-full items-center gap-1 pt-2 border-t border-mid-gray/20"
      >
        {availableSections.map((section) => {
          const Icon = section.icon;
          const isActive = activeSection === section.id;

          return (
            <button
              key={section.id}
              type="button"
              // Upstream renders these as bare clickable divs with no role,
              // tabIndex or keyboard handler, so the whole navigation is
              // unreachable without a mouse. A button gets Enter/Space and
              // focus for free; aria-current exposes the selection to a screen
              // reader, so it does not depend on the colour at all.
              aria-current={isActive ? "page" : undefined}
              className={`flex gap-2 items-center p-2 w-full rounded-lg cursor-pointer transition-colors text-start bg-transparent border-0 focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-logo-primary ${
                isActive
                  ? ""
                  : "hover:bg-mid-gray/15 opacity-70 hover:opacity-100"
              }`}
              onClick={() => onSectionChange(section.id)}
            >
              <Icon
                width={24}
                height={24}
                className={`shrink-0 ${isActive ? "text-logo-primary" : ""}`}
              />
              <p
                className={`text-sm truncate ${isActive ? "font-semibold" : "font-medium"}`}
                title={t(section.labelKey)}
              >
                {t(section.labelKey)}
              </p>
            </button>
          );
        })}
      </nav>
      {/* Fork-only: the advanced-settings switch lives here rather than buried
          in About, so it is reachable and reversible from every section. */}
      <div className="mt-auto w-full border-t border-mid-gray/20 py-2">
        <AdvancedSwitch />
      </div>
    </div>
  );
};
