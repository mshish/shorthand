import {
  SlidersHorizontal,
  Mic,
  Cpu,
  Sparkles,
  AppWindow,
  History,
  Info,
} from "lucide-react";
import { ModesSettings } from "./settings/ModesSettings";
import { AudioSettings } from "./settings/AudioSettings";
import { ModelSettings } from "./settings/ModelSettings";
import { AICleanupSettings } from "./settings/AICleanupSettings";
import { AppSettings } from "./settings/AppSettings";
import { HistorySettings } from "./settings/HistorySettings";
import { AboutSettings } from "./settings/AboutSettings";

/**
 * The fork's settings sections, spread into `SECTIONS_CONFIG` in
 * `src/components/Sidebar.tsx` ahead of upstream's own entries.
 *
 * Kept in this file so registering or reordering a section never conflicts with
 * upstream's edits to that object, and spread first so the app opens on Modes.
 *
 * These do not sit *alongside* upstream's General / Advanced / Models /
 * Post-processing screens — they replace them. Those four are no longer
 * registered anywhere, so the fork owns settings presentation outright. Their
 * files stay in the tree, untouched and unregistered: deleting a file upstream
 * still maintains turns every future edit into a delete/modify conflict, which
 * is the expensive kind. `tests/settings-coverage.spec.ts` is what makes
 * leaving them unregistered safe — it fails if any leaf setting control stops
 * being reachable.
 *
 * The order is the order a person meets the product: what the shortcuts do,
 * then what it listens to, then what it transcribes with, then the optional
 * cleanup, then the app itself, then what it kept, then what it is.
 */
export const SHORTHAND_SECTIONS = {
  modes: {
    labelKey: "sidebar.modes",
    icon: SlidersHorizontal,
    component: ModesSettings,
    enabled: () => true,
  },
  audio: {
    labelKey: "sidebar.audio",
    icon: Mic,
    component: AudioSettings,
    enabled: () => true,
  },
  model: {
    labelKey: "sidebar.model",
    icon: Cpu,
    component: ModelSettings,
    enabled: () => true,
  },
  // Same predicate the section had before the redesign: the LLM connection is
  // only worth a sidebar row once one of the modes will actually use it.
  aicleanup: {
    labelKey: "sidebar.aiCleanup",
    icon: Sparkles,
    component: AICleanupSettings,
    enabled: (settings: any) =>
      (settings?.post_process_enabled ?? false) ||
      (settings?.dictation?.post_process_enabled ?? false),
  },
  app: {
    labelKey: "sidebar.app",
    icon: AppWindow,
    component: AppSettings,
    enabled: () => true,
  },
  history: {
    labelKey: "sidebar.history",
    icon: History,
    component: HistorySettings,
    enabled: () => true,
  },
  about: {
    labelKey: "sidebar.about",
    icon: Info,
    component: AboutSettings,
    enabled: () => true,
  },
};
