/*
 * The real settings window, rendered in a plain browser — NOT part of the app
 * bundle.
 *
 * Only ever reached through `./preview.tsx`, which installs the fake IPC layer
 * first. Importing this module directly will throw on the first `type()` call.
 *
 * Everything visible here is the app's own code: the real `Sidebar`, the real
 * `SECTIONS_CONFIG` resolved through the real `useVisibleSection`, the real
 * section components with their real `useSettings()` calls. Nothing about the
 * settings UI is re-implemented, which is the point — a screenshot of a
 * hand-rolled copy only ever proves the copy looks right.
 *
 * This mirrors the main-app branch of `src/App.tsx`. What it leaves out, and
 * why:
 *   - onboarding, which is a different screen and gates on
 *     `onboarding_completed` (the mock sets it true);
 *   - `WhatsNewGate`, which opens a modal over the whole window and would
 *     cover the thing being photographed;
 *   - the `Toaster`, which has nothing to report with no backend behind it.
 * The sidebar, the content pane and the footer — the frame the redesign is
 * actually about — are the real ones.
 */

import React from "react";
import ReactDOM from "react-dom/client";
import { platform } from "@tauri-apps/plugin-os";

import "@/App.css";
import "@/i18n";

import AccessibilityPermissions from "@/components/AccessibilityPermissions";
import SecureInputWarning from "@/components/SecureInputWarning";
import Footer from "@/components/footer";
import { ErrorBoundary } from "@/components/ErrorBoundary";
import { Sidebar } from "@/components/Sidebar";
import { useSettings } from "@/hooks/useSettings";
import { useVisibleSection } from "@/shorthand/useVisibleSection";
import { useModelStore } from "@/stores/modelStore";

// Same two boot steps main.tsx performs before rendering. The theme is not one
// of them: the screenshot script owns `data-theme`, and calling
// syncThemeFromSettings() here would let the mock's `theme: "system"` overwrite
// whichever theme is being photographed.
document.documentElement.dataset.platform = platform();
void useModelStore.getState().initialize();

const SettingsWindow: React.FC = () => {
  const { settings } = useSettings();
  const { currentSection, setCurrentSection, ActiveComponent } =
    useVisibleSection(settings);

  return (
    <div className="h-screen flex flex-col select-none cursor-default">
      <div className="flex-1 flex overflow-hidden">
        <Sidebar
          activeSection={currentSection}
          onSectionChange={setCurrentSection}
        />
        <div className="flex-1 flex flex-col overflow-hidden">
          <div className="flex-1 overflow-y-auto">
            <div className="flex flex-col items-center p-4 gap-4">
              <AccessibilityPermissions />
              <SecureInputWarning />
              <ErrorBoundary context="Settings section">
                <ActiveComponent />
              </ErrorBoundary>
            </div>
          </div>
        </div>
      </div>
      <Footer />
    </div>
  );
};

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <SettingsWindow />
  </React.StrictMode>,
);
