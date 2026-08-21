import { Mic, Captions, AppWindow, Keyboard } from "lucide-react";
import { CaptureSettings } from "./CaptureSettings";
import { TranscriptionSettings } from "./TranscriptionSettings";
import { AppSettings } from "./AppSettings";
import { DictationSettings } from "./DictationSettings";

/**
 * Fork-only sidebar section configs (Capture, Transcription, App,
 * Dictation), kept out of `src/components/Sidebar.tsx`'s `SECTIONS_CONFIG`
 * so registering or changing these never conflicts with upstream's own
 * entries in that object. Spread into `SECTIONS_CONFIG` first so the app
 * opens on Capture by default; see `src/shorthand/visibility.ts` for how
 * these replace upstream's general/models/advanced/postprocessing sections
 * in the simplified profile.
 */
export const SHORTHAND_SECTIONS = {
  capture: {
    labelKey: "sidebar.capture",
    icon: Mic,
    component: CaptureSettings,
    enabled: () => true,
  },
  transcription: {
    labelKey: "sidebar.transcription",
    icon: Captions,
    component: TranscriptionSettings,
    enabled: () => true,
  },
  app: {
    labelKey: "sidebar.app",
    icon: AppWindow,
    component: AppSettings,
    enabled: () => true,
  },
  dictation: {
    labelKey: "sidebar.dictation",
    icon: Keyboard,
    component: DictationSettings,
    enabled: () => true,
  },
};
