/**
 * Fork-only. Turns upstream Handy's translation catalogues into Shorthand's.
 *
 * The catalogues under `src/i18n/locales/` are kept byte-identical to
 * upstream, so `git merge upstream/main` never conflicts on them and new
 * upstream strings arrive cleanly. The rename happens here instead, applied at
 * build time by the Vite plugin in `./vite-branding-plugin.ts`.
 *
 * Editing the locale files directly was rejected: it would put ~400 changed
 * lines into the 24 files upstream churns most, and — worse — every future
 * upstream string containing "Handy" would arrive unrenamed with nothing to
 * catch it. `scripts/check-branding.ts` is the guard that makes this
 * approach safe rather than merely cheap.
 *
 * Order matters: substitution runs first, then fork-only strings are merged on
 * top. That is why the strings below can say "Handy" and mean it — they are
 * never passed through the substitution.
 */

const BRAND_FROM = "Handy";
const BRAND_TO = "Shorthand";

/**
 * Strings that exist only in this fork. Merged in after substitution, so they
 * are authoritative and immune to the rename.
 *
 * English only, deliberately. These never reach the locale files, so
 * `check:translations` — which compares key parity between `en` and the other
 * 23 catalogues on disk — never sees them and cannot fail on them. i18next's
 * configured `fallbackLng: "en"` renders them in every locale.
 */
export const FORK_ONLY_STRINGS: Record<string, string> = {
  "sidebar.capture": "Capture",
  "sidebar.transcription": "Transcription",
  "sidebar.app": "App",

  // ---- The redesigned settings sections --------------------------------
  "sidebar.modes": "Modes",
  "sidebar.audio": "Audio",
  "sidebar.model": "Model",
  "sidebar.aiCleanup": "AI cleanup",

  "settings.modes.heading": "How each mode behaves",
  "settings.modes.description":
    "Transcription streams a meeting to whatever is following along. Dictation types what you say into whatever window you are in.",
  "settings.modes.tabs.label": "Capture mode",
  "settings.modes.tabs.transcription": "Transcription",
  "settings.modes.tabs.dictation": "Dictation",
  "settings.modes.shared.title": "Shared by both modes",
  "settings.modes.shared.description":
    "One setting, used by whichever mode is recording.",

  "settings.audio.groups.input": "Where sound comes from",
  "settings.model.groups.output": "How transcripts read",
  "settings.aiCleanup.title": "How AI cleans up what you said",

  // ---- Sentence case, and one name per feature -------------------------
  //
  // Upstream labels settings in Title Case; the redesign's copy rule is
  // sentence case, because a settings row is a phrase, not a heading, and Title
  // Case makes every row look equally important.
  //
  // This is all-or-nothing on purpose. An earlier pass overrode three labels
  // and left the rest, which moved the inconsistency from between-tabs to
  // *within* a single screen — "Capture Shortcut / Push to talk / Overlay /
  // AI cleanup / Save recordings" in one list. Half-converted reads as a bug in
  // a way that uniformly Title Case did not. So every Title Case label the
  // settings tree renders is overridden here.
  //
  // Acronyms and proper nouns keep their capitals: API, URL, ONNX, English,
  // Handy, Beta, What's New.
  //
  // Done here rather than in the locale files for the usual reason: those stay
  // byte-identical to upstream so merges never conflict on them, and
  // check:translations compares key parity against en, which these never enter.
  // The cost is that they are English-only — acceptable, because the alternative
  // is 24 files of churn in the files upstream touches most.
  "appLanguage.title": "Application language",
  "settings.about.appDataDirectory.title": "App data directory",
  "settings.about.sourceCode.title": "Source code",
  "settings.about.supportDevelopment.title": "Support development",
  "settings.advanced.acceleration.ort.title": "ONNX acceleration",
  "settings.advanced.autoSubmit.title": "Auto submit",
  "settings.advanced.autostart.label": "Launch on startup",
  "settings.advanced.clipboardHandling.title": "Clipboard handling",
  "settings.advanced.customWords.title": "Custom words",
  "settings.advanced.experimentalToggle.label": "Experimental features",
  "settings.advanced.fillerWordRemoval.title": "Remove filler words",
  "settings.advanced.followStream.label": "Follow live transcript output",
  "settings.advanced.lazyStreamClose.label":
    "Keep mic open between transcriptions",
  "settings.advanced.modelUnload.title": "Unload model",
  "settings.advanced.overlay.position.title": "Overlay position",
  "settings.advanced.pasteMethod.title": "Paste method",
  "settings.advanced.showTrayIcon.label": "Show tray icon",
  "settings.advanced.startHidden.label": "Start hidden",
  "settings.advanced.systemAudio.label": "Capture system audio",
  "settings.advanced.systemAudioDevice.title": "System audio device",
  "settings.advanced.typingTool.title": "Typing tool",
  "settings.advanced.voiceActivityDetection.title": "Voice activity detection",
  "settings.debug.appendTrailingSpace.label": "Append trailing space",
  "settings.debug.clamshellMicrophone.title": "Clamshell microphone",
  "settings.debug.historyLimit.title": "History limit",
  "settings.debug.keyboardDiagnostic.title": "Keyboard diagnostic",
  "settings.debug.keyboardImplementation.title": "Keyboard implementation",
  "settings.debug.liveLogs.title": "Live logs",
  "settings.debug.logDirectory.title": "Log directory",
  "settings.debug.logLevel.title": "Log level",
  "settings.debug.muteWhileRecording.label": "Mute while recording",
  "settings.debug.postProcessingToggle.label": "AI cleanup",
  "settings.debug.recordingBuffer.title": "Extra recording buffer",
  "settings.debug.reliablePaste.title": "Reliable paste (Beta)",
  "settings.debug.soundTheme.label": "Sound theme",
  "settings.debug.updateChecks.label": "Check for updates",
  "settings.debug.wordCorrectionThreshold.title": "Word correction threshold",
  "settings.dictation.enable.label": "Enable dictation",
  "settings.dictation.privacy.saveRecordings.label": "Save recordings",
  "settings.dictation.privacy.saveTranscripts.label": "Save transcripts",
  "settings.general.pushToTalk.label": "Push to talk",
  "settings.general.shortcut.title": "Handy shortcuts",
  "settings.models.title": "Transcription models",
  "settings.postProcessing.api.apiKey.title": "API key",
  "settings.postProcessing.prompts.selectedPrompt.title": "Selected prompt",
  "settings.sound.audioFeedback.label": "Audio feedback",
  "settings.sound.channel.title": "Input channel",
  "settings.sound.outputDevice.title": "Output device",
  "theme.title": "Application theme",

  // These two say "Handy" on purpose: they name the upstream project.
  "settings.about.showAllSettings.label": "Show all Handy settings",
  "settings.about.showAllSettings.description":
    "Reveal every setting and transcription model from upstream Handy, including the ones Shorthand hides.",
  "settings.privacy.title": "Privacy",
  "settings.privacy.saveRecordings.label": "Save recordings",
  "settings.privacy.saveRecordings.description":
    "Keep the audio of each transcription in your recordings folder so you can play it back or re-transcribe it. Off by default; no recording is kept.",
  "settings.privacy.saveTranscripts.label": "Save transcripts",
  "settings.privacy.saveTranscripts.description":
    "Keep the text of each transcription in your local history. Off by default; no transcript is kept.",
  "settings.history.transcriptNotSaved": "Transcript not saved.",
};

/**
 * Dotted key paths in the UPSTREAM catalogues whose value must keep saying
 * "Handy" — for example a string that credits the upstream project by name.
 *
 * Empty today. The mechanism exists because the alternative is discovering the
 * need for it by shipping a wrong string, and because `check-branding` asserts
 * that every key listed here still contains the word, which catches upstream
 * rewording a sentence out from under a stale entry.
 */
export const BRAND_EXEMPT_KEYS: ReadonlySet<string> = new Set<string>([]);

export interface BrandingWarning {
  locale: string;
  key: string;
  value: string;
  reason: string;
}

export interface BrandingResult {
  translation: Record<string, unknown>;
  warnings: BrandingWarning[];
}

/**
 * Matches the product name as a standalone word, optionally carrying a
 * Scandinavian/German genitive `s`. The genitive matters: Danish writes
 * "Handys lokale tale-til-tekst", and a bare `\bHandy\b` leaves that as
 * "Handys" while rebranding everything around it.
 *
 * Compounds like "Handy-Symbol" are already covered, because a hyphen is a
 * word boundary.
 */
const WORD_BOUNDED = new RegExp(`\\b${BRAND_FROM}(s?)\\b`, "g");
// Deliberately not global: `.test()` on a global regex advances `lastIndex`
// between calls, so it would alternate true/false across strings.
const ANY_OCCURRENCE = new RegExp(BRAND_FROM);

/**
 * German uses "Handy" as the everyday word for a mobile phone, so a
 * substitution there could in principle corrupt real prose rather than rename
 * the product. No current string hits that case — every German match is a
 * product reference — but the risk is specific to German and worth surfacing
 * when the text changes.
 *
 * Restricted to `de` deliberately: warning on every language's compounds
 * produced twelve false alarms and trained the reader to skim past them.
 */
const FALSE_FRIEND_LOCALES: ReadonlySet<string> = new Set(["de"]);

function setByPath(
  target: Record<string, unknown>,
  path: string,
  value: string,
): void {
  const parts = path.split(".");
  let cursor = target;
  for (const part of parts.slice(0, -1)) {
    const next = cursor[part];
    if (typeof next !== "object" || next === null) {
      cursor[part] = {};
    }
    cursor = cursor[part] as Record<string, unknown>;
  }
  cursor[parts[parts.length - 1]] = value;
}

/**
 * Rebrand one locale's catalogue. Pure: the input is never mutated, so the
 * same function backs both the build plugin and the guard script.
 */
export function applyBranding(
  translation: Record<string, unknown>,
  locale: string,
): BrandingResult {
  const warnings: BrandingWarning[] = [];

  const walk = (node: unknown, path: string): unknown => {
    if (typeof node === "string") {
      if (BRAND_EXEMPT_KEYS.has(path)) return node;

      const substituted = node.replace(WORD_BOUNDED, `${BRAND_TO}$1`);

      if (substituted !== node && FALSE_FRIEND_LOCALES.has(locale)) {
        warnings.push({
          locale,
          key: path,
          value: node,
          reason: `rebranded in German, where "${BRAND_FROM}" is also the everyday word for a mobile phone — confirm this names the product`,
        });
      }

      if (ANY_OCCURRENCE.test(substituted)) {
        warnings.push({
          locale,
          key: path,
          value: substituted,
          reason: `"${BRAND_FROM}" survives after substitution, glued to surrounding characters — needs a human`,
        });
      }

      return substituted;
    }

    if (Array.isArray(node)) {
      return node.map((item, index) => walk(item, `${path}[${index}]`));
    }

    if (typeof node === "object" && node !== null) {
      const out: Record<string, unknown> = {};
      for (const [key, value] of Object.entries(node)) {
        out[key] = walk(value, path ? `${path}.${key}` : key);
      }
      return out;
    }

    return node;
  };

  const rebranded = walk(translation, "") as Record<string, unknown>;

  for (const [path, value] of Object.entries(FORK_ONLY_STRINGS)) {
    setByPath(rebranded, path, value);
  }

  return { translation: rebranded, warnings };
}
