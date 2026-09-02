import React, { useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { openUrl } from "@tauri-apps/plugin-opener";
import { commands } from "@/bindings";
import { Button } from "@/components/ui/Button";
import { SettingContainer } from "@/components/ui/SettingContainer";
import {
  ACTION_LABEL_KEYS,
  describeObsidianPlugin,
  type ObsidianPluginAction,
  type ObsidianPluginRowState,
} from "./obsidianPluginState";

const OBSIDIAN_DOWNLOAD_URL = "https://obsidian.md/download";

interface ObsidianPluginRowProps {
  grouped?: boolean;
}

/**
 * Fork-only row: is the Shorthand plugin installed in the vault Obsidian
 * would open, and the one button that moves that along.
 *
 * The status is read from disk by the backend, so it is only as fresh as the
 * last check. The check that matters is the one after the person comes back
 * from Obsidian having pressed Install there — which is a window-focus event
 * from here — so the row re-checks on every focus as well as on mount. What
 * the row says for each status lives in `obsidianPluginState.ts`; this file
 * only fetches and renders.
 */
export const ObsidianPluginRow: React.FC<ObsidianPluginRowProps> = ({
  grouped = false,
}) => {
  const { t } = useTranslation();
  const [state, setState] = useState<ObsidianPluginRowState>({
    phase: "loading",
  });
  const [awaitingObsidian, setAwaitingObsidian] = useState(false);
  const [openError, setOpenError] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    // tauri-specta resolves a backend `Err` as `{status: "error"}`, not a
    // rejection — see docs/FRONTEND_TESTING.md for the bug that taught us.
    const result = await commands.getObsidianPluginStatus();
    if (result.status === "error") {
      setState({ phase: "error", message: String(result.error) });
      return;
    }
    setState({ phase: "ready", status: result.data, awaitingObsidian });
  }, [awaitingObsidian]);

  useEffect(() => {
    refresh();
    window.addEventListener("focus", refresh);
    return () => window.removeEventListener("focus", refresh);
  }, [refresh]);

  const act = async (action: ObsidianPluginAction) => {
    setOpenError(null);
    switch (action) {
      case "retry":
        await refresh();
        return;
      case "get_obsidian":
        await openUrl(OBSIDIAN_DOWNLOAD_URL);
        return;
      case "install":
      case "show": {
        const result = await commands.openObsidianPluginPage();
        if (result.status === "error") {
          setOpenError(String(result.error));
          return;
        }
        if (action === "install") {
          setAwaitingObsidian(true);
          setState((current) =>
            current.phase === "ready"
              ? { ...current, awaitingObsidian: true }
              : current,
          );
        }
      }
    }
  };

  const view = describeObsidianPlugin(state);
  const description = openError
    ? t("settings.notes.obsidian.openFailed", { error: openError })
    : t(view.descriptionKey, view.params);

  return (
    <SettingContainer
      title={t("settings.notes.plugin.title")}
      description={description}
      descriptionMode="inline"
      grouped={grouped}
    >
      {view.action && (
        <Button
          variant={view.action === "install" ? "primary" : "secondary"}
          size="md"
          onClick={() => act(view.action as ObsidianPluginAction)}
        >
          {t(ACTION_LABEL_KEYS[view.action])}
        </Button>
      )}
    </SettingContainer>
  );
};
