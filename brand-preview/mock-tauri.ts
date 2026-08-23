/*
 * A fake Tauri IPC layer, so the real settings UI runs in a plain browser.
 *
 * NOT part of the app bundle — nothing under `src/` imports this, and nothing
 * here is loaded unless `brand-preview/index.html` is the entry point.
 *
 * Every Tauri JS API — `@tauri-apps/api/core`'s `invoke`, the event plugin, the
 * generated `commands` object in `src/bindings.ts` — ultimately calls
 * `window.__TAURI_INTERNALS__.invoke(cmd, args)`. Installing a handler there
 * before any Tauri module is imported is the whole seam: the app cannot tell it
 * is not talking to Rust, so `useSettings()` resolves and the real components
 * render.
 *
 * That installation is Tauri's own `mockIPC` from `@tauri-apps/api/mocks`, the
 * documented way to run a Tauri frontend outside Tauri
 * (https://v2.tauri.app/develop/tests/mocking/). Writing to
 * `__TAURI_INTERNALS__` by hand also works, but leaves out the pieces the
 * module already gets right — `transformCallback`'s callback registry, and
 * `__TAURI_EVENT_PLUGIN_INTERNALS__.unregisterListener`, whose absence throws
 * on every `listen()` cleanup. `shouldMockEvents` gives listen/emit a working
 * in-page implementation for free.
 *
 * Two things `mockIPC` does not cover:
 *
 *   - `@tauri-apps/plugin-os`. `type()`, `platform()`, `family()` and friends
 *     are synchronous: they read `window.__TAURI_OS_PLUGIN_INTERNALS__`
 *     directly instead of going through `invoke`, so no IPC mock can reach
 *     them and the object has to be defined here. Without it
 *     `type() === "linux"` throws on a property of undefined. (`locale()` and
 *     `hostname()` are the only two that are commands.)
 *   - Unknown commands. `mockIPC` passes everything to one callback; the
 *     fallback below warns and resolves to `null` rather than rejecting,
 *     because a rejection escaping a `useEffect` can blank the page and hide
 *     every other thing the screenshot is meant to show. A warning names the
 *     gap and leaves the rest rendered.
 *
 * Command names here are the Rust-side snake_case names, not the camelCase
 * wrappers: `commands.getAppSettings()` calls `TAURI_INVOKE("get_app_settings")`.
 * See `src/bindings.ts`.
 */

import {
  mockConvertFileSrc,
  mockIPC,
  mockWindows,
} from "@tauri-apps/api/mocks";

import type {
  AppSettings,
  AudioDevice,
  ModelInfo,
  ShortcutBinding,
} from "@/bindings";

/** The platform the preview pretends to be. */
const OS_TYPE = "windows";

const binding = (
  id: string,
  name: string,
  description: string,
  keys: string,
): ShortcutBinding => ({
  id,
  name,
  description,
  default_binding: keys,
  current_binding: keys,
});

/*
 * A complete `AppSettings`, shaped after `src/bindings.ts` and valued after the
 * Rust defaults in `src-tauri/src/settings.rs` (`get_default_settings`), with
 * the Windows arm of every `#[cfg(target_os)]` default taken to match OS_TYPE
 * above.
 *
 * Deliberately the *default* settings rather than a well-used profile: the
 * screenshots are meant to answer "what does a new user meet", and a mock
 * seeded with everything switched on would answer a different question.
 *
 * Three fields depart from the Rust defaults, each because the default hides
 * the thing being photographed rather than describing it:
 *   - `dictation.enabled` (default false) is true, so the Dictation tab shows
 *     its rows instead of one toggle above a column of disabled ones.
 *   - `onboarding_completed` (default false) is true, so the app opens on
 *     settings rather than the onboarding wizard.
 *   - `selected_model` (default "") names a downloaded model, so the footer's
 *     model picker reads as a working app rather than a fresh install.
 */
const SETTINGS: AppSettings = {
  settings_schema_version: 2,
  bindings: {
    transcribe: binding(
      "transcribe",
      "Transcribe",
      "Converts your speech into text.",
      "ctrl+alt+space",
    ),
    transcribe_with_post_process: binding(
      "transcribe_with_post_process",
      "Transcribe with Post-Processing",
      "Converts your speech into text and applies AI post-processing.",
      "ctrl+alt+shift+space",
    ),
    cancel: binding(
      "cancel",
      "Cancel",
      "Cancels the current recording.",
      "escape",
    ),
    dictate: binding(
      "dictate",
      "Dictate",
      "Converts your speech into text and pastes it into the focused window.",
      "ctrl+space",
    ),
    dictate_with_post_process: binding(
      "dictate_with_post_process",
      "Dictate with Post-Processing",
      "Converts your speech into text, applies AI post-processing, and pastes it into the focused window.",
      "ctrl+shift+space",
    ),
  },
  push_to_talk: true,
  audio_feedback: false,
  audio_feedback_volume: 1.0,
  sound_theme: "marimba",
  start_hidden: false,
  autostart_enabled: false,
  update_checks_enabled: true,
  show_whats_new_on_update: true,
  whats_new_last_seen_version: "0.7.0",
  selected_model: "parakeet-tdt-0.6b-v3",
  onboarding_completed: true,
  always_on_microphone: false,
  selected_microphone: null,
  selected_channel: null,
  clamshell_microphone: null,
  selected_output_device: null,
  system_audio_enabled: false,
  follow_stream_enabled: false,
  system_audio_device: null,
  translate_to_english: false,
  selected_language: "auto",
  overlay_position: "bottom",
  debug_mode: false,
  log_level: "debug",
  custom_words: [],
  model_unload_timeout: "never",
  word_correction_threshold: 0.18,
  history_limit: 5,
  recording_retention_period: "preserve_limit",
  save_recordings: false,
  save_transcripts: false,
  paste_method: "none",
  clipboard_handling: "dont_modify",
  auto_submit: false,
  auto_submit_key: "enter",
  post_process_enabled: false,
  post_process_provider_id: "openai",
  post_process_providers: [
    {
      id: "openai",
      label: "OpenAI",
      base_url: "https://api.openai.com/v1",
      allow_base_url_edit: false,
      models_endpoint: "/models",
      supports_structured_output: true,
    },
    {
      id: "openrouter",
      label: "OpenRouter",
      base_url: "https://openrouter.ai/api/v1",
      allow_base_url_edit: false,
      models_endpoint: "/models",
      supports_structured_output: true,
    },
    {
      id: "anthropic",
      label: "Anthropic",
      base_url: "https://api.anthropic.com/v1",
      allow_base_url_edit: false,
      models_endpoint: "/models",
      supports_structured_output: false,
    },
  ],
  post_process_api_keys: { openai: "", openrouter: "", anthropic: "" },
  post_process_models: { openai: "", openrouter: "", anthropic: "" },
  post_process_prompts: [
    {
      id: "default_improve_transcriptions",
      name: "Improve Transcriptions",
      prompt: "<transcript>\n${output}\n</transcript>\n\nClean it up.",
    },
  ],
  post_process_selected_prompt_id: null,
  mute_while_recording: false,
  append_trailing_space: false,
  app_language: "en",
  theme: "system",
  experimental_enabled: false,
  lazy_stream_close: false,
  keyboard_implementation: "tauri",
  show_tray_icon: true,
  paste_delay_ms: 60,
  paste_delay_after_ms: 60,
  reliable_paste: false,
  typing_tool: "auto",
  external_script_path: null,
  filler_word_removal_enabled: true,
  custom_filler_words: null,
  transcribe_accelerator: "auto",
  ort_accelerator: "auto",
  transcribe_gpu_device: -1,
  extra_recording_buffer_ms: 0,
  vad_enabled: true,
  overlay_style: "live",
  show_all_settings: false,
  dictation: {
    enabled: true,
    push_to_talk: true,
    paste_method: "ctrl_v",
    clipboard_handling: "dont_modify",
    auto_submit: false,
    auto_submit_key: "enter",
    append_trailing_space: false,
    typing_tool: "auto",
    overlay_style: "minimal",
    save_recordings: false,
    save_transcripts: false,
    post_process_enabled: false,
    post_process_selected_prompt_id: null,
  },
};

const AUDIO_DEVICES: AudioDevice[] = [
  { index: "default", name: "Default", is_default: true },
  { index: "1", name: "Yeti Nano", is_default: false },
  { index: "2", name: "Headset Microphone (Jabra Evolve)", is_default: false },
];

const OUTPUT_DEVICES: AudioDevice[] = [
  { index: "default", name: "Default", is_default: true },
  { index: "1", name: "Speakers (Realtek Audio)", is_default: false },
];

const model = (
  id: string,
  name: string,
  description: string,
  overrides: Partial<ModelInfo> = {},
): ModelInfo => ({
  id,
  name,
  description,
  filename: `${id}.bin`,
  source: {
    Url: { url: `https://blob.handy.computer/${id}.bin`, sha256: null },
  },
  size_mb: 600,
  is_downloaded: false,
  is_downloading: false,
  partial_size: 0,
  is_directory: false,
  engine_type: "Parakeet",
  accuracy_score: 7,
  speed_score: 8,
  supports_translation: false,
  is_recommended: false,
  supported_languages: ["en"],
  supports_language_selection: false,
  is_custom: false,
  supports_streaming: false,
  supports_language_detection: false,
  ...overrides,
});

const MODELS: ModelInfo[] = [
  model(
    "parakeet-tdt-0.6b-v3",
    "Parakeet V3",
    "Fast multilingual model, English and 24 European languages.",
    {
      is_downloaded: true,
      is_recommended: true,
      accuracy_score: 8,
      speed_score: 9,
      supports_streaming: true,
    },
  ),
  model(
    "whisper-large-v3-turbo",
    "Whisper Large V3 Turbo",
    "Most accurate, multilingual, slower on CPU.",
    {
      size_mb: 1600,
      engine_type: "TranscribeCpp",
      accuracy_score: 9,
      speed_score: 5,
      supports_translation: true,
      supports_language_selection: true,
      supports_language_detection: true,
    },
  ),
  model("whisper-small", "Whisper Small", "A smaller, faster Whisper.", {
    size_mb: 466,
    engine_type: "TranscribeCpp",
    accuracy_score: 6,
    speed_score: 7,
    supports_language_selection: true,
  }),
];

/**
 * Canned responses, keyed by the Rust command name.
 *
 * A handler may mutate SETTINGS: the settings store applies updates
 * optimistically and only rolls back if the command rejects, so persisting is
 * not what makes a click stick on screen — but a later `get_app_settings`
 * (the store refreshes on several events) would otherwise hand back the old
 * value and silently undo it.
 */
const HANDLERS: Record<string, (args: any) => unknown> = {
  get_app_settings: () => SETTINGS,
  get_default_settings: () => SETTINGS,

  change_show_all_settings_setting: ({ enabled }) => {
    SETTINGS.show_all_settings = enabled;
    return null;
  },
  change_dictation_settings: ({ dictation }) => {
    SETTINGS.dictation = dictation;
    return null;
  },
  change_ptt_setting: ({ enabled }) => {
    SETTINGS.push_to_talk = enabled;
    return null;
  },
  change_post_process_enabled_setting: ({ enabled }) => {
    SETTINGS.post_process_enabled = enabled;
    return null;
  },
  change_overlay_style_setting: ({ style }) => {
    SETTINGS.overlay_style = style;
    return null;
  },
  change_save_recordings_setting: ({ enabled }) => {
    SETTINGS.save_recordings = enabled;
    return null;
  },
  change_save_transcripts_setting: ({ enabled }) => {
    SETTINGS.save_transcripts = enabled;
    return null;
  },
  change_binding: ({ id, binding: keys }) => {
    const existing = SETTINGS.bindings?.[id];
    if (existing) existing.current_binding = keys;
    return { success: true, binding: existing ?? null, error: null };
  },
  reset_binding: ({ id }) => {
    const existing = SETTINGS.bindings?.[id];
    if (existing) existing.current_binding = existing.default_binding;
    return { success: true, binding: existing ?? null, error: null };
  },

  get_available_microphones: () => AUDIO_DEVICES,
  get_available_output_devices: () => OUTPUT_DEVICES,
  get_selected_microphone: () => "default",
  get_selected_output_device: () => "default",
  get_clamshell_microphone: () => "default",
  get_microphone_channels: () => 2,
  // Drives whether the Audio section offers the clamshell-microphone row.
  // A laptop, so the row is on screen rather than silently absent.
  is_laptop: () => true,
  get_microphone_mode: () => false,
  check_custom_sounds: () => ({ start: false, stop: false }),
  is_recording: () => false,

  get_available_models: () => MODELS,
  get_model_info: ({ modelId }) => MODELS.find((m) => m.id === modelId) ?? null,
  get_current_model: () => SETTINGS.selected_model,
  get_transcription_model_status: () => "loaded",
  get_model_load_status: () => ({
    is_loaded: true,
    current_model: SETTINGS.selected_model,
  }),
  is_model_loading: () => false,
  rescan_local_models: () => null,

  get_available_accelerators: () => ({
    transcribe: ["auto", "cpu", "gpu"],
    ort: ["auto", "cpu", "directml"],
    gpu_devices: [],
  }),
  get_available_typing_tools: () => ["auto"],
  get_keyboard_implementation: () => "tauri",
  get_secure_input_status: () => ({
    enabled: false,
    sustained: false,
    culprit_pid: null,
    culprit_name: null,
    fallback_active: false,
    covered_bindings: [],
    degraded_bindings: [],
    uncovered_bindings: [],
    recorder_blocked: false,
  }),
  get_windows_microphone_permission_status: () => ({
    supported: true,
    overall_access: "allowed",
    device_access: "allowed",
    app_access: "allowed",
    desktop_app_access: "allowed",
  }),
  check_apple_intelligence_available: () => false,
  is_portable: () => false,
  get_app_dir_path: () => "C:\\Users\\preview\\AppData\\Roaming\\shorthand",
  get_log_dir_path: () =>
    "C:\\Users\\preview\\AppData\\Roaming\\shorthand\\logs",
  get_history_entries: () => ({ entries: [], has_more: false }),
  fetch_post_process_models: () => [],

  suspend_all_bindings: () => null,
  resume_all_bindings: () => null,
  initialize_enigo: () => null,
  initialize_shortcuts: () => null,

  // Plugin commands. `plugin:event|*` is absent on purpose: `shouldMockEvents`
  // below hands those to Tauri's own in-page implementation before this table
  // is consulted.
  //
  // `plugin:updater|check` returning null is not a stub — null is the
  // documented "no update available" result, which is what the preview wants.
  "plugin:updater|check": () => null,
  "plugin:os|locale": () => "en-US",
  "plugin:os|hostname": () => "preview",
  "plugin:app|version": () => "0.7.0-preview",
  "plugin:app|name": () => "Shorthand",
  "plugin:macos-permissions|check_accessibility_permission": () => true,
  "plugin:macos-permissions|check_microphone_permission": () => true,
};

/** Commands that were hit but not handled, reported once each. */
const missingCommands = new Set<string>();

mockIPC(
  (cmd, args) => {
    const handler = HANDLERS[cmd];
    if (handler) return handler(args ?? {});

    if (!missingCommands.has(cmd)) {
      missingCommands.add(cmd);
      console.warn(`[mock-tauri] unhandled command: ${cmd}`, args);
    }
    return null;
  },
  { shouldMockEvents: true },
);

// The window the settings UI runs in. `@tauri-apps/api/window`'s
// `getCurrentWindow()` reads this label at import time.
mockWindows("main");
mockConvertFileSrc(OS_TYPE);

declare global {
  interface Window {
    __TAURI_OS_PLUGIN_INTERNALS__: Record<string, unknown>;
  }
}

window.__TAURI_OS_PLUGIN_INTERNALS__ = {
  os_type: OS_TYPE,
  platform: OS_TYPE,
  family: "windows",
  version: "10.0.26200",
  arch: "x86_64",
  exe_extension: "exe",
  eol: "\r\n",
};
