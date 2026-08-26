/*
 * Component gallery — NOT part of the app bundle.
 *
 * A flat page of the settings UI's presentational primitives: every Button
 * variant at once, the Badge fix, the resolved brand tokens, and the same rows
 * in upstream's `SettingsGroup` card ("Now") beside the fork's borderless
 * `Sheet` ("Proposed"), so the two containers can be compared in one shot
 * rather than across two.
 *
 * This is the deliberately *unreal* half of the harness and stays that way. It
 * imports only from `@/components/ui/` and hand-rolls the sidebar and tab
 * markup, so it renders with no Tauri mock at all — which is what makes it
 * usable for comparing a primitive against its replacement, side by side, in a
 * layout the app never actually produces. The real settings window, running the
 * real components against a fake IPC layer, is `index.html` / `preview.tsx`.
 *
 * Run:  bun x vite dev --port 5199
 *       http://localhost:5199/brand-preview/gallery.html
 *
 * Do not add this directory to .gitignore: Tailwind v4 skips gitignored files
 * when scanning for class names, so any class used only here would silently
 * fail to compile.
 */

import React from "react";
import ReactDOM from "react-dom/client";
import {
  Mic,
  Captions,
  Cpu,
  Sparkles,
  AppWindow,
  History,
  Info,
} from "lucide-react";

import "@/App.css";

import { Button } from "@/components/ui/Button";
import Badge from "@/components/ui/Badge";
import { Input } from "@/components/ui/Input";
import { Select } from "@/components/ui/Select";
import { SettingContainer } from "@/components/ui/SettingContainer";
import { SettingsGroup } from "@/components/ui/SettingsGroup";
import { ToggleSwitch } from "@/components/ui/ToggleSwitch";
import { ShorthandWordmark } from "@/shorthand/brand";
import { Sheet } from "@/shorthand/ui/Sheet";

const SIDEBAR_ROWS = [
  { id: "modes", label: "Modes", icon: Mic },
  { id: "audio", label: "Audio", icon: Captions },
  { id: "model", label: "Model", icon: Cpu },
  { id: "cleanup", label: "AI cleanup", icon: Sparkles },
  { id: "app", label: "App", icon: AppWindow },
  { id: "history", label: "History", icon: History },
  { id: "about", label: "About", icon: Info },
] as const;

const BUTTON_VARIANTS = [
  "primary",
  "primary-soft",
  "secondary",
  "warning",
  "danger",
  "danger-ghost",
  "ghost",
] as const;

const SWATCH_TOKENS = [
  "--color-background",
  "--color-text",
  "--color-logo-primary",
  "--color-background-ui",
  "--color-mid-gray",
  "--brand-highlighter",
] as const;

const MODEL_OPTIONS = [
  { value: "parakeet-v3", label: "Parakeet v3 (English, fast)" },
  { value: "whisper-turbo", label: "Whisper Large v3 Turbo" },
  { value: "whisper-small", label: "Whisper Small" },
];

/** The chip markup GlobalShortcutInput renders, without its Tauri listeners. */
const ShortcutChips: React.FC<{ keys: string[] }> = ({ keys }) => (
  <div className="flex items-center space-x-1">
    {keys.map((key, index) => (
      <React.Fragment key={key}>
        <span className="px-2 py-1 text-sm font-semibold bg-mid-gray/10 border border-mid-gray/80 hover:bg-logo-primary/10 rounded-md cursor-pointer hover:border-logo-primary">
          {key}
        </span>
        {index < keys.length - 1 && (
          <span className="text-sm text-mid-gray">+</span>
        )}
      </React.Fragment>
    ))}
  </div>
);

/** Resolved custom-property values, re-read whenever the theme attribute flips. */
function useResolvedTokens(names: readonly string[]) {
  const [values, setValues] = React.useState<string[]>(() =>
    names.map(() => ""),
  );

  React.useEffect(() => {
    const read = () => {
      const style = getComputedStyle(document.documentElement);
      setValues(names.map((name) => style.getPropertyValue(name).trim()));
    };
    read();

    const observer = new MutationObserver(read);
    observer.observe(document.documentElement, {
      attributes: true,
      attributeFilter: ["data-theme"],
    });
    return () => observer.disconnect();
    // `names` is a module-level constant; re-running on identity is pointless.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  return values;
}

const Swatches: React.FC = () => {
  const values = useResolvedTokens(SWATCH_TOKENS);

  return (
    <div className="flex flex-wrap gap-3">
      {SWATCH_TOKENS.map((token, index) => (
        <div
          key={token}
          className="flex items-center gap-2 border border-mid-gray/20 rounded-lg px-2 py-1.5"
        >
          <span
            className="w-8 h-8 rounded-md border border-mid-gray/40 shrink-0"
            style={{ backgroundColor: `var(${token})` }}
          />
          <span className="flex flex-col leading-tight">
            <span className="text-xs font-medium">{token}</span>
            <span className="text-xs font-mono text-mid-gray">
              {values[index] || "—"}
            </span>
          </span>
        </div>
      ))}
    </div>
  );
};

/** Small label for the harness's own scaffolding, not for the app's UI. */
const SectionLabel: React.FC<{ children: React.ReactNode }> = ({
  children,
}) => (
  <h2 className="text-xs font-medium text-mid-gray uppercase tracking-wide px-4">
    {children}
  </h2>
);

interface RowsProps {
  toggle: boolean;
  setToggle: (value: boolean) => void;
  model: string | null;
  setModel: (value: string | null) => void;
  words: string;
  setWords: (value: string) => void;
}

/**
 * The settings rows, rendered identically inside both containers. Both copies
 * share one piece of state on purpose: the columns are meant to differ only in
 * their container, so anything that differs in a screenshot is the container.
 */
const SettingRows: React.FC<RowsProps> = ({
  toggle,
  setToggle,
  model,
  setModel,
  words,
  setWords,
}) => (
  <>
    <ToggleSwitch
      checked={toggle}
      onChange={setToggle}
      label="Remove filler words"
      description="Strip um, uh and similar disfluencies from the transcript."
      grouped={true}
    />

    <SettingContainer
      title="Model"
      description="The speech-to-text model used for this mode."
      grouped={true}
    >
      <div className="w-56">
        <Select
          value={model}
          options={MODEL_OPTIONS}
          onChange={(value) => setModel(value)}
          isClearable={false}
          placeholder="Select a model"
        />
      </div>
    </SettingContainer>

    <SettingContainer
      title="Custom words"
      description="Names and jargon the model should prefer."
      grouped={true}
    >
      <Input
        variant="compact"
        value={words}
        onChange={(event) => setWords(event.target.value)}
        className="w-56"
      />
    </SettingContainer>

    <SettingContainer
      title="Shortcut"
      description="Press to start and stop recording."
      grouped={true}
    >
      <ShortcutChips keys={["Ctrl", "Shift", "Space"]} />
    </SettingContainer>

    <SettingContainer
      title="Save transcripts"
      description="Kept on this machine only, and never sent anywhere."
      descriptionMode="inline"
      grouped={true}
    >
      <Button variant="secondary" size="sm">
        Open folder
      </Button>
    </SettingContainer>
  </>
);

const Preview: React.FC = () => {
  const [activeSection, setActiveSection] = React.useState<string>("modes");
  const [activeTab, setActiveTab] = React.useState<
    "transcription" | "dictation"
  >("transcription");
  const [toggle, setToggle] = React.useState(true);
  const [model, setModel] = React.useState<string | null>("parakeet-v3");
  const [words, setWords] = React.useState("Shorthand, Tauri, Obsidian");

  const rowProps: RowsProps = {
    toggle,
    setToggle,
    model,
    setModel,
    words,
    setWords,
  };

  return (
    <div className="h-screen flex flex-col select-none cursor-default">
      <div className="flex-1 flex overflow-hidden">
        {/* Hand-rolled copy of components/Sidebar.tsx's markup — the real one
            calls useSettings() to decide which rows are visible. */}
        <div className="flex flex-col w-40 h-full border-e border-mid-gray/20 items-center px-2">
          <ShorthandWordmark height={24} className="m-4" />
          <div className="flex flex-col w-full items-center gap-1 pt-2 border-t border-mid-gray/20">
            {SIDEBAR_ROWS.map((section) => {
              const Icon = section.icon;
              const isActive = activeSection === section.id;

              return (
                <div
                  key={section.id}
                  className={`p-2 w-full rounded-lg cursor-pointer transition-colors hover:bg-mid-gray/15 ${
                    isActive ? "" : "hover:opacity-100 opacity-70"
                  }`}
                  onClick={() => setActiveSection(section.id)}
                >
                  {/* No sweep here, and no background or border on the row
                      either. A sidebar label is too short to hold a stroke:
                      "Modes" renders a 44x22 mark, 1.9:1, and "App" a 28x22
                      square, at which size the corner radii eat the whole
                      perimeter and the pen line detaches into a second object.
                      See the aspect-ratio table in `brand/marks.css`.

                      The selection is carried by the accent icon against
                      dimmed neighbours instead. The icon is the load-bearing
                      half: with only the label changing, the row read
                      half-selected. */}
                  <div className="flex gap-2 items-center">
                    <Icon
                      width={24}
                      height={24}
                      className={`shrink-0 ${isActive ? "text-logo-primary" : ""}`}
                    />
                    <p className="text-sm font-medium truncate">
                      {section.label}
                    </p>
                  </div>
                </div>
              );
            })}
          </div>
        </div>

        {/* Content pane */}
        <div className="flex-1 overflow-y-auto">
          <div className="flex flex-col items-center p-4 gap-4">
            <div className="max-w-[1200px] w-full mx-auto space-y-6">
              {/* Tab bar */}
              <div className="flex items-center gap-1 border-b border-mid-gray/20">
                {(
                  [
                    ["transcription", "Transcription"],
                    ["dictation", "Dictation"],
                  ] as const
                ).map(([id, label]) => {
                  const isActive = activeTab === id;
                  return (
                    <button
                      key={id}
                      onClick={() => setActiveTab(id)}
                      className={`relative px-4 py-2 text-sm font-medium cursor-pointer transition-colors ${
                        isActive ? "" : "text-mid-gray hover:text-text"
                      }`}
                    >
                      {/* A plain coral underline on the active tab — see
                          brand/marks.css's `.sh-tab-indicator`. */}
                      {label}
                      {isActive && (
                        <span className="sh-tab-indicator" aria-hidden="true" />
                      )}
                    </button>
                  );
                })}
              </div>

              {/* Now / Proposed: the same rows, a different container. */}
              <div className="grid grid-cols-2 gap-8 items-start">
                <div className="space-y-3">
                  <SectionLabel>Now</SectionLabel>
                  <SettingsGroup
                    title="Transcription"
                    description="How Shorthand turns what you said into text."
                  >
                    <SettingRows {...rowProps} />
                  </SettingsGroup>
                </div>

                <div className="space-y-3">
                  <SectionLabel>Proposed</SectionLabel>
                  <Sheet
                    title="Transcription"
                    description="How Shorthand turns what you said into text."
                  >
                    <SettingRows {...rowProps} />
                  </Sheet>
                </div>
              </div>

              <div className="space-y-2">
                <SectionLabel>Buttons</SectionLabel>
                <div className="flex flex-wrap items-center gap-2 px-4">
                  {BUTTON_VARIANTS.map((variant) => (
                    <Button key={variant} variant={variant}>
                      {variant}
                    </Button>
                  ))}
                </div>
              </div>

              <div className="space-y-2">
                <SectionLabel>Badge</SectionLabel>
                {/* `primary` paints bg-logo-primary but sets no foreground, so
                    the label inherits --color-text and goes ink-on-ink. The
                    second badge is the proposed fix and nothing else. */}
                <div className="flex items-center gap-3 px-4">
                  <Badge variant="primary">Recommended (now)</Badge>
                  <Badge variant="primary" className="text-background">
                    Recommended (proposed)
                  </Badge>
                </div>
              </div>

              <div className="space-y-2">
                <SectionLabel>Resolved tokens</SectionLabel>
                <div className="px-4">
                  <Swatches />
                </div>
              </div>
            </div>
          </div>
        </div>
      </div>
    </div>
  );
};

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <Preview />
  </React.StrictMode>,
);
