import React, { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { getVersion } from "@tauri-apps/api/app";
import { emit } from "@tauri-apps/api/event";
import { openUrl } from "@tauri-apps/plugin-opener";
import { AppDataDirectory } from "@/components/settings/AppDataDirectory";
import { LogDirectory } from "@/components/settings/debug/LogDirectory";
import { Button } from "@/components/ui/Button";
import { SettingContainer } from "@/components/ui/SettingContainer";
import { AdvancedOnly } from "@/shorthand/ui/AdvancedOnly";
import { Sheet } from "@/shorthand/ui/Sheet";

/** Shorthand's own donation page. Upstream's row points at handy.computer. */
const DONATE_URL = "https://donate.stripe.com/fZufZh6T31Jwdig89afEk00";
const SOURCE_URL = "https://github.com/mshish/shorthand";
const HANDY_URL = "https://github.com/cjpais/Handy";

/**
 * Fork-only "About" section.
 *
 * The content is upstream's, minus three rows that the redesign moves
 * elsewhere: `ThemeSelector` and `AppLanguageSelector` go to App, where the
 * rest of the application-wide preferences now live, and
 * `ShowAllSettingsToggle` goes to the sidebar footer so the escape hatch is
 * reachable from every section rather than only from this one.
 *
 * It is a fork file rather than an edit to `about/AboutSettings.tsx` for two
 * reasons. First, the two remaining path rows have to become Advanced, and
 * disclosure is a fork concept upstream's screen knows nothing about. Second,
 * this section is borderless: it uses `Sheet` instead of `SettingsGroup`, so
 * upstream's own About keeps its card and keeps merging cleanly. See Part 2
 * of `docs/superpowers/specs/2026-08-23-shorthand-brand-ux-redesign.md`.
 */
export const AboutSettings: React.FC = () => {
  const { t } = useTranslation();
  const [version, setVersion] = useState("");

  useEffect(() => {
    const fetchVersion = async () => {
      try {
        const appVersion = await getVersion();
        setVersion(appVersion);
      } catch (error) {
        console.error("Failed to get app version:", error);
        setVersion("0.1.2");
      }
    };

    fetchVersion();
  }, []);

  const handleDonateClick = async () => {
    try {
      await openUrl(DONATE_URL);
    } catch (error) {
      console.error("Failed to open donate link:", error);
    }
  };

  // Asks the single `UpdateChecker` instance in the footer to run a manual
  // check, rather than mounting a second one here. A second instance would
  // fire its own check on mount, register its own listener and report status
  // in two places at once; emitting the event upstream's checker already
  // listens for reuses the one that exists. It is a no-op while automatic
  // update checks are switched off, because the checker only listens then.
  const handleCheckForUpdates = () => {
    emit("check-for-updates").catch((error) => {
      console.error("Failed to request an update check:", error);
    });
  };

  return (
    <div className="max-w-3xl w-full mx-auto space-y-8">
      <Sheet title={t("settings.about.title")}>
        {/* Version and the manual check share a row: the check is an action on
            the version, not a separate setting, and no existing key describes
            it as one. */}
        <SettingContainer
          title={t("settings.about.version.title")}
          description={t("settings.about.version.description")}
          descriptionMode="inline"
          grouped={true}
        >
          <div className="flex items-center gap-3">
            {/* eslint-disable-next-line i18next/no-literal-string */}
            <span className="text-sm font-mono">v{version}</span>
            <Button
              variant="secondary"
              size="md"
              onClick={handleCheckForUpdates}
            >
              {t("footer.checkForUpdates")}
            </Button>
          </div>
        </SettingContainer>

        <SettingContainer
          title={t("settings.about.supportDevelopment.title")}
          description={t("settings.about.supportDevelopment.description")}
          descriptionMode="inline"
          grouped={true}
        >
          <Button variant="primary" size="md" onClick={handleDonateClick}>
            {t("settings.about.supportDevelopment.button")}
          </Button>
        </SettingContainer>

        <SettingContainer
          title={t("settings.about.sourceCode.title")}
          description={t("settings.about.sourceCode.description")}
          descriptionMode="inline"
          grouped={true}
        >
          <Button
            variant="secondary"
            size="md"
            onClick={() => openUrl(SOURCE_URL)}
          >
            {t("settings.about.sourceCode.button")}
          </Button>
        </SettingContainer>

        <AdvancedOnly>
          {/* Where the app keeps its files. Useful, and needed perhaps once a
              year; both keep tooltips as Advanced rows may. */}
          <AppDataDirectory descriptionMode="tooltip" grouped={true} />
          <LogDirectory descriptionMode="tooltip" grouped={true} />
        </AdvancedOnly>
      </Sheet>

      <Sheet title={t("settings.about.acknowledgments.title")}>
        {/* Handy first: it is not a dependency, it is the app this one is a
            fork of. Everything the ggml row credits arrived through it. */}
        <SettingContainer
          title={t("settings.about.acknowledgments.handy.title")}
          description={t("settings.about.acknowledgments.handy.description")}
          descriptionMode="inline"
          grouped={true}
          layout="stacked"
        >
          <div className="space-y-3">
            <div className="text-sm text-mid-gray">
              {t("settings.about.acknowledgments.handy.details")}
            </div>
            <Button
              variant="secondary"
              size="md"
              onClick={() => openUrl(HANDY_URL)}
            >
              {t("settings.about.acknowledgments.handy.button")}
            </Button>
          </div>
        </SettingContainer>
        <SettingContainer
          title={t("settings.about.acknowledgments.ggml.title")}
          description={t("settings.about.acknowledgments.ggml.description")}
          descriptionMode="inline"
          grouped={true}
          layout="stacked"
        >
          <div className="text-sm text-mid-gray">
            {t("settings.about.acknowledgments.ggml.details")}
          </div>
        </SettingContainer>
      </Sheet>
    </div>
  );
};
