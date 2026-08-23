import React from "react";
import { useTranslation } from "react-i18next";
import { Cog, FlaskConical, History, Info, Sparkles, Cpu } from "lucide-react";
import { ShorthandMark, ShorthandWordmark } from "@/shorthand/brand";
import { AdvancedSwitch } from "@/shorthand/ui/AdvancedSwitch";
import { useSettings } from "../hooks/useSettings";
import { getVisibleSectionIds } from "@/shorthand/visibility";
import { SHORTHAND_SECTIONS } from "@/shorthand/sections";
import {
  GeneralSettings,
  AdvancedSettings,
  HistorySettings,
  DebugSettings,
  AboutSettings,
  PostProcessingSettings,
  ModelsSettings,
} from "./settings";

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

export const SECTIONS_CONFIG = {
  ...SHORTHAND_SECTIONS,
  general: {
    labelKey: "sidebar.general",
    icon: ShorthandMark,
    component: GeneralSettings,
    enabled: () => true,
  },
  history: {
    labelKey: "sidebar.history",
    icon: History,
    component: HistorySettings,
    enabled: () => true,
  },
  models: {
    labelKey: "sidebar.models",
    icon: Cpu,
    component: ModelsSettings,
    enabled: () => true,
  },
  advanced: {
    labelKey: "sidebar.advanced",
    icon: Cog,
    component: AdvancedSettings,
    enabled: () => true,
  },
  postprocessing: {
    labelKey: "sidebar.postProcessing",
    icon: Sparkles,
    component: PostProcessingSettings,
    enabled: (settings) =>
      (settings?.post_process_enabled ?? false) ||
      (settings?.dictation?.post_process_enabled ?? false),
  },
  debug: {
    labelKey: "sidebar.debug",
    icon: FlaskConical,
    component: DebugSettings,
    enabled: (settings) => settings?.debug_mode ?? false,
  },
  about: {
    labelKey: "sidebar.about",
    icon: Info,
    component: AboutSettings,
    enabled: () => true,
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
