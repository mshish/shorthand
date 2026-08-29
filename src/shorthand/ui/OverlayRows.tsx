import React from "react";
import { useTranslation } from "react-i18next";
import { Dropdown } from "@/components/ui/Dropdown";
import { SettingContainer } from "@/components/ui/SettingContainer";
import { useSettings } from "@/hooks/useSettings";
import type { OverlayPosition, OverlayStyle } from "@/bindings";

/**
 * `overlay_style` and `overlay_position` rendered separately.
 *
 * Upstream's `ShowOverlay` always renders both, with no prop to split them.
 * That is fine for one screen and wrong for this design, because the two
 * settings are not the same kind of thing:
 *
 *   - `overlay_style` has a `DictationSettings` counterpart, so it is per-mode
 *     and belongs in the Modes tabs;
 *   - `overlay_position` exists only on `AppSettings`, so it is shared and must
 *     appear exactly once — in App.
 *
 * Splitting them here rather than adding props to upstream's component keeps
 * the change additive. `ShowOverlay` is untouched and still works for anyone
 * who wants both rows together.
 *
 * The duplicated option lists and the legacy-value handling are copied
 * deliberately rather than imported: upstream does not export them, and reading
 * them out of that module would couple this file to its internals.
 */

export const OverlayStyleRow: React.FC<{
  descriptionMode?: "inline" | "tooltip";
  grouped?: boolean;
}> = ({ descriptionMode = "inline", grouped = true }) => {
  const { t } = useTranslation();
  const { getSetting, updateSetting, isUpdating } = useSettings();

  const selectedStyle = (getSetting("overlay_style") || "live") as OverlayStyle;

  return (
    <SettingContainer
      title={t("settings.advanced.overlay.style.title")}
      description={t("settings.advanced.overlay.style.description")}
      descriptionMode={descriptionMode}
      grouped={grouped}
    >
      <Dropdown
        options={[
          {
            value: "none",
            label: t("settings.advanced.overlay.style.options.none"),
          },
          {
            value: "minimal",
            label: t("settings.advanced.overlay.style.options.minimal"),
          },
          {
            value: "live",
            label: t("settings.advanced.overlay.style.options.live"),
          },
        ]}
        selectedValue={selectedStyle}
        onSelect={(value) =>
          updateSetting("overlay_style", value as OverlayStyle)
        }
        disabled={isUpdating("overlay_style")}
      />
    </SettingContainer>
  );
};

/**
 * The dictation mode's own `overlay_style`, without the position row.
 *
 * `DictationShowOverlay` renders both, the same way upstream's `ShowOverlay`
 * does, and for the same historical reason: each screen used to own its whole
 * overlay story. Now that the shared position appears exactly once in App,
 * rendering `DictationShowOverlay` in the Modes pane would put a second copy of
 * that one control on screen, bound to the same field — two inputs writing the
 * same value, which is a bug users find before designers do.
 *
 * A fork-owned row rather than a prop on `DictationShowOverlay`: that component
 * is fork-owned too, but it is the surface the previous dictation design
 * documented and it stays valid for anyone rendering the pair together.
 */
export const DictationOverlayStyleRow: React.FC<{
  descriptionMode?: "inline" | "tooltip";
  grouped?: boolean;
  disabled?: boolean;
}> = ({ descriptionMode = "inline", grouped = true, disabled = false }) => {
  const { t } = useTranslation();
  const { getSetting, updateSetting, isUpdating } = useSettings();
  const dictation = getSetting("dictation");

  const selectedStyle = (dictation?.overlay_style || "minimal") as OverlayStyle;

  return (
    <SettingContainer
      title={t("settings.advanced.overlay.style.title")}
      description={t("settings.advanced.overlay.style.description")}
      descriptionMode={descriptionMode}
      grouped={grouped}
      disabled={disabled}
    >
      <Dropdown
        options={[
          {
            value: "none",
            label: t("settings.advanced.overlay.style.options.none"),
          },
          {
            value: "minimal",
            label: t("settings.advanced.overlay.style.options.minimal"),
          },
          {
            value: "live",
            label: t("settings.advanced.overlay.style.options.live"),
          },
        ]}
        selectedValue={selectedStyle}
        onSelect={(value) =>
          updateSetting("dictation", {
            ...dictation,
            overlay_style: value as OverlayStyle,
          } as NonNullable<typeof dictation>)
        }
        disabled={disabled || isUpdating("dictation")}
      />
    </SettingContainer>
  );
};

/**
 * Assisted notes' own `overlay_style`, without the position row. Mirrors
 * `DictationOverlayStyleRow` for the same reason: the shared position
 * already renders exactly once in App, so a second copy here bound to the
 * same field would be two inputs writing one value.
 */
export const AssistedNotesOverlayStyleRow: React.FC<{
  descriptionMode?: "inline" | "tooltip";
  grouped?: boolean;
  disabled?: boolean;
}> = ({ descriptionMode = "inline", grouped = true, disabled = false }) => {
  const { t } = useTranslation();
  const { getSetting, updateSetting, isUpdating } = useSettings();
  const assistedNotes = getSetting("assisted_notes");

  const selectedStyle = (assistedNotes?.overlay_style ||
    "minimal") as OverlayStyle;

  return (
    <SettingContainer
      title={t("settings.advanced.overlay.style.title")}
      description={t("settings.advanced.overlay.style.description")}
      descriptionMode={descriptionMode}
      grouped={grouped}
      disabled={disabled}
    >
      <Dropdown
        options={[
          {
            value: "none",
            label: t("settings.advanced.overlay.style.options.none"),
          },
          {
            value: "minimal",
            label: t("settings.advanced.overlay.style.options.minimal"),
          },
          {
            value: "live",
            label: t("settings.advanced.overlay.style.options.live"),
          },
        ]}
        selectedValue={selectedStyle}
        onSelect={(value) =>
          updateSetting("assisted_notes", {
            ...assistedNotes,
            overlay_style: value as OverlayStyle,
          } as NonNullable<typeof assistedNotes>)
        }
        disabled={disabled || isUpdating("assisted_notes")}
      />
    </SettingContainer>
  );
};

/**
 * The shared position. Self-hides when *no* mode would draw an overlay at all,
 * because a position for something that never appears is a dead control.
 *
 * Upstream gates this on the top-level style alone; here it has to consider
 * all three, since dictation and assisted notes each carry their own style
 * and any of the three modes showing an overlay makes the shared position
 * meaningful.
 */
export const OverlayPositionRow: React.FC<{
  descriptionMode?: "inline" | "tooltip";
  grouped?: boolean;
}> = ({ descriptionMode = "inline", grouped = true }) => {
  const { t } = useTranslation();
  const { getSetting, updateSetting, isUpdating } = useSettings();

  const transcriptionStyle = getSetting("overlay_style") || "live";
  const dictationStyle = getSetting("dictation")?.overlay_style;
  const assistedNotesStyle = getSetting("assisted_notes")?.overlay_style;
  const anyOverlayShown =
    transcriptionStyle !== "none" ||
    (dictationStyle !== undefined && dictationStyle !== "none") ||
    (assistedNotesStyle !== undefined && assistedNotesStyle !== "none");

  if (!anyOverlayShown) return null;

  // Only "top" and "bottom" are selectable; anything else (empty, or a legacy
  // "none" from before the position was retired) falls back to "bottom".
  const selectedPosition: OverlayPosition =
    getSetting("overlay_position") === "top" ? "top" : "bottom";

  return (
    <SettingContainer
      title={t("settings.advanced.overlay.position.title")}
      description={t("settings.advanced.overlay.position.description")}
      descriptionMode={descriptionMode}
      grouped={grouped}
    >
      <Dropdown
        options={[
          {
            value: "bottom",
            label: t("settings.advanced.overlay.position.options.bottom"),
          },
          {
            value: "top",
            label: t("settings.advanced.overlay.position.options.top"),
          },
        ]}
        selectedValue={selectedPosition}
        onSelect={(value) =>
          updateSetting("overlay_position", value as OverlayPosition)
        }
        disabled={isUpdating("overlay_position")}
      />
    </SettingContainer>
  );
};
