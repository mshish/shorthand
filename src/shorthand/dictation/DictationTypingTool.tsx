import React, { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { Dropdown } from "@/components/ui/Dropdown";
import { SettingContainer } from "@/components/ui/SettingContainer";
import { useSettings } from "@/hooks/useSettings";
import { useOsType } from "@/hooks/useOsType";
import { commands } from "@/bindings";
import type { DictationSettings, TypingTool } from "@/bindings";

interface DictationTypingToolProps {
  descriptionMode?: "inline" | "tooltip";
  grouped?: boolean;
  disabled?: boolean;
}

const allToolLabels: Record<string, string> = {
  wtype: "wtype",
  kwtype: "kwtype",
  dotool: "dotool",
  ydotool: "ydotool",
  xdotool: "xdotool",
};

export const DictationTypingTool: React.FC<DictationTypingToolProps> = ({
  descriptionMode = "tooltip",
  grouped = false,
  disabled = false,
}) => {
  const { t } = useTranslation();
  const { getSetting, updateSetting, isUpdating } = useSettings();
  const osType = useOsType();
  const [availableTools, setAvailableTools] = useState<string[] | null>(null);
  const dictation = getSetting("dictation") as DictationSettings | undefined;

  useEffect(() => {
    if (osType !== "linux") return;
    commands
      .getAvailableTypingTools()
      .then(setAvailableTools)
      .catch(() => {
        setAvailableTools(["auto"]);
      });
  }, [osType]);

  // Only relevant on Linux, and only when paste_method is "direct" — same
  // gating as upstream's TypingTool.tsx.
  if (osType !== "linux") {
    return null;
  }
  if (dictation?.paste_method !== "direct") {
    return null;
  }

  const tools = availableTools ?? ["auto"];
  const options = tools.map((tool) =>
    tool === "auto"
      ? { value: "auto", label: t("settings.advanced.typingTool.options.auto") }
      : { value: tool, label: allToolLabels[tool] ?? tool },
  );

  const selectedTool = (dictation?.typing_tool || "auto") as TypingTool;

  return (
    <SettingContainer
      title={t("settings.advanced.typingTool.title")}
      description={t("settings.advanced.typingTool.description")}
      descriptionMode={descriptionMode}
      grouped={grouped}
      disabled={disabled}
      tooltipPosition="bottom"
    >
      <Dropdown
        options={options}
        selectedValue={selectedTool}
        onSelect={(value) =>
          updateSetting("dictation", {
            ...dictation,
            typing_tool: value as TypingTool,
          } as DictationSettings)
        }
        disabled={disabled || isUpdating("dictation")}
      />
    </SettingContainer>
  );
};
