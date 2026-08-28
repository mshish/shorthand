import React from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { commands } from "@/bindings";
import { useSettings } from "@/hooks/useSettings";
import { Button } from "../../ui/Button";
import { SettingContainer } from "../../ui/SettingContainer";

interface SystemAudioPermissionNoticeProps {
  grouped?: boolean;
}

/**
 * What stands in for the system-audio toggle once availability reports
 * `permission_denied`.
 *
 * macOS grants a Core Audio process tap with no error when it refuses it: the
 * tap opens, the stream starts, and every sample is zero. So the backend only
 * reaches this state after a capture attempt has failed to change the
 * permission answer — which it cannot tell apart from a consent dialog the
 * user ignored, or one they never saw. The copy therefore explains what is
 * needed without asserting that anyone declined anything.
 *
 * Both scopes render this, and both replace their toggle with it, which is why
 * **Try again** is not optional: it is the only route back into the app's own
 * flow for someone who has just granted the permission in System Settings. It
 * re-reads availability and nothing else, because the backend reads the
 * permission on every such call and clears its remembered refusal the moment
 * it sees a grant — no capture attempt is needed to notice one.
 */
export const SystemAudioPermissionNotice: React.FC<
  SystemAudioPermissionNoticeProps
> = ({ grouped = false }) => {
  const { t } = useTranslation();
  const { refreshSystemAudioAvailability, isProbingSystemAudio } =
    useSettings();

  const openPrivacySettings = async () => {
    try {
      // tauri-specta returns a backend refusal as a resolved
      // {status: "error"} — this command Errs on every non-macOS platform —
      // and an onClick handler cannot surface a rejection, so report it the
      // way the rest of the settings UI reports a failed command.
      const result = await commands.openSystemAudioPrivacySettings();
      if (result.status === "error") {
        console.error(
          "Failed to open system audio privacy settings:",
          result.error,
        );
        toast.error(String(result.error));
      }
    } catch (error) {
      console.error("Failed to open system audio privacy settings:", error);
      toast.error(String(error));
    }
  };

  return (
    <SettingContainer
      title={t("settings.advanced.systemAudio.label")}
      description={t("settings.advanced.systemAudio.permissionNeeded")}
      descriptionMode="inline"
      layout="stacked"
      grouped={grouped}
    >
      <div className="flex flex-wrap items-center gap-2">
        <Button
          variant="primary"
          onClick={() => {
            void openPrivacySettings();
          }}
        >
          {t("accessibility.openSettings")}
        </Button>
        <Button
          variant="secondary"
          disabled={isProbingSystemAudio}
          onClick={() => {
            void refreshSystemAudioAvailability();
          }}
        >
          {t("settings.advanced.systemAudio.tryAgain")}
        </Button>
      </div>
    </SettingContainer>
  );
};
