import React, { useCallback, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { openUrl } from "@tauri-apps/plugin-opener";
import { getCurrentWindow } from "@tauri-apps/api/window";
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
 * from Obsidian having pressed Install there — which is Tauri's window focus
 * event — so the row re-checks on every focus as well as on mount. What
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
  const [openError, setOpenError] = useState<string | null>(null);
  // Single source of truth for the hand-off flag, kept in a ref rather than
  // state: `refresh` reads it fresh on every call, so it does not need to be
  // a dependency of the `useCallback` below. Making it a dependency would
  // mean every flip of the flag gives `refresh` a new identity, which tears
  // down and re-subscribes the focus listener (and re-fetches) in the effect
  // that follows. True after Install has been pressed, cleared by `refresh`
  // once a status read reports the plugin installed — see
  // `ObsidianPluginRowState["awaitingObsidian"]` in `obsidianPluginState.ts`.
  const awaitingObsidianRef = useRef(false);

  const refresh = useCallback(async () => {
    // An action error is only relevant until the next status read explains
    // where things actually stand, so it doesn't survive a refresh.
    setOpenError(null);
    try {
      // tauri-specta resolves a backend `Err` as `{status: "error"}`, not a
      // rejection — see docs/FRONTEND_TESTING.md for the bug that taught us.
      // A rejection (IPC failure, backend panic) is still possible, so this
      // is wrapped too: without it, a rejection here would leave the row on
      // "Checking…" forever instead of the `error` phase and its retry
      // button.
      const result = await commands.getObsidianPluginStatus();
      if (result.status === "error") {
        setState({ phase: "error", message: String(result.error) });
        return;
      }
      if (result.data.kind === "installed") {
        awaitingObsidianRef.current = false;
      }
      setState({
        phase: "ready",
        status: result.data,
        awaitingObsidian: awaitingObsidianRef.current,
      });
    } catch (error) {
      setState({ phase: "error", message: String(error) });
    }
  }, []);

  useEffect(() => {
    refresh();
    const unlisten = getCurrentWindow().onFocusChanged(
      ({ payload: focused }) => {
        if (focused) refresh();
      },
    );
    return () => {
      unlisten.then((fn) => fn());
    };
  }, [refresh]);

  const act = async (action: ObsidianPluginAction) => {
    setOpenError(null);
    switch (action) {
      case "retry":
        await refresh();
        return;
      case "get_obsidian":
        try {
          await openUrl(OBSIDIAN_DOWNLOAD_URL);
        } catch (error) {
          setOpenError(String(error));
        }
        return;
      case "install":
      case "show": {
        try {
          const result = await commands.openObsidianPluginPage();
          if (result.status === "error") {
            setOpenError(String(result.error));
            return;
          }
        } catch (error) {
          setOpenError(String(error));
          return;
        }
        if (action === "install") {
          awaitingObsidianRef.current = true;
          setState((current) =>
            current.phase === "ready"
              ? { ...current, awaitingObsidian: true }
              : current,
          );
          await refresh();
        }
      }
    }
  };

  const view = describeObsidianPlugin(state);
  const description = openError
    ? t("settings.notes.obsidian.openFailed", { error: openError })
    : t(view.descriptionKey, view.params);
  const action = view.action;

  return (
    <SettingContainer
      title={t("settings.notes.plugin.title")}
      description={description}
      descriptionMode="inline"
      grouped={grouped}
    >
      {action && (
        <Button
          variant={action === "install" ? "primary" : "secondary"}
          size="md"
          onClick={() => act(action)}
        >
          {t(ACTION_LABEL_KEYS[action])}
        </Button>
      )}
    </SettingContainer>
  );
};
