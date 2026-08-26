# Plan: three small UI fixes — duplicate Default device, streaming-only model page, AI-cleanup note

**Goal:** Fix three unrelated user-visible defects: a duplicate Default device, two contradictory model-visibility controls, and missing guidance on the AI-cleanup page.

**Architecture:** All three are frontend. Fix 1 deletes a fork-authored duplication. Fix 2 gives the streaming chip one predicate that governs both model sections and moves that predicate plus the hatch-derived default into a tiny import-free fork module with Bun tests. Fix 3 adds one fork-catalogue string and one `<p>`.

**Tech Stack:** React 18, TypeScript (strict), i18next, Zustand, Tailwind. No new dependencies.

## Global constraints

- **Land `2026-08-26-fork-only-translation-catalogues.md` first.** This plan targets its reviewed file layout and test scripts; do not implement a conditional old-layout path.
- **Locale files under `src/i18n/locales/` are not to be edited.** The new user-facing string goes in `src/shorthand/locales/en.json`.
- **Keep upstream-file edits small and local** (`AGENTS.md` § Keep the diff mergeable). Exactly one upstream file is touched: `src/components/settings/models/ModelsSettings.tsx`.
- No hardcoded strings in JSX; `eslint-plugin-i18next` enforces it on `src/**/*.{ts,tsx}`.
- Run before committing: `bun run test:unit`, `bun run lint`, `bun run format`, `bun run build`, `bun run check:branding`, `bun run check:translations`, `bun run check:fork-translations`.
- Do not edit `package.json`; the prerequisite plan already adds the test scripts this plan uses.

## Independence

| Fix | Files | Depends on |
| --- | --- | --- |
| 1 — duplicate "Default" | `SystemAudioDeviceSelector.tsx` | nothing |
| 2 — streaming-only default | `ModelsSettings.tsx`, `modelVisibility.ts`, new `streamingModelFilter.ts` and test | fork-catalogue plan for `test:unit` |
| 3 — AI-cleanup note | `AICleanupSettings.tsx`, `locales/en.json` | fork-catalogue plan |

After the prerequisite lands, all three fixes are independent and can be committed separately. None touches the same file.

---

## Task 1 — One "Default" in the system-audio device dropdown

### Root cause (confirmed)

Three layers each create a `Default` entry, and only two of them cancel out:

1. `src-tauri/src/commands/audio.rs:252-274`, `get_available_output_devices`, prepends `AudioDevice { index: "default", name: "Default", is_default: true }` to the cpal enumeration. `get_available_microphones` (:194-218) does the same for inputs.
2. `src/stores/settingsStore.ts:295-313`, `refreshOutputDevices`, **filters out** any device named `Default` or `default` and then prepends its own `DEFAULT_AUDIO_DEVICE` (`:71-75`, identical shape). This is idempotent with respect to layer 1 — it deletes what Rust injected and puts back an equivalent entry. So layers 1+2 together yield exactly one `Default`.
3. `src/components/settings/advanced/SystemAudioDeviceSelector.tsx:35-44` prepends a **third** entry on top of the already-normalised list, then spreads the list. Result: `Default`, `Default`, `<real devices>`.

Layer 3 is the whole bug. The three sibling selectors — `OutputDeviceSelector.tsx:42-45`, `MicrophoneSelector.tsx:39`, `ClamshellMicrophoneSelector.tsx:65` — all do a plain `.map()` over the store list and add nothing. `SystemAudioDeviceSelector` is the single outlier of four.

### Which layer should own it

**The store (`refreshOutputDevices` / `refreshAudioDevices`).** It already does, and it is the right owner: it is the one place that decides what "the list of devices the UI may render" contains, and its filter-then-prepend is deliberately idempotent so it does not matter whether the backend injected an entry. Components consume that list; they do not extend it. That contract is what makes a fourth selector safe to add later.

Rust layer 1 is arguably the wrong layer — a backend enumeration inventing a UI sentinel — but it is upstream's code, it is symmetric across inputs and outputs, and the store already neutralises it. Editing `commands/audio.rs` to remove it would be an upstream edit for zero user-visible gain and would break any other caller that expects the entry. **Leave it.**

### Which label the surviving option carries

**The store entry's raw device name, `"Default"`, exactly as the three sibling selectors render it.** Drop the component-local `t("settings.advanced.systemAudioDevice.default")` call.

The store owns the sentinel and every selector consumes the same `AudioDevice` list. Translating one component independently would make the same entry carry different labels on neighboring screens. If the sentinel needs a translated display name later, add one shared presentation helper for all four selectors; do not put it back into this component alone.

### Change

**File:** `src/components/settings/advanced/SystemAudioDeviceSelector.tsx` (fork-authored — `git log` shows commit `e22a920` and no `upstream/main` history for the path, so this edit has **zero** merge cost)

- [ ] Replace the `options` construction (currently lines 35-44) with a plain map, matching `OutputDeviceSelector.tsx:42-45`:

  ```tsx
  const options = outputDevices.map((device: AudioDevice) => ({
    value: device.name,
    label: device.name,
  }));
  ```

- [ ] Leave everything else untouched: `selectedDevice`, the `onSelect`, the `disabled` predicate and the `outputDevices.length === 0` placeholder all keep working. `AudioDevice` and `t` both stay in use.

### Round-trip after the fix (confirmed)

- Store list entry is `{ index: "default", name: "Default", is_default: true }`, so the option value is `"Default"`.
- `selectedDevice = getSetting("system_audio_device") || "Default"` (`:34`), and `Dropdown` matches `option.value === selectedValue` (`Dropdown.tsx:44-46`). Match holds.
- Selecting it calls `updateSetting("system_audio_device", "Default")`; the store updater at `settingsStore.ts:137-145` maps `"Default" | "default"` to `null` before invoking `setSystemAudioDevice`; `commands/audio.rs:479-480` filters case-insensitively and stores `None`. Reading back, `null || "Default"` re-selects the entry. **The round-trip is intact.**
- During the initial load `outputDevices` is `[]`, so `options` is now empty where it previously held one unusable entry — no visible change, because the same component already disables the dropdown and shows the loading placeholder on `outputDevices.length === 0` (`:60-69`).

### Verify

```
bun run lint
bun run build
bun run check:translations
bun run tauri dev     # Windows only: Settings → Audio (advanced) → System audio device
```

Manual, on Windows with **Capture system audio** on and **Mute while recording** off:

1. Open the System audio device dropdown. Exactly one `Default` row, at the top, followed by the real output devices with no repeats.
2. Select a real device, close and reopen Settings — it is still selected.
3. Select `Default`, close and reopen — still selected. Confirm `system_audio_device` is absent/`null` in the settings store file, not the string `"Default"`.
4. The Output device row under Audio still lists one `Default` (unchanged control).

---

## Task 2 — Model page defaults to streaming-only

### Root cause (confirmed)

Two mechanisms answer the same visibility question and can disagree.

`src/shorthand/modelVisibility.ts:43-57` (`isModelVisible`, applied at `ModelsSettings.tsx:61` via `useVisibleModels`) already hides non-streaming models **unless** they are downloaded, downloading, or custom whenever `show_all_settings` is false — and `show_all_settings` defaults to `false` (`src-tauri/src/settings.rs:1002`). Separately, `ModelsSettings.tsx:38,187` owns a `filterStreaming` chip which starts false and filters the already-shortened list. The bundled catalog has **67 models, 8 of which stream** (`src-tauri/src/catalog/catalog.json`), so the default *Available to Download* section is streaming-only while the chip says filtering is off.

What is **not** filtered by `modelVisibility.ts` is the *Downloaded Models* section, because downloaded/custom/in-progress models are exemptions there. The chip can filter that section, but turning it off cannot reveal non-streaming models in *Available to Download*: the earlier hatch-owned predicate already removed them. The same page therefore has two visibility decisions, and the visible chip does not control one of them.

The fix is one decision for the whole page: the chip governs both sections. `show_all_settings` determines only the chip's untouched initial state; it is no longer an independent model predicate. Turning the chip off must reveal non-streaming models in *Available to Download* as well as *Downloaded Models*.

### Answers to the four questions the brief poses

1. **Should `filterStreaming` simply default to `true`?** No. `show_all_settings` is documented to the user as *"Reveal every setting and transcription model from upstream Handy, including the ones Shorthand hides"* (`src/shorthand/locales/en.json` after the prerequisite). The untouched chip starts off when that hatch is on.
2. **Should it default to `!show_all_settings`?** Yes, but **only as the chip's default**. Once the chip state is resolved, that one boolean governs every model on the page. `ModelsSettings.tsx` must stop passing its models through the hatch-driven `useVisibleModels`; the hook itself stays intact for onboarding.
3. **Should the toggle stay visible?** Yes. It becomes the un-filter, and it is the only in-context way to see a non-streaming model you have not downloaded without leaving for the About pane. Keep it as ephemeral component state, like `searchQuery`, `filterTranslation` and `languageFilter` beside it — persisting a view filter would mean a new Rust settings field for something that is a scroll position, not a preference.
4. **Does defaulting it on risk hiding a model the user is using?** **Yes, and this is the part that must not be skipped.** Catalog entries do not get their streaming flag from the uncertain probe path: `handy-computer/parakeet-unified-en-0.6b-gguf` declares `capabilities.streaming: true` in `catalog.json`, alternate-quant `ModelInfo` entries are built by `render_model_info` / `to_model_info_for_file` from `self.caps.supports_streaming` (`managers/model.rs:249`), and local discovery explicitly skips the header probe for catalog-listed quants because the catalog is authoritative (`model.rs:1765-1768`). The genuine unprobed case is the **other**, non-catalog GGUF branch in the local HF cache (`model.rs:1787-1828`): `local_caps` reads `probe.supports_streaming.unwrap_or(false)` (`model.rs:360-364`), so an absent header key reads as non-streaming until a load reconciles it through `set_runtime_capabilities`. The filter must therefore never hide the current model. In-flight downloads and `is_custom` models retain the same protection; custom models are exempt because the app cannot know a user-supplied model's streaming capability from a catalog it is not in.

### The complete chip × hatch contract

The hatch sets the chip only while `filterStreamingOverride` is `null`. An explicit chip click wins until this page unmounts. After resolution, the hatch has no second say over model visibility:

| Chip state | `show_all_settings` | Downloaded | Available to Download |
| --- | --- | --- | --- |
| on (untouched default, or explicit) | off | Streaming models, plus current/in-progress/custom exemptions | Streaming catalog models |
| off (explicit) | off | All downloaded, in-progress, and custom models | All otherwise-eligible catalog models, including non-streaming models |
| on (explicit) | on | Streaming models, plus current/in-progress/custom exemptions | Streaming catalog models |
| off (untouched default, or explicit) | on | All downloaded, in-progress, and custom models | All otherwise-eligible catalog models, including non-streaming models |

Changing the hatch while the page remains mounted updates the chip only when the user has not touched it. This preserves the existing reactive hatch behavior without letting two predicates disagree.

### Change

**Create: `src/shorthand/streamingModelFilter.ts`** (fork-only, with no runtime imports)

- [ ] Add the two decisions the component needs as pure functions:

  ```ts
  interface StreamingFilterModel {
    id: string;
    is_downloaded: boolean;
    is_downloading: boolean;
    is_custom: boolean;
  }

  /**
   * `null` means the user has not touched the chip in this mounted page. The
   * default follows the existing show-all-settings hatch; an explicit click
   * wins until the page unmounts.
   */
  export function resolveStreamingFilter(
    override: boolean | null,
    showAllSettings: boolean,
  ): boolean {
    return override ?? !showAllSettings;
  }

  /**
   * Models the chip must not hide even when capability data says false.
   * The current model and an in-progress download must remain operable. Custom
   * models are exempt because the app cannot know a user-supplied model's
   * streaming capability from a catalog it is not in. `is_downloaded` alone is
   * deliberately not an exemption: chip off can reveal an ordinary downloaded
   * model, while exempting every downloaded model would make the chip useless
   * for the entire Downloaded Models section.
   */
  export function isStreamingFilterExempt(
    model: StreamingFilterModel,
    currentModel: string | null,
  ): boolean {
    return model.id === currentModel || model.is_downloading || model.is_custom;
  }
  ```

- [ ] Add `src/shorthand/streamingModelFilter.test.ts` with Bun cases for:
  - no override + hatch off → filter on;
  - no override + hatch on → filter off;
  - both explicit overrides win in both hatch states;
  - current, downloading, and custom models are each exempt;
  - an unrelated, complete, non-custom model is not exempt;
  - `is_downloaded: true` alone does **not** make a model exempt.

  Keep the fixture to `{ id, is_downloaded, is_downloading, is_custom }`; importing `ModelInfo`, React, Zustand, or the Tauri API would defeat the point of the small pure module.

- [ ] Update comments in `modelVisibility.ts`, with no functional change. `isModelVisible` and `useVisibleModels` stay in place for `Onboarding.tsx`, which has no streaming chip; `ModelsSettings.tsx` stops using them. Preserve the predicate's `is_downloaded || is_downloading || is_custom` exemptions so a model already on disk, in progress, or custom never disappears from onboarding's only picker. Correct its capability explanation at the same time: catalog-listed models and alternate quants use catalog caps (`model.rs:249`) and skip probing during local discovery (`:1765-1768`); only genuinely non-catalog local models can receive a false value from an absent probe field through `local_caps` (`:360-364`). Do not make this hook chip-driven and do not move the new functions into this hook-bearing module.

**File: `src/components/settings/models/ModelsSettings.tsx`** (upstream file, fork commit `875b2f3` already on top of upstream `4b3a969`; keep the edit local)

- [ ] Remove the `useVisibleModels` import. In the `useModelStore()` destructure, bind `models` directly instead of `models: allModels`, and remove `const models = useVisibleModels(allModels)`. This prevents the hatch-driven onboarding predicate from shortening the available section before the chip runs. Add `useSettings` and import `isStreamingFilterExempt` / `resolveStreamingFilter` from the new pure module.
- [ ] Replace the state at `:38`:

  ```tsx
  // null = follow the default; a boolean = the user has decided for this session.
  const [filterStreamingOverride, setFilterStreamingOverride] = useState<
    boolean | null
  >(null);
  ```

  After `useModelStore()` destructuring (so `currentModel` is in scope), derive the default from settings:

  ```tsx
  const { getSetting } = useSettings();
  const showAllSettings = getSetting("show_all_settings") ?? false;
  const filterStreaming = resolveStreamingFilter(
    filterStreamingOverride,
    showAllSettings,
  );
  ```

  **Do not** write `useState(!showAllSettings)`. `useState` reads its initializer once, while settings arrive asynchronously. The nullable override preserves the distinction between "not touched" and an explicit click.

- [ ] At the single filter predicate (`:187`), add the exemption:

  ```tsx
  if (
    filterStreaming &&
    !model.supports_streaming &&
    !isStreamingFilterExempt(model, currentModel)
  )
    return false;
  ```

  and add `currentModel` to the `useMemo` dependency array at `:196`. Because `filteredModels` now starts from the store's full `models` array and is split into sections only afterward (`:198-228`), this one predicate governs both sections. Do not add a second streaming predicate to either section.

- [ ] At the chip's `onClick` (`:289`):

  ```tsx
  onClick={() => setFilterStreamingOverride(!filterStreaming)}
  ```

  `aria-pressed={filterStreaming}` at `:292` and the conditional class at `:293-297` already read the derived value and need no change.

- [ ] Leave `settings.models.filters.streaming` ("Filter models that support live streaming transcription") alone. It describes what the chip does in both states and needs no fork override.

**Automated verify:** `bun run test:unit`. The new tests must be observed failing before the module is implemented, then passing afterward.

### What is deliberately not done

- **Not** deleting `isModelVisible` / `useVisibleModels` or making them chip-driven. They remain the hatch-driven onboarding guard, where there is no chip with which to recover a hidden on-disk, in-progress, or custom model. The model page deliberately stops calling the hook so the chip can own both sections.
- **Not** treating `is_downloaded` as a chip exemption. A blanket exemption would make every card in *Downloaded Models* immune to the chip and recreate the split decision D2 removes. Reachability remains intact: the model page begins with the full store list and chip off reveals ordinary downloaded models; the current, downloading, and custom cases remain visible while chip on; and onboarding keeps `isModelVisible`'s broader on-disk exemption.
- **Not** persisting the filter as a setting. It is a view filter; three siblings in the same toolbar are ephemeral.

### Verify

```
bun run test:unit
bun run lint
bun run build
bun run check:translations
bun run tauri dev
```

Manual, in order:

1. Fresh profile, `show_all_settings` off, Settings → Model. The streaming chip is **pressed** (highlighted). *Available to Download* lists the 8 streaming catalog entries.
2. Click the chip off. *Available to Download* now reveals the non-streaming catalog models; click it on again and they disappear. This is the primary regression guard that the chip governs that section.
3. With the hatch on, download and select a genuinely non-catalog GGUF whose probe reports no streaming capability. Return with the hatch off and the chip untouched. The chip is pressed, but the model remains in *Downloaded Models*, marked active, with working controls. **Do not use catalogued Parakeet Unified for this test; its descriptor says streaming is true.**
4. Select a streaming model instead. With the chip on, the ordinary downloaded non-streaming model from step 3 disappears; turn the chip off and it returns with working Select and Delete. This proves the chip also governs *Downloaded Models* and that `is_downloaded` alone is not an exemption.
5. Start a download of a non-streaming model with the chip off, then turn the chip on mid-download. Its card stays visible with its progress bar.
6. Add and rescan a custom model whose `supports_streaming` value is false. It stays visible in *Downloaded Models* while the chip is on and can be selected or deleted. This is the regression guard for the `is_custom` exemption the earlier proposal dropped.
7. With no chip click on a fresh mount, hatch off yields chip on and hatch on yields chip off without an app restart. In each hatch state, explicitly choose the opposite chip state and confirm both section results match the four-row table above; after that click, changing the hatch does not overwrite the chip for the rest of that mounted session.

---

## Task 3 — A second note on the AI cleanup page

### Decision: a second `<p>` with its own key, not an extension of the existing one

`AICleanupSettings.tsx:40-42` renders one note today from the fork-only key `settings.aiCleanup.sharedNote` (moved to `src/shorthand/locales/en.json` by the prerequisite).

Extend or add? **Add.** Three reasons:

1. The two say different kinds of thing. The existing note states a fact about scope — what is shared, what is per-mode. The new one is a **recommendation**. Running them together as one paragraph makes the recommendation read as a continuation of the plumbing explanation, which is where a reader stops.
2. Three sentences of 12px `text-mid-gray` above the first control is a wall at exactly the moment the user is looking for the API key field.
3. The recommendation is the sentence most likely to be reworded when Assisted Notes ships and the Modes UI becomes a **Notetaking** group. Its own key means that edit touches one string, not a paragraph shared with an unrelated statement.

The cost is one extra key and a wrapper `<div>`. If a reviewer prefers zero markup change, the fallback is to append the sentence to `sharedNote` — but take the wrapper.

### The string

```json
"settings.aiCleanup.dictationNote":
  "AI cleanup is intended for Dictation. Enabling it for notetaking is an advanced setting and is not recommended.",
```

Why this wording:

- **Three-mode safe.** It names Dictation, which is a peer in the new IA and will keep its name, and says "notetaking" — the group label Meeting and Assisted Notes will sit under. It names neither notetaking mode, so it stays true when the second one ships.
- **"notetaking", not "the notetaking modes".** There is one notetaking mode today and there will be two; the mass noun is correct in both worlds, where the plural is wrong now and the singular is wrong later.
- **Lowercase "notetaking".** The existing note capitalises "Modes" because that is a label on screen. The Notetaking group is not on screen yet, so capitalising it today would point at something the user cannot find. When the group ships, capitalising it is a one-word follow-up — record it in that plan.
- **Voice.** Two short declaratives; sentence case; states the intent then the exception, in the shape of `settings.advanced.switch.description` ("Show every setting, not just the ones most people need. Nothing moves — …"). No exclamation, no second person imperative, no "please".
- **It matches shipped behaviour.** `ModesSettings.tsx` already puts the AI-cleanup rows behind `<AdvancedOnly>` on the Meetings tab and in the default view on Dictation, with a comment saying the asymmetry is deliberate. The note tells the user what the UI is already doing.

Alternative if a reason is wanted (longer, and asserts something about the downstream enhancer that the note itself cannot verify): *"AI cleanup is intended for Dictation. Notetaking transcripts are enhanced further down the line, so cleaning them up first is an advanced setting, and not recommended."* Preferred only if the owner wants the why in the UI.

### Locale files: definitively **no**

Zero of the 24 catalogues under `src/i18n/locales/` change (the brief said 25; there are 24 directories, 23 of them non-English).

- `forkStringsFor(locale)` layers a locale's fork catalogue over `en.json`, so the new key renders in English until that locale has a fork translation.
- `bun run check:translations` compares key parity between `en` and the other 23 **files on disk**. A fork key never enters those files, so the check never sees it. Adding the key to `en/translation.json` alone would **fail** that check; adding it to all 24 would put English text in 23 translated files and dirty files that are meant to stay byte-identical to upstream.
- Confirmed by inspection: `settings.aiCleanup` does not exist in `en` or `de` on disk, yet `settings.aiCleanup.sharedNote` and `settings.aiCleanup.title` render — the mechanism works exactly as documented.

### Change

**File: `src/shorthand/locales/en.json`**

- [ ] Add `settings.aiCleanup.dictationNote` beside `settings.aiCleanup.sharedNote`. JSON cannot carry the old proposed comment; the naming rationale stays in this plan. Use the exact string above.

**File: `src/shorthand/settings/AICleanupSettings.tsx`**

- [ ] Replace the single `<p>` at `:40-42` with:

  ```tsx
  <div className="px-1 space-y-1 text-xs text-mid-gray">
    <p>{t("settings.aiCleanup.sharedNote")}</p>
    <p>{t("settings.aiCleanup.dictationNote")}</p>
  </div>
  ```

  `space-y-1` keeps the two notes reading as one block; the outer `space-y-8` still separates the block from the first `Sheet`.

- [ ] Extend the existing comment above it with one sentence on why the second note is separate — the file's comments already carry this weight and a bare second `<p>` would invite someone to merge them back.

The key is genuinely fork-only and belongs in `locales/en.json`, not `english-copy.json`. The prerequisite ordering removes the old two-layout branch.

### Verify

```
bun run lint
bun run build
bun run test:unit
bun run check:branding
bun run check:translations
bun run check:fork-translations
bun run tauri dev     # Settings → AI cleanup
```

Manual:

1. The AI cleanup page shows two short notes, tightly spaced, above the API group.
2. Switch the app language to German. Both notes render in English (the documented fallback), and no other string on the page regresses.
3. `bun run check:translations` and `bun run check:fork-translations` pass. The first proves no upstream catalogue changed; the second checks the fork catalogue layout.

---

## Concerns

**C1 — Reusing `useVisibleModels` on the model page would reintroduce the bug.** The hook remains correct for onboarding, where the hatch is the only escape route. On the model page, it would pre-filter *Available to Download* and prevent chip off from revealing non-streaming models. Keep the boundary explicit: `useVisibleModels` owns onboarding; the chip owns both model-page sections.

**C2 — `supports_streaming` can still be unknown or stale, but catalog-listed Parakeet Unified is not the unprobed case.** Its catalog descriptor says `streaming: true`; alternate quants use that descriptor (`model.rs:249`), and both local-discovery paths skip the header probe for catalog matches (`:1595-1615`, `:1765-1785`). The genuine `unwrap_or(false)` uncertainty is a non-catalog custom model or non-catalog GGUF in the local HF cache (`local_caps`, `:360-364`). Custom and current models are exempt; an unselected non-catalog HF-cache model remains recoverable by turning the chip off. Separately, a catalog flag could be stale relative to runtime behavior — for example, the catalog currently distinguishes non-streaming `parakeet-tdt-0.6b-v3` from streaming `parakeet-unified-en-0.6b`. Auditing catalog flags against post-load runtime reports is worthwhile, but it is not part of this UI fix.

**C3 — `selected_output_device` has a genuinely inconsistent sentinel. Out of scope; do not fix here.** `set_selected_output_device` (`commands/audio.rs:280`) compares `device_name == "default"` **exactly**, while `set_system_audio_device` (`:480`) uses `eq_ignore_ascii_case("default")`. The UI sends `"Default"` (capital D, the device's own name), so choosing Default for the output device persists `Some("Default")` rather than `None` — two representations of "no explicit device" for one field. It is benign today only because `audio_feedback.rs:103` special-cases the literal `"Default"` before falling back. Fixing it means editing an upstream file and, if `Some("Default")` is already on disk for existing users, thinking about what a migration does. Flagged, not scheduled.

**C4 — The Rust side injecting a UI sentinel into a device enumeration is the wrong layer**, and it is why the store has to filter before it prepends. It is upstream's code and symmetric across `get_available_microphones` / `get_available_output_devices`; the store already makes the frontend independent of it. Leave it, but note that removing it would be the right cleanup to offer upstream if a PR ever goes that way.

**C5 — Existing fork additions under `src/i18n/locales/` are outside this plan.** The reviewed fork-catalogue plan is the prerequisite and owns that migration. This plan verifies that its own diff does not touch those files.

**C6 — Fixes 1 and 3 still rely on manual component checks.** Fix 2's default and exemption rules have Bun coverage in the new pure module. Fix 1 remains Windows-only, and Fix 3 is a static two-paragraph rendering change. A Playwright Tauri stub would cover both, but building it remains separate work under `docs/FRONTEND_TESTING.md`.

**C7 — Fix 3 and the Assisted Notes plan share both wording and behavior.** The warning says AI cleanup for notetaking is Advanced. Meetings already puts it behind `<AdvancedOnly>`, and the Assisted Notes plan now does the same. If the Notetaking product label changes, update both plans before implementing either copy change.
