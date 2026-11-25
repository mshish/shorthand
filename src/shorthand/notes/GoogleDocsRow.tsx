import React, { useCallback, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { commands } from "@/bindings";
import { Button } from "@/components/ui/Button";
import { SettingContainer } from "@/components/ui/SettingContainer";
import {
  describeGoogleDocs,
  GOOGLE_ACTION_LABEL_KEYS,
  type GoogleDocsAction,
  type GoogleDocsRowState,
} from "./googleDocsState";

/** Matches `shorthand::google::STATUS_EVENT`. */
const STATUS_EVENT = "google-docs-status-changed";

interface GoogleDocsRowProps {
  grouped?: boolean;
}

/**
 * Fork-only row: is Google connected, which doc meetings go into, and how
 * the last meeting went. Re-reads on mount, on window focus, and whenever
 * the backend says something changed (a sign-in finishing, a meeting
 * starting or ending). The copy lives in `googleDocsState.ts`.
 */
export const GoogleDocsRow: React.FC<GoogleDocsRowProps> = ({
  grouped = false,
}) => {
  const { t } = useTranslation();
  const [state, setState] = useState<GoogleDocsRowState>({ phase: "loading" });
  const [actionError, setActionError] = useState<string | null>(null);
  /** Set by Cancel, so the sign-in it stops doesn't read as a failure. */
  const cancelled = useRef(false);

  const refresh = useCallback(async () => {
    try {
      const status = await commands.getGoogleDocsStatus();
      setState((current) =>
        current.phase === "connecting" ? current : { phase: "ready", status },
      );
    } catch (error) {
      setState({ phase: "error", message: String(error) });
    }
  }, []);

  useEffect(() => {
    refresh();
    const unlistenFocus = getCurrentWindow().onFocusChanged(
      ({ payload: focused }) => {
        if (focused) refresh();
      },
    );
    const unlistenStatus = listen(STATUS_EVENT, () => refresh());
    return () => {
      unlistenFocus.then((fn) => fn());
      unlistenStatus.then((fn) => fn());
    };
  }, [refresh]);

  const act = async (action: GoogleDocsAction) => {
    setActionError(null);
    try {
      switch (action) {
        case "retry":
          await refresh();
          return;
        case "cancel":
          cancelled.current = true;
          await commands.cancelGoogleSignIn();
          return;
        case "connect_pick":
        case "change_doc":
        case "connect_create": {
          cancelled.current = false;
          setState({ phase: "connecting" });
          const result = await commands.connectGoogleDocs(
            action === "connect_create",
          );
          if (result.status === "error") {
            if (!cancelled.current) setActionError(String(result.error));
            setState({ phase: "loading" });
            await refresh();
            return;
          }
          setState({ phase: "ready", status: result.data });
          return;
        }
        case "open_doc": {
          const result = await commands.openGoogleDoc();
          if (result.status === "error") setActionError(String(result.error));
          return;
        }
        case "disconnect": {
          const result = await commands.disconnectGoogleDocs();
          if (result.status === "error") {
            setActionError(String(result.error));
            return;
          }
          setState({ phase: "ready", status: result.data });
          return;
        }
      }
    } catch (error) {
      setActionError(String(error));
      setState({ phase: "loading" });
      await refresh();
    }
  };

  const view = describeGoogleDocs(state);
  const description = actionError
    ? t("settings.notes.google.actionFailed", { error: actionError })
    : [
        t(view.descriptionKey, view.params),
        view.lastRunKey ? t(view.lastRunKey, view.lastRunParams) : null,
      ]
        .filter(Boolean)
        .join(" ");

  return (
    <SettingContainer
      title={t("settings.notes.google.rowTitle")}
      description={description}
      descriptionMode="inline"
      grouped={grouped}
    >
      <div className="flex flex-wrap gap-2 justify-end">
        {view.actions.map((action, index) => (
          <Button
            key={action}
            variant={index === 0 ? "primary" : "secondary"}
            size="md"
            onClick={() => act(action)}
          >
            {t(GOOGLE_ACTION_LABEL_KEYS[action])}
          </Button>
        ))}
      </div>
    </SettingContainer>
  );
};
