import React, { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { commands, type LLMPrompt } from "@/bindings";
import { Button } from "@/components/ui/Button";
import { Dropdown } from "@/components/ui/Dropdown";
import { Input } from "@/components/ui/Input";
import { SettingContainer } from "@/components/ui/SettingContainer";
import { Textarea } from "@/components/ui/Textarea";
import { useSettings } from "@/hooks/useSettings";
import { notify } from "@/stores/noticeStore";

interface NotesPromptsRowsProps {
  grouped?: boolean;
}

/** The picker's value for each built-in Default, which is never stored. */
const DEFAULT_MEETING = "__default_meeting";
const DEFAULT_ASSISTED = "__default_assisted";
/** The mode pickers' value for "Default". */
const DEFAULT_CHOICE = "__default";

type Defaults = Partial<Record<"meeting" | "assisted", string>>;

/**
 * Fork-only rows: the note-taking prompts the Google Docs note taker
 * follows, kept like the AI cleanup prompts. Default is the notes engine's
 * own and can't be edited; the user's prompts start from a copy of it.
 * Meetings and Assisted notes each choose one.
 */
export const NotesPromptsRows: React.FC<NotesPromptsRowsProps> = ({
  grouped = false,
}) => {
  const { t } = useTranslation();
  const { getSetting, refreshSettings } = useSettings();
  const prompts: LLMPrompt[] = getSetting("notes_prompts") ?? [];
  const meetingId = getSetting("meeting_notes_prompt_id") ?? null;
  const assistedId = getSetting("assisted_notes_notes_prompt_id") ?? null;

  const [editing, setEditing] = useState<string>(DEFAULT_MEETING);
  const [isCreating, setIsCreating] = useState(false);
  const [draftName, setDraftName] = useState("");
  const [draftText, setDraftText] = useState("");
  const [defaults, setDefaults] = useState<Defaults>({});
  const [defaultError, setDefaultError] = useState<string | null>(null);

  const editingDefault =
    editing === DEFAULT_MEETING || editing === DEFAULT_ASSISTED;
  const defaultKey = editing === DEFAULT_ASSISTED ? "assisted" : "meeting";
  const selected = prompts.find((p) => p.id === editing) ?? null;

  const loadDefault = async (key: "meeting" | "assisted") => {
    if (defaults[key] !== undefined) return defaults[key];
    const result = await commands.defaultNotesPrompt(key === "assisted");
    if (result.status === "error") {
      setDefaultError(result.error);
      return undefined;
    }
    setDefaultError(null);
    setDefaults((current) => ({ ...current, [key]: result.data }));
    return result.data;
  };

  useEffect(() => {
    if (editingDefault) void loadDefault(defaultKey);
    // Loads each Default once, when it is first shown.
  }, [editingDefault, defaultKey]);

  useEffect(() => {
    if (isCreating) return;
    setDraftName(selected?.name ?? "");
    setDraftText(selected?.prompt ?? "");
  }, [isCreating, editing, selected?.name, selected?.prompt]);

  // A deleted prompt leaves the editor on the Default.
  useEffect(() => {
    if (!editingDefault && !isCreating && selected === null) {
      setEditing(DEFAULT_MEETING);
    }
  }, [editingDefault, isCreating, selected]);

  const failed = (error: unknown) =>
    notify({
      title: t("settings.notes.prompts.saveFailed", {
        error: error instanceof Error ? error.message : String(error),
      }),
    });

  const choose = async (assisted: boolean, value: string | null) => {
    const id = !value || value === DEFAULT_CHOICE ? null : value;
    const result = await commands.changeNotesPromptSelection(assisted, id);
    if (result.status === "error") return failed(result.error);
    await refreshSettings();
  };

  const startCreate = async (fromKey: "meeting" | "assisted") => {
    setIsCreating(true);
    setDraftName("");
    setDraftText((await loadDefault(fromKey)) ?? "");
  };

  const create = async () => {
    const result = await commands.addNotesPrompt(
      draftName.trim(),
      draftText.trim(),
    );
    if (result.status === "error") return failed(result.error);
    await refreshSettings();
    setEditing(result.data.id);
    setIsCreating(false);
  };

  const save = async () => {
    if (!selected) return;
    const result = await commands.updateNotesPrompt(
      selected.id,
      draftName.trim(),
      draftText.trim(),
    );
    if (result.status === "error") return failed(result.error);
    await refreshSettings();
  };

  const remove = async () => {
    if (!selected) return;
    const result = await commands.deleteNotesPrompt(selected.id);
    if (result.status === "error") return failed(result.error);
    setEditing(DEFAULT_MEETING);
    await refreshSettings();
  };

  const choiceOptions = [
    { value: DEFAULT_CHOICE, label: t("settings.notes.prompts.default") },
    ...prompts.map((p) => ({ value: p.id, label: p.name })),
  ];
  const editOptions = [
    {
      value: DEFAULT_MEETING,
      label: t("settings.notes.prompts.defaultMeeting"),
    },
    {
      value: DEFAULT_ASSISTED,
      label: t("settings.notes.prompts.defaultAssisted"),
    },
    ...prompts.map((p) => ({ value: p.id, label: p.name })),
  ];
  const isDirty =
    !!selected &&
    (draftName.trim() !== selected.name ||
      draftText.trim() !== selected.prompt.trim());
  const canSubmit = draftName.trim().length > 0 && draftText.trim().length > 0;

  const nameAndText = (
    <>
      <div className="space-y-2 flex flex-col">
        <label className="text-sm font-semibold">
          {t("settings.notes.prompts.name")}
        </label>
        <Input
          type="text"
          value={draftName}
          onChange={(e) => setDraftName(e.target.value)}
          placeholder={t("settings.notes.prompts.namePlaceholder")}
          variant="compact"
        />
      </div>
      <div className="space-y-2 flex flex-col">
        <label className="text-sm font-semibold">
          {t("settings.notes.prompts.instructions")}
        </label>
        <Textarea
          value={draftText}
          onChange={(e) => setDraftText(e.target.value)}
          className="min-h-[200px]"
        />
      </div>
    </>
  );

  return (
    <>
      <SettingContainer
        title={t("settings.notes.prompts.meeting.title")}
        description={t("settings.notes.prompts.meeting.description")}
        descriptionMode="tooltip"
        grouped={grouped}
      >
        <Dropdown
          options={choiceOptions}
          selectedValue={meetingId ?? DEFAULT_CHOICE}
          onSelect={(value) => void choose(false, value)}
        />
      </SettingContainer>
      <SettingContainer
        title={t("settings.notes.prompts.assisted.title")}
        description={t("settings.notes.prompts.assisted.description")}
        descriptionMode="tooltip"
        grouped={grouped}
      >
        <Dropdown
          options={choiceOptions}
          selectedValue={assistedId ?? DEFAULT_CHOICE}
          onSelect={(value) => void choose(true, value)}
        />
      </SettingContainer>
      <SettingContainer
        title={t("settings.notes.prompts.edit.title")}
        description={t("settings.notes.prompts.edit.description")}
        descriptionMode="tooltip"
        layout="stacked"
        grouped={grouped}
      >
        <div className="space-y-3">
          <div className="flex gap-2 min-w-0">
            <Dropdown
              options={editOptions}
              selectedValue={editing}
              onSelect={(value) => {
                if (!value) return;
                setEditing(value);
                setIsCreating(false);
              }}
              disabled={isCreating}
              className="flex-1 min-w-0"
            />
            <Button
              onClick={() => void startCreate(defaultKey)}
              variant="primary"
              size="md"
              disabled={isCreating}
              className="shrink-0"
            >
              {t("settings.notes.prompts.new")}
            </Button>
          </div>

          {isCreating && (
            <div className="space-y-3">
              {nameAndText}
              <div className="flex gap-2 pt-2">
                <Button
                  onClick={() => void create()}
                  variant="primary"
                  size="md"
                  disabled={!canSubmit}
                >
                  {t("settings.notes.prompts.create")}
                </Button>
                <Button
                  onClick={() => setIsCreating(false)}
                  variant="secondary"
                  size="md"
                >
                  {t("settings.notes.prompts.cancel")}
                </Button>
              </div>
            </div>
          )}

          {!isCreating && editingDefault && (
            <div className="space-y-3">
              <p className="text-xs text-ink-3">
                {t("settings.notes.prompts.defaultReadOnly")}
              </p>
              {defaultError !== null ? (
                <p className="text-sm text-ink-3">
                  {t("settings.notes.prompts.defaultFailed", {
                    error: defaultError,
                  })}
                </p>
              ) : (
                <Textarea
                  value={
                    defaults[defaultKey] ??
                    t("settings.notes.prompts.defaultLoading")
                  }
                  readOnly
                  className="w-full min-h-[200px]"
                />
              )}
              <div className="flex gap-2 pt-2">
                <Button
                  onClick={() => void startCreate(defaultKey)}
                  variant="secondary"
                  size="md"
                  disabled={defaults[defaultKey] === undefined}
                >
                  {t("settings.notes.prompts.copyDefault")}
                </Button>
              </div>
            </div>
          )}

          {!isCreating && selected && (
            <div className="space-y-3">
              {nameAndText}
              <div className="flex gap-2 pt-2">
                <Button
                  onClick={() => void save()}
                  variant="primary"
                  size="md"
                  disabled={!canSubmit || !isDirty}
                >
                  {t("settings.notes.prompts.save")}
                </Button>
                <Button
                  onClick={() => void remove()}
                  variant="secondary"
                  size="md"
                >
                  {t("settings.notes.prompts.delete")}
                </Button>
              </div>
            </div>
          )}
        </div>
      </SettingContainer>
    </>
  );
};
