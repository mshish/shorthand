import React from "react";
import { useTranslation } from "react-i18next";
import { Dropdown } from "@/components/ui/Dropdown";
import { SettingContainer } from "@/components/ui/SettingContainer";
import { useSettings } from "@/hooks/useSettings";
import type {
  DictationSettings,
  OverlayPosition,
  OverlayStyle,
} from "@/bindings";

interface DictationShowOverlayProps {
  descriptionMode?: "inline" | "tooltip";
  grouped?: boolean;
  disabled?: boolean;
}

/**
 * Fork-only sibling of ShowOverlay.tsx. The overlay *style* dropdown is
 * per-mode (settings.dictation.overlay_style, default "minimal" per the
 * spec). The overlay *position* dropdown stays bound to the shared
 * top-level overlay_position via the ordinary getSetting/updateSetting path
 * — the spec is explicit that "top-versus-bottom is a screen-layout
 * preference, not a mode one", so it is not read from or written to the
 * dictation struct at all.
 */
export const DictationShowOverlay: React.FC<DictationShowOverlayProps> = ({
  descriptionMode = "tooltip",
  grouped = false,
  disabled = false,
}) => {
  const { t } = useTranslation();
  const { getSetting, updateSetting, isUpdating } = useSettings();
  const dictation = getSetting("dictation") as DictationSettings | undefined;

  const styleOptions = [
    { value: "none", label: t("settings.advanced.overlay.style.options.none") },
    {
      value: "minimal",
      label: t("settings.advanced.overlay.style.options.minimal"),
    },
    { value: "live", label: t("settings.advanced.overlay.style.options.live") },
  ];

  const positionOptions = [
    {
      value: "bottom",
      label: t("settings.advanced.overlay.position.options.bottom"),
    },
    { value: "top", label: t("settings.advanced.overlay.position.options.top") },
  ];

  const selectedStyle = (dictation?.overlay_style || "minimal") as OverlayStyle;
  const selectedPosition: OverlayPosition =
    getSetting("overlay_position") === "top" ? "top" : "bottom";

  return (
    <>
      <SettingContainer
        title={t("settings.advanced.overlay.style.title")}
        description={t("settings.advanced.overlay.style.description")}
        descriptionMode={descriptionMode}
        grouped={grouped}
        disabled={disabled}
      >
        <Dropdown
          options={styleOptions}
          selectedValue={selectedStyle}
          onSelect={(value) =>
            updateSetting("dictation", {
              ...dictation,
              overlay_style: value as OverlayStyle,
            } as DictationSettings)
          }
          disabled={disabled || isUpdating("dictation")}
        />
      </SettingContainer>

      {selectedStyle !== "none" && (
        <SettingContainer
          title={t("settings.advanced.overlay.position.title")}
          description={t("settings.dictation.overlayPosition.sharedDescription")}
          descriptionMode={descriptionMode}
          grouped={grouped}
          disabled={disabled}
        >
          <Dropdown
            options={positionOptions}
            selectedValue={selectedPosition}
            onSelect={(value) =>
              updateSetting("overlay_position", value as OverlayPosition)
            }
            disabled={disabled || isUpdating("overlay_position")}
          />
        </SettingContainer>
      )}
    </>
  );
};
