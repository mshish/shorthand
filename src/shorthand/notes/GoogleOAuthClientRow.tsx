import React, { useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { listen } from "@tauri-apps/api/event";
import { commands, type GoogleDocsStatus } from "@/bindings";
import { ApiKeyField } from "@/components/settings/PostProcessingSettingsApi/ApiKeyField";
import { Input } from "@/components/ui/Input";
import { SettingContainer } from "@/components/ui/SettingContainer";

/** Matches `shorthand::google::STATUS_EVENT`. */
const STATUS_EVENT = "google-docs-status-changed";

interface GoogleOAuthClientRowProps {
  grouped?: boolean;
}

/**
 * Fork-only, Advanced: sign in with your own Google OAuth client instead of
 * the one built into Shorthand. The client ID is a setting; the secret goes
 * to the OS keyring and is never sent back, so its field is write-only, like
 * the post-processing API key.
 */
export const GoogleOAuthClientRow: React.FC<GoogleOAuthClientRowProps> = ({
  grouped = false,
}) => {
  const { t } = useTranslation();
  const [status, setStatus] = useState<GoogleDocsStatus | null>(null);
  const [clientId, setClientId] = useState("");
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    try {
      const next = await commands.getGoogleDocsStatus();
      setStatus(next);
      setClientId(next.custom_client_id ?? "");
    } catch (e) {
      setError(String(e));
    }
  }, []);

  useEffect(() => {
    refresh();
    const unlisten = listen(STATUS_EVENT, () => refresh());
    return () => {
      unlisten.then((fn) => fn());
    };
  }, [refresh]);

  const apply = (
    result: Awaited<ReturnType<typeof commands.setGoogleOauthClientSecret>>,
  ) => {
    if (result.status === "error") {
      setError(result.error);
      throw new Error(result.error);
    }
    setError(null);
    setStatus(result.data);
  };

  const commitClientId = async () => {
    if (clientId.trim() === (status?.custom_client_id ?? "")) return;
    try {
      apply(await commands.changeGoogleOauthClientIdSetting(clientId));
    } catch {
      // `apply` already shows the error.
    }
  };

  const secretStatus = status?.custom_client_secret ?? "missing";
  const description = error
    ? t("settings.notes.google.client.saveFailed", { error })
    : status?.using_custom_client
      ? t("settings.notes.google.client.descriptionCustom")
      : status?.custom_client_id || secretStatus === "configured"
        ? t("settings.notes.google.client.descriptionIncomplete")
        : t("settings.notes.google.client.descriptionBuiltIn");

  return (
    <SettingContainer
      title={t("settings.notes.google.client.title")}
      description={description}
      descriptionMode="inline"
      grouped={grouped}
      layout="stacked"
    >
      <div className="flex flex-col gap-2">
        <Input
          type="text"
          value={clientId}
          onChange={(event) => setClientId(event.target.value)}
          onBlur={commitClientId}
          placeholder={t("settings.notes.google.client.idPlaceholder")}
          variant="compact"
          disabled={status === null}
          className="w-full"
          spellCheck={false}
          autoComplete="off"
        />
        <div className="flex items-center gap-2">
          <ApiKeyField
            status={secretStatus}
            onCommit={async (secret) =>
              apply(await commands.setGoogleOauthClientSecret(secret))
            }
            onClear={async () => {
              try {
                apply(await commands.clearGoogleOauthClientSecret());
              } catch {
                // `apply` already shows the error.
              }
            }}
            disabled={status === null || secretStatus === "unavailable"}
          />
        </div>
      </div>
    </SettingContainer>
  );
};
