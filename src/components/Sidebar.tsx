import React from "react";
import { useTranslation } from "react-i18next";
import { Cog, FlaskConical, History, Info, Sparkles, Cpu } from "lucide-react";
import { ShorthandMark, ShorthandWordmark } from "@/shorthand/brand";
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

  // The trailing edge is the ruled margin of a steno pad, in the accent rather
  // than in neutral grey: the one place the fork spends colour.
  return (
    <div className="flex flex-col w-40 h-full border-e-2 border-logo-primary items-center px-2">
      <ShorthandWordmark height={24} className="m-4" />
      <div className="flex flex-col w-full items-center gap-1 pt-2 border-t border-mid-gray/20">
        {availableSections.map((section) => {
          const Icon = section.icon;
          const isActive = activeSection === section.id;

          return (
            <div
              key={section.id}
              // Active rows are marked by a stroke in the margin — the same
              // device as the sidebar's own rule — instead of a filled pill.
              // The inactive rows carry a transparent border of the same width
              // so nothing shifts when the selection moves.
              className={`flex gap-2 items-center p-2 w-full rounded-lg cursor-pointer transition-colors border-s-2 ${
                isActive
                  ? "border-background-ui bg-logo-primary/25"
                  : "border-transparent hover:bg-mid-gray/15 hover:opacity-100 opacity-80"
              }`}
              onClick={() => onSectionChange(section.id)}
            >
              <Icon width={24} height={24} className="shrink-0" />
              <p
                className="text-sm font-medium truncate"
                title={t(section.labelKey)}
              >
                {t(section.labelKey)}
              </p>
            </div>
          );
        })}
      </div>
    </div>
  );
};
