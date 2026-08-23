# Shorthand: brand pivot and settings redesign

Status: built
Date: 2026-08-23

Revision 1 was rewritten after a Codex review found eight blockers. Revision 2
was rewritten again after a second Codex review and, more usefully, after the
brand layer was **built and screenshotted in both themes** — which found three
things no amount of review would have: `mix-blend-mode` destroys the mark on a
dark ground, the mark degrades into a chip below a ~5:1 aspect ratio, and the
`Badge` contrast failure predicted from the maths is exactly as bad on screen as
the numbers said.

Both halves are now implemented on `brand-ux-redesign` and verified against the
rendered UI rather than only against the code. **"As built" at the end records
where the implementation diverged from what is specified below, and why** — read
it before trusting any specific claim above it.

## What this is for

Two goals, in the user's words: a brand that evokes **playful but useful
simplicity**, and a settings surface that is **not too much for most users**,
with an advanced mode for everyone else.

The emotional target is worth stating plainly, because several decisions below
only make sense against it:

- **Playful** — the app should feel like something a person made, not something
  a company shipped. A moment of surprise is allowed. Nothing bouncy, nothing
  cute, no mascot.
- **Useful** — playfulness never costs legibility, discoverability or speed. If
  a decoration and a label compete, the label wins.
- **Simple** — the default screen answers "what do I need to know" and stops.
  Simplicity here means _fewer things visible_, not _fewer things possible_.

**No setting is removed.** Regrouping is explicitly in scope; loss is not. Part 2
carries a complete destination map for every settings component in the tree, and
Testing turns that into a check that fails if any component becomes unreachable.

## Context

Shorthand is a private fork of [cjpais/Handy](https://github.com/cjpais/Handy),
repurposed for meeting transcription: it captures microphone audio, system audio
(Windows only, and only with a streaming-capable model) or both, and streams live
speaker-labelled transcripts over a local socket to follower processes. Dictation
— hold a key, speak, text lands in the focused window — is an opt-in second mode
added in
[2026-08-20-shorthand-dictation-mode-design.md](2026-08-20-shorthand-dictation-mode-design.md).

Two things are wrong with the current product surface.

**The brand says the wrong thing.** The identity in
[BRANDING.md](../../../BRANDING.md) is copying-pencil violet on pad stock, set in
IBM Plex, with the radius scale halved so corners are crisp. Every one of those
choices was made to say _clerical, archival, institutional_ — the opposite of
the emotional target above.

**There is too much settings surface.** Roughly 45 controls are reachable in the
default profile, as flat lists of toggles inside bordered cards, with most
explanations hidden in tooltips. The largest single contributor is duplication:
transcription and dictation expose nearly the same row inventory in two separate
sidebar sections.

## The constraint that shapes everything

The fork merges from upstream indefinitely, and some commits may become pull
requests back to Handy. Every line this work changes in a file upstream also
changes is a merge conflict, forever.

**Revision 1 stated this budget as "five places" while mixing files, hunks and
concepts as units.** Part 4 restates it in files, and admits the additions
honestly rather than asserting a number that was already false.

The design principle that follows from it, and that revision 1 violated four
times: _if a visual decision can only be implemented by editing inside an
upstream component, it is either the wrong decision or it needs a fork-owned
replacement named up front._

## Part 1 — The brand

### The idea: a marked-up transcript

A transcript is plain until someone marks it. Almost the entire UI is paper and
ink; colour appears only on the thing that is currently live. Playful because the
mark is a surprise against an otherwise quiet page; useful because colour is
never decorative — it always means "this one".

Chosen over **Gregg's ellipse** (shorthand is drawn from arcs because curves are
faster to write; curvature everywhere) — a stronger geometry but a weaker
discipline — and over **the stenotype keybed** (chunky keys, real press states),
which fights "simple", and whose skeuomorphism dates fast.

The idea supplies a colour rule but no geometry, type or motion. The rest of this
part derives those.

### The defect this pivot has to fix first

`--color-logo-primary` is used **96 times across 34 files**, in four mutually
incompatible roles:

| Role                            | Example                                                        | What it needs                                                                 |
| ------------------------------- | -------------------------------------------------------------- | ----------------------------------------------------------------------------- |
| Light tint behind dark text     | `Button` `primary-soft` at `/20`–`/30`, `Sidebar` at `/25`     | anything — at 20–30% over paper, every hue blends to a pale wash              |
| Solid fill under **white** text | `AccessibilityOnboarding.tsx:351,390`, `UpdateChecker.tsx:223` | ≥4.5:1 against white, so a dark value                                         |
| Foreground text and icons       | `text-logo-primary` throughout                                 | ≥4.5:1 against the background, so dark in light theme and light in dark theme |
| Focus ring                      | `Button`, `ToggleSwitch`, `ModelsSettings`                     | ≥3:1 against the background                                                   |

No single value satisfies all four, which is why the current violet already fails
two of them: white on `#b295d8` is about 2.1:1, and `text-logo-primary` on pad
stock is about 2.3:1. **This is a pre-existing defect, not one the pivot
introduces** — but a bright highlighter would take those from "poor" to
"unreadable", so the pivot has to resolve it rather than inherit it.

Revision 1's palette failed precisely here. It is also why BRANDING.md's rule
that `--color-logo-primary` "must stay a light tint in both themes" is **wrong as
written**: that is only true for the _solid_ usages. At `/20`–`/30` the value
barely matters.

### The resolution: two tokens, because there are two jobs

| Token                  | Owner                | Job                                                                                                                                                                                                    |
| ---------------------- | -------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `--color-logo-primary` | upstream (96 usages) | the app's general accent. Must satisfy all four roles above. Becomes **theme-flipping**, which `src/styles/theme.css` already supports via `--light-color-logo-primary` / `--dark-color-logo-primary`. |
| `--brand-highlighter`  | **new, fork-only**   | the sweep. Used only by fork components, always as a background behind text whose colour the fork also sets. Zero upstream usages, so zero risk.                                                       |

This is not a workaround — it is what makes the chosen idea achievable. "Colour
appears only on the live thing" cannot be delivered by substituting a token that
34 files already use for spinners, progress bars, focus rings and badges. It can
only be delivered by giving the mark its own token and quieting the general
accent.

**Said precisely, because the looser version is false:** `--color-logo-primary`
remains a general chromatic accent across upstream — recommended badges, progress
bars, hover states, links. So what this delivers is an **accent hierarchy**, not
a strict semantic colour rule:

- the **highlighter** means _live_, and marks nothing else;
- the **ink accent** means _set_ — a checked toggle, a primary button, a
  selected item. Conventional, and left conventional.

Claiming the strict rule would require touching 96 call sites, which the merge
constraint forbids and which no user would thank us for.

**Screenshotting the composed page proved why the two-tier version is the honest
one.** With the fork's layer alone the rule reads as true. On a real screen it
does not: `ToggleSwitch` fills a checked track with `--color-background-ui` at
full strength (`ToggleSwitch.tsx:47`), and two of them were louder and larger
than either mark — the eye reaches them first in the light theme.

The tempting fix is to quieten the toggle. That is the wrong one: a checked
toggle wearing the accent is how a user answers "which of these are on?" at a
glance, and greying it trades the brief's _useful_ half for its _playful_ half.

**The right fix is the one Part 2 already makes.** A screen with forty-five rows
is a field of blue switches and no mark survives it; a screen with twenty-four,
most of them not toggles, has room for one. The settings simplification is not a
separate workstream that happens to share a document — it is the thing that makes
the brand legible. If the default view ever grows back toward forty-five rows,
the mark stops working, and that is the signal to cut rows rather than to add
colour.

### Palette

| Token                             | Light     | Dark      | Named for                                       |
| --------------------------------- | --------- | --------- | ----------------------------------------------- |
| `--light/dark-color-background`   | `#faf8f2` | `#12141a` | paper                                           |
| `--light/dark-color-text`         | `#12151f` | `#eceef4` | ink                                             |
| `--light/dark-color-logo-primary` | `#12459e` | `#6aa9f5` | ink at writing strength / the same ink, diluted |
| `--color-background-ui`           | `#1e5bd6` | (same)    | ink at full strength                            |
| `--brand-highlighter` (fork-only) | `#e8f35c` | `#d9e84a` | highlighter                                     |
| `--light/dark-color-mid-gray`     | `#66697a` | `#969aab` | pencil                                          |

`--color-error` and `--color-warning` keep upstream's values and remain the only
other colours, because they are not part of the document — they are interruptions
to it.

**The hue — blue-black ink, with a yellow highlighter over it.** The page has
exactly two colours: what the words were written in, and what someone swept over
the part that mattered.

Blue was ruled _out_ in an earlier round, as the default accent of most software
written this decade, and the user has since chosen it directly. That decision
stands, and it is better than the ruling-out allowed for, because the objection
was to blue _alone_. A lone blue accent is generic. Blue ink under a yellow
highlighter is not a palette choice at all — it is a description of a marked-up
page, and it is the reason highlighters are yellow and ink is blue in the first
place: they sit opposite each other on the wheel, so the sweep pops against the
ink instead of competing with it. The pairing is what stops it reading as
another SaaS blue.

"Vibrant, not too bright" also lands inside a window the contrast maths dictates
independently — see below — so the brief and the constraint point at the same
colour rather than fighting.

Still ruled out for the accent: green (reads success), amber and orange (collide
with `--color-warning`), pink (upstream Handy's), violet (the direction being
replaced).

**Measured contrast.** Every ratio below was computed, not estimated, and is
asserted by test 6.

| Pair                                                 | Ratio | Need |      |
| ---------------------------------------------------- | ----- | ---- | ---- |
| `#12151f` on `#faf8f2` — body text, light            | 17.15 | 4.5  | pass |
| `#eceef4` on `#12141a` — body text, dark             | 15.87 | 4.5  | pass |
| `#66697a` on `#faf8f2` — secondary text, light       | 5.11  | 4.5  | pass |
| `#969aab` on `#12141a` — secondary text, dark        | 6.58  | 4.5  | pass |
| `#12459e` on `#faf8f2` — `text-logo-primary`, light  | 8.34  | 4.5  | pass |
| `#6aa9f5` on `#12141a` — `text-logo-primary`, dark   | 7.54  | 4.5  | pass |
| white on `#1e5bd6` — primary button label            | 5.96  | 4.5  | pass |
| `#1e5bd6` vs `#faf8f2` — button fill vs light ground | 5.61  | 3    | pass |
| `#1e5bd6` vs `#12141a` — button fill vs dark ground  | 3.09  | 3    | pass |
| `#faf8f2` on `#12459e` — Badge `primary`, light      | 8.34  | 4.5  | pass |
| `#12141a` on `#6aa9f5` — Badge `primary`, dark       | 7.54  | 4.5  | pass |
| `#12151f` on `#e8f35c` — ink on the sweep, light     | 15.10 | 4.5  | pass |
| `#12141a` on `#d9e84a` — ink on the sweep, dark      | 13.67 | 4.5  | pass |
| `#12459e` hairline vs `#faf8f2` — sweep edge, light  | 8.34  | 3    | pass |
| `#6aa9f5` hairline vs `#12141a` — sweep edge, dark   | 7.54  | 3    | pass |
| `#12459e` focus ring on `#faf8f2`                    | 8.34  | 3    | pass |
| `#6aa9f5` focus ring on `#12141a`                    | 7.54  | 3    | pass |

Nothing fails. The thinnest margin is the primary fill against the dark ground at
3.09:1, and that number is not a coincidence — it is the binding constraint that
picked the blue.

**Why the primary blue is mid, not deep.** `--color-background-ui` has to carry
white text _and_ separate from both grounds as a non-text fill. Those pull
opposite ways and leave a narrow luminance window: at least 0.119 to clear 3:1
against the dark ground, at most 0.183 to keep white at 4.5:1. A deep navy cannot
reach it at all — sapphire `#0f52ba` has luminance 0.097, so it can never hit 3:1
against _any_ dark background, however dark. Every hand-picked ink blue —
registrar `#12459e`, quink `#1749c0`, cobalt `#2050c8` — fails the same way, at
2.1–2.7:1.

`#1e5bd6` sits inside the window. So "vibrant blue, not too bright" was not a
preference the maths had to be bent around; it is the only band that works.

Getting here took several corrections, each worth recording because each was a
real defect:

- **The primary fill was twice wrong before it was right.** Revision 2's first
  olive gave 2.94:1 against the dark ground; the first blues tried were deep
  navies that could not reach 3:1 at any ground. Today's `#6e3d9b` manages 2.94:1
  on today's dark background, so the shipped app has this failure now — the
  window is not a new tax, it is an existing bug finally being measured.
- **Three upstream sites put white on `bg-logo-primary`**
  (`AccessibilityOnboarding.tsx:351,390`, `UpdateChecker.tsx:223`). They already
  fail today at 2.1:1. The fix is not a text swap but a **semantic** one: those
  are primary actions, so they become `bg-background-ui`, which carries white at
  5.30:1 in both themes. One word each, in two upstream files.
- **`Badge`'s `primary` variant sets a background and no text colour**
  (`Badge.tsx:15`), so it inherits ink. Against the new dark ink-blue accent that is
  2.47:1. It needs `text-background` added — a one-line upstream edit, taken
  deliberately, because making the accent dark fixes `text-logo-primary` across
  dozens of sites and breaks exactly this one.

**Secondary text has to become theme-dependent, and that costs a promise.**
`--color-mid-gray` is a single value in upstream's `styles/theme.css:28`, unlike
`--color-logo-primary`, which upstream already resolves from a
`--light-` / `--dark-` pair. A single grey **cannot** work here: passing 4.5:1
against `#faf8f2` requires a luminance at or below 0.17, and passing it against
`#141210` requires 0.203 or above. There is no overlap — an exhaustive search of
all 256 greys returns none, with the best worst-case being `#777777` at 4.17:1.
Today's values fail for the same reason: upstream's `#808080` is 3.43:1 on pad
stock, and the fork's current `#7a757f` is 3.90:1 in both themes.

So `brand/theme.css` gains its own `@media (prefers-color-scheme: dark)` and
`:root[data-theme="…"]` blocks, mirroring the structure upstream already uses a
few lines further down its own file.

That means **amending the promise in that file's header**, which currently claims
it "introduces no new selector". The accurate version: it introduces no new
_utility or component_ selector, and mirrors upstream's own theme-selection
selectors solely to set values. The reason the promise existed — that a restyle
upstream should merge without conflict — is untouched, because every line is
still in a fork-only file. Saying so in the header matters more than preserving
the sentence.

**The sweep and WCAG's luminance-only model.** `#e8f35c` against `#faf8f2` is
1.14:1. A saturated yellow on white is genuinely visible — hue and chroma carry
it — but WCAG 2.x measures luminance alone and cannot see that, and a rule the
design cannot satisfy is a rule the design should stop leaning on. So the sweep
gains **a hairline of the accent along its lower edge** — `#12459e` on light at
8.34:1, `#6aa9f5` on dark at 7.54:1 — giving a compliant boundary.

This is the constraint improving the design rather than taxing it: a highlighter
stroke with a pen line under it is what a marked-up page actually looks like.

In the dark theme the sweep carries `--color-background` as its text colour
rather than `--color-text`, since a bright highlighter needs dark ink on it in
both themes. That rule lives in the fork's own CSS and costs nothing upstream.

### Geometry: containers are paper, marks are hand-made

- **Kill the card** — in fork-owned sections. The bordered, rounded box around
  every settings group goes; a group becomes a heading plus rows separated by
  hairlines and whitespace, the way paragraphs sit on a page. This removes
  roughly forty borders and is the single largest reduction in the redesign.
  History is the documented exception: it hand-rolls its own card inside
  `HistorySettings.tsx:279`, and replacing it means owning a fork copy of a large
  upstream component. It keeps its card for now.
- **Marks are the only rounded things** — in fork-owned components. Tabs, the
  active sidebar row and badges the fork draws. Revision 1 also promised fully
  rounded shortcut chips; their radius is hardcoded inside
  `GlobalShortcutInput.tsx:282` and `HandyKeysShortcutInput.tsx:330` with no
  styling hook, so **that promise is withdrawn** rather than paid for with an
  upstream edit.
- **The radius scale splits.** `--radius-*` returns near upstream's values for
  genuine containers; fork marks use full rounding. The halved scale belonged to
  the crisp-ledger direction and goes with it.
- **The ruled sidebar margin goes.** It was the previous direction's flourish,
  and it is chrome.

### The sweep

One motif, one meaning: _this is the current thing_.

**Where it goes was settled by building it and screenshotting both themes, not
by argument.** Three findings changed the design, and all three are recorded in
`marks.css` next to the code they explain:

- **It marks text, never a container.** On a 40px sidebar row the rotation is
  imperceptible, the gradient unevenness invisible, and the asymmetric radii
  read as plain "rounded" — the exact rectangular active-state fill the
  direction exists to avoid, with yellow substituted for blue.
- **The text has to be long enough.** Moving the mark to the label only moved
  the failure one scale down. What governs is the rendered aspect ratio: running
  text at 10.4:1 works, a tab label at 5.2:1 works, `AI cleanup` at 3.0:1 is
  weak, `Modes` at 1.9:1 is a chip, `App` at 1.2:1 is a square. Below about 5:1
  the corner radii eat the whole perimeter, no straight section survives, and
  the pen line detaches along the entire bottom edge — two objects, a badge with
  an underline, not one stroke.
- **So the sidebar does not get the sweep.** Its selection is an accent icon
  plus a full-weight label against dimmed neighbours. That is quieter, which
  suits a rail that is permanently on screen, and it protects the rule the whole
  direction rests on: colour means _live_, not merely _selected_.

The sweep therefore marks **running text and the active tab**. Recording state
is out of scope with the rest of the overlay, but `marks.css` is imported into
`RecordingOverlay.css` as well as `App.css` so the overlay can adopt it without
a second import later. Revision 2 promised the sweep on recording state while
placing the overlay out of scope and importing the file only into `App.css`;
that contradiction is resolved by doing the import and deferring the usage.

**`mix-blend-mode: multiply` was removed.** A highlighter is physically
translucent, so multiply looked right — and on the dark theme it is right and
fatal: a highlighter over black paper deposits nothing. `#d9e84a` multiplied by
`#12141a` resolves to about `#0f1208`, which put the dark label ink at roughly
1.05:1 against its own mark. Every dark-theme mark rendered as a black smudge
with only the pen line surviving. In the light theme it bought nothing visible.
The translucency that actually reads is at the two ends, and the gradient's
alpha supplies that in both themes without a blend mode.

Revision 1 also claimed it for the selected model card. Active-model styling is
internal to upstream's `ModelCard.tsx:117` with no indicator hook, so **that
claim is withdrawn**; the model list keeps upstream's own active treatment.

Drawn as a translucent block with round caps and a slightly uneven top and bottom
edge — a `mask-image` with a subtle wobble, not a flat rectangle — with a hairline
of the accent along its lower edge. A perfectly rectangular flat fill is what
every UI framework already does and would throw the idea away.

### Motion

One primitive: a sweep is _drawn_, left to right, ~180ms, ease-out, on tab change,
section change and recording start. **Wrapped in
`@media (prefers-reduced-motion: no-preference)`**, with the reduced case showing
the sweep already drawn. No such query exists anywhere in the frontend today;
this adds the first one.

Within the settings window nothing else animates beyond `transition-colors`. The
recording overlay and the model cards keep their existing animations — both are
out of scope, and revision 1's blanket "nothing else animates" was simply untrue
of them.

### Type

**Atkinson Hyperlegible Next** for the UI, **Atkinson Hyperlegible Mono** for
paths, shortcuts and logs. Drawn by the Braille Institute so characters cannot be
confused with one another; its whole thesis is legibility of the written record,
which is the thing this app produces. It carries real warmth and quirk — the tail
on the `l`, the open `g` — without being a novelty face, which matters because a
UI that is 90% achromatic cannot carry personality in colour.

Both ship via `@fontsource`, so self-hosting is unchanged: the app is offline-first
and must never fetch a webfont at runtime.

Ruled out: Inter and Geist (the decade's default), Nunito and Quicksand (rounded
and friendly is generic playful, and generic is the thing being avoided),
Fraunces (a serif at 13px row labels is a legibility bet a transcription app
should not take).

### What is kept

The pointed-pen `s` mark and the wordmark lockup are unchanged; they already say
the product's idea and are not the part that reads as serious. The mark fills with
`currentColor`, so only its colour changes, and `scripts/gen-brand-mark.ts` never
re-runs.

## Part 2 — Settings information architecture

### The rule that decides where a setting goes

Revision 1's rule ("iff it has a `DictationSettings` counterpart") could not
generate its own table, because shortcuts live in the shared `AppSettings.bindings`
map, not on `DictationSettings`. Corrected:

> A row is **per-mode** if it has a `DictationSettings` counterpart **or** a
> mode-specific binding id. Everything else is **shared** and appears exactly once.
>
> Three things sit outside the rule, and are listed here rather than bent to
> fit, because a rule with unstated exceptions is not a rule:
>
> 1. **`dictation.enabled`** — a meta-control, not a setting. It has no
>    counterpart because meeting mode cannot be switched off. It renders as the
>    Dictation tab's first row.
> 2. **`AccessibilityPermissions`** — an OS permission prompt, not a persisted
>    setting at all. It renders in the Dictation tab when dictation is on,
>    because that is the mode that needs the permission.
> 3. **`external_script_path`** — bound inside `PasteMethod` and revealed only
>    by the Linux-only "external script" method. It has no dictation
>    counterpart, so by the rule it is shared; in practice it travels with
>    whichever paste-method row is on screen. Named explicitly so it cannot be
>    lost when `PasteMethod` is wrapped.

`DictationSettings` has thirteen fields. Twelve are per-mode overrides of an
`AppSettings` counterpart; `enabled` is the exception, with no counterpart,
because meeting mode cannot be switched off. Mode-specific bindings are
`transcribe` / `transcribe_with_post_process` against `dictate` /
`dictate_with_post_process`. `cancel` is a single shared binding.

Two consequences revision 1 got wrong:

- **`cancel`** is shared, so it renders once, in a "Shared by both modes" group
  below the tabs — not duplicated into each tab.
- **`overlay_style`** is per-mode but **`overlay_position`** is a shared
  `AppSettings` field. The style rows go in the tabs; the single position control
  moves to App.

### Sections

**Modes · Audio · Model · AI cleanup · App · History · About**, plus Debug when
`debug_mode` is on. `AI cleanup` keeps today's predicate — visible when either
mode has post-processing enabled.

### Advanced: disclosure in place

The Advanced switch moves out of About into the sidebar footer, visible from every
section, persisting to the existing `show_all_settings` field. **No Rust change.**

Its meaning changes: today it swaps the fork's simplified sections for upstream's
full ones — two trees sharing no vocabulary, so the hatch feels like a different
app. It will instead reveal additional rows and groups in place. Same sections,
same order, more of the page you were already on.

### Complete destination map

Every settings component in `src/components/settings/**` and `src/shorthand/**`,
and where it lands. `D` = visible by default, `A` = revealed by Advanced,
`Dbg` = Debug section. Nothing is dropped.

**Modes — per tab**

| Transcription                                  | Dictation                                      |                 |
| ---------------------------------------------- | ---------------------------------------------- | --------------- |
| —                                              | `DictationEnableToggle`                        | D               |
| `ShortcutInput` `transcribe`                   | `ShortcutInput` `dictate`                      | D               |
| `PushToTalk`                                   | `DictationToggleField` `push_to_talk`          | D               |
| `ShowOverlay` (style)                          | `DictationShowOverlay` (style)                 | D               |
| `PostProcessingToggle`                         | `DictationToggleField` `post_process_enabled`  | D               |
| prompt selector                                | `DictationPostProcessPrompt`                   | D               |
| `ShortcutInput` `transcribe_with_post_process` | `ShortcutInput` `dictate_with_post_process`    | D               |
| `SaveRecordings`                               | `DictationToggleField` `save_recordings`       | D               |
| `SaveTranscripts`                              | `DictationToggleField` `save_transcripts`      | D               |
| —                                              | `AccessibilityPermissions`                     | D, when enabled |
| `PasteMethod`                                  | `DictationPasteMethod`                         | A / **D**       |
| `TypingTool`                                   | `DictationTypingTool`                          | A               |
| `ClipboardHandling`                            | `DictationClipboardHandling`                   | A               |
| `AutoSubmit`                                   | `DictationAutoSubmit`                          | A               |
| `AppendTrailingSpace`                          | `DictationToggleField` `append_trailing_space` | A               |

Paste method is default on Dictation and advanced on Transcription: a field is
shown by default in the tab where it is load-bearing. Revision 1 justified this by
claiming the fork _forces_ `PasteMethod::None` in transcription mode — it does
not. `None` is the **default**, and legacy profiles are migrated to it, but a
user-chosen method stays effective (`shorthand/dictation.rs:80`). The placement
stands; the reason is corrected.

**Modes — shared by both modes:** `ShortcutInput` `cancel` (A), rendered once
below the tabs.

Its two existing visibility predicates must survive the move: Cancel is hidden
when push-to-talk is on (releasing the key already cancels) and on Linux
(dynamic-shortcut instability). Both live in `GeneralSettings.tsx:19-29` and
`CaptureSettings.tsx:26-38` today. Because push-to-talk is now per-mode, the row
is shown only when _neither_ mode has push-to-talk enabled — the strictly safer
reading, since a visible-but-useless shortcut is better than a hidden one that
still fires.

**Audio:** `MicrophoneSelector` D · `SystemAudioCapture` D · `SystemAudioDeviceSelector` D ·
`ChannelSelector` A · `MuteWhileRecording` A · `VoiceActivityDetection` A ·
`AlwaysOnMicrophone` A · `ClamshellMicrophoneSelector` A · `RecordingBuffer` Dbg.

Both system-audio rows self-hide outside Windows, so the default control count is
platform-dependent. Only the capture toggle also checks model capability
(`SystemAudioCapture.tsx:26-46`); the device selector checks stored enablement
and mute state but never the model (`SystemAudioDeviceSelector.tsx:32-69`). That
asymmetry is upstream's and is unchanged here.

**Model:** `ModelsSettings` D · `ModelSettingsCard` (wrapping `LanguageSelector`,
`TranslateToEnglish`) D · `CustomWords` D · `FillerWordRemoval` D ·
`ModelUnloadTimeout` A · `AccelerationSelector` A · `WordCorrectionThreshold` Dbg.

**AI cleanup:** `ProviderSelect` D · `ApiKeyField` D · `BaseUrlField` D (revealed
by the custom provider, as today) · `ModelSelect` D ·
`PostProcessingSettingsPrompts` D.

**App:** `ThemeSelector` D · `AppLanguageSelector` D · `AutostartToggle` D ·
`AudioFeedback` D · `StartHidden` A · `ShowTrayIcon` A · `OutputDeviceSelector` A ·
`VolumeSlider` A · `SoundPicker` A · `ShowOverlay` (position) A ·
`UpdateChecksToggle` A · `ShowWhatsNewOnUpdate` A · `FollowStreamOutput` A ·
`ExperimentalToggle` A · `LazyStreamClose` A · `KeyboardImplementationSelector` A ·
`PasteDelay` Dbg · `ReliablePaste` Dbg.

**History:** the entry list and `OpenRecordingsButton` D · `HistoryLimit` A ·
`RecordingRetentionPeriod` A.

**About:** version and links (Donate, Source, Acknowledgments) D ·
`AppDataDirectory` A · `LogDirectory` A · `WhatsNewPreview` Dbg. The
`ShowAllSettingsToggle` leaves About for the sidebar footer.

**Debug** keeps `KeyboardDiagnostic`, `LiveLogViewer`, `LogLevelSelector` and the
rows marked `Dbg` above.

`DebugPaths.tsx` is **dead code**: it is defined but rendered nowhere, and is not
exported from `debug/index.ts`. It is therefore not a setting to preserve, and
the exhaustiveness test needs an explicit allow-list entry for it rather than a
destination. It is left in place rather than deleted, for the same
delete/modify-conflict reason as upstream's unregistered screens.

Four controls that are debug-gated today are **promoted** into Advanced in their
topical section, because they are user-facing preferences rather than
diagnostics: `AlwaysOnMicrophone`, `ClamshellMicrophoneSelector`, `SoundPicker`,
`UpdateChecksToggle`. Two settings revision 1 lost entirely —
`KeyboardImplementationSelector` and `LazyStreamClose` — are placed in App /
Advanced.

Default row count is approximately 24, against roughly 45 today.

### The consequence, stated plainly

Upstream's General, Advanced, Models and Post-processing screens are never
rendered; the fork owns settings presentation. Their files stay in the tree,
untouched and unregistered — deleting a file upstream still maintains turns every
future edit into a delete/modify conflict, the expensive kind.

That makes exhaustiveness a hard requirement, enforced by test.

### Copy

- Group headings become sentences in sentence case, not uppercase micro-labels.
- Default rows render descriptions inline. Hiding a setting's explanation behind
  a tooltip is the opposite of simple. Advanced rows may keep tooltips.
  `SettingContainer` supports `descriptionMode="inline"`, but `VolumeSlider`
  hardcodes `"tooltip"` and `SoundPicker` exposes no such prop at all — both are
  Advanced-only rows, so **they keep tooltips** rather than earning an upstream
  edit.
- Labels are rewritten in plain language where the current one names an
  implementation ("Paste method" becomes "How text arrives").

Fork-only strings go in `FORK_ONLY_STRINGS` in `src/shorthand/branding.ts`.
Revision 1 claimed `src/i18n/locales/*/translation.json` is byte-identical to
upstream; it is not — all 24 locale files carry fork copy. The accurate policy:
**new fork-only UI strings go in `FORK_ONLY_STRINGS`, English only**, and the
Handy → Shorthand rename happens at build time in
`src/shorthand/vite-branding-plugin.ts`. Existing locale edits stay as they are.

## Part 3 — The Modes pane

Sidebar label **Modes**; page heading a sentence; tab bar directly beneath.

```
Modes
How each mode behaves

  ┌──────────────────┐
  │ Transcription    │  Dictation      ← the active tab carries the sweep
  └──────────────────┘
  … per-tab rows (Part 2) …

  Shared by both modes
  Cancel shortcut
```

Both tabs are always visible; the Dictation tab leads with its own enable toggle
rather than hiding, so the feature is discoverable instead of invisible.

Two behaviours carry over from the dictation spec and must not regress:

- Dictate shortcut rows are **hidden**, not disabled, while dictation is off.
  `SettingContainer`'s `disabled` prop only fades the label — it never reaches the
  key-recorder chip or the Reset button, so a disabled row would still register a
  live global shortcut.
- `AccessibilityPermissions` has no disabled state, only self-hide, so it is not
  rendered at all while dictation is off.

Nothing about any setting's value, default, storage or independence changes. This
part is entirely presentation.

### Accessibility

Non-negotiable, and treated as acceptance criteria rather than aspiration.

- `Tabs.tsx` implements the WAI-ARIA tabs pattern: `role="tablist"` /
  `role="tab"` / `role="tabpanel"`, `aria-selected`, `aria-controls`, roving
  `tabIndex`, and Left/Right/Home/End key handling.
- Sidebar rows today are bare clickable `<div>`s with no role, `tabIndex`,
  keyboard handler or `aria-current` (`Sidebar.tsx:112`). They gain
  `role="tab"`-equivalent semantics, keyboard operation and `aria-current="page"`.
  This is an edit inside an upstream file already in the budget.
- The sweep is never the only signal. Active state is also carried by
  `aria-current` / `aria-selected` and by font weight, so it survives greyscale
  and screen readers.
- All motion is behind `prefers-reduced-motion`.

## Part 4 — Files and merge budget

### Upstream files touched

Stated in **files**, and larger than revision 1 claimed.

Already in the budget:

1. `src/App.css` — the existing `@import`, plus one line for `marks.css`
2. `src/overlay/RecordingOverlay.css` — the existing `@import`
3. `src/overlay/RecordingOverlay.css` — the existing `--s-font` indirection
4. `src/components/Sidebar.tsx` — wordmark and section list; now also the footer
   Advanced switch and the row accessibility semantics
5. `src/components/onboarding/{Onboarding,AccessibilityOnboarding}.tsx` — the
   existing wordmark

Added by this work, and not previously admitted:

6. `src/components/onboarding/AccessibilityOnboarding.tsx` — two
   `bg-logo-primary` → `bg-background-ui` swaps (same file as 5)
7. `src/components/update-checker/UpdateChecker.tsx` — the same swap, once
8. `src/components/ui/Badge.tsx` — one line: `primary` gains `text-background`
9. `package.json` and `bun.lock` — swap two `@fontsource` dependencies for two
   others, and add `@axe-core/playwright` for test 7. Dependency manifests, where
   a conflict is a one-line re-add rather than a semantic merge.

Items 6–8 are each one word or one line, and all three fix contrast failures that
exist today. They are the price of making the accent dark enough to be readable
as foreground text, which is the trade Part 1 argues for.

Generated binary assets also change: `src-tauri/app-icon.png`, eleven tray PNGs
under `src-tauri/resources/`, and the whole `src-tauri/icons/` tree via
`tauri icon`. These are **artifacts, not source** — a conflict is resolved by
re-running the generators, never by merging.

They cannot be regenerated without also editing their generator:
`scripts/gen-brand-icons.mjs:35-43,151-161` hardcodes the old violet `PAPER`,
`INK` and `VIOLET` constants and a violet icon gradient. That file is fork-owned,
so it is not upstream conflict surface, but it is a source change and belongs on
the list. `scripts/gen-brand-mark.ts` is untouched — the mark's geometry does not
change.

**On counting.** The list above is nine numbered entries covering eight distinct
files plus two manifests; entries 2 and 3 are two hunks in one file, and entry 5
is a two-file glob. The honest unit is: **seven upstream source files, three
manifests, one fork-owned generator, and one tree of generated binaries.** The
number is less useful than the shape — what matters is that every entry is a
one-word or one-line change except `Sidebar.tsx`, which the fork already owns
the top of.

`src/components/ui/SettingsGroup.tsx` is **not** edited: killing the card uses a
fork-owned replacement in fork sections only, so upstream's screens keep their own
component and a restyle upstream still merges cleanly.

### New fork-only files

| File                                           | What it is                                                                                                                                                                                           |
| ---------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `src/shorthand/ui/Sheet.tsx`                   | the borderless paper group replacing `SettingsGroup` in fork sections                                                                                                                                |
| `src/shorthand/ui/Tabs.tsx`                    | the tab bar, full ARIA tabs pattern, sweep as the active indicator                                                                                                                                   |
| `src/shorthand/ui/AdvancedOnly.tsx`            | renders children only when `show_all_settings` is on                                                                                                                                                 |
| `src/shorthand/useAdvanced.ts`                 | reads and writes `show_all_settings`                                                                                                                                                                 |
| `src/shorthand/settings/ModesSettings.tsx`     | the tabbed pane                                                                                                                                                                                      |
| `src/shorthand/settings/AudioSettings.tsx`     | replaces `CaptureSettings`                                                                                                                                                                           |
| `src/shorthand/settings/ModelSettings.tsx`     | replaces `TranscriptionSettings`                                                                                                                                                                     |
| `src/shorthand/settings/AICleanupSettings.tsx` | fork-owned; provider, key, base URL, model, prompt library                                                                                                                                           |
| `src/shorthand/settings/AppSettings.tsx`       | replaces today's `AppSettings`                                                                                                                                                                       |
| `src/shorthand/settings/AboutSettings.tsx`     | fork-owned About, so it can be borderless                                                                                                                                                            |
| `src/shorthand/brand/marks.css`                | the sweep, its animation, and the reduced-motion rule                                                                                                                                                |
| `src/shorthand/ui/OverlayStyleRow.tsx`         | renders `overlay_style` alone. Upstream's `ShowOverlay` always renders style **and** the shared `overlay_position` together with no prop to split them, and the map needs them in different sections |
| `src/shorthand/ui/OverlayPositionRow.tsx`      | the shared `overlay_position`, for App                                                                                                                                                               |
| `src/shorthand/settings/HistorySettings.tsx`   | fork-owned History. Upstream's renders only the heading, open-folder button and entry list — it has no home for `HistoryLimit` or retention, which the map puts there                                |
| `tests/settings-coverage.spec.ts`              | the exhaustiveness check (Playwright — the repo has no React unit harness)                                                                                                                           |
| `tests/modes-tabs.spec.ts`                     | the per-mode isolation and tab behaviour checks                                                                                                                                                      |

`src/shorthand/brand/theme.css` changes values, adds one fork-only custom
property (`--brand-highlighter`), and — as Part 1 explains — adds theme-selection
blocks mirroring upstream's, because secondary text provably cannot be one value.
Its header comment is amended to match. It still introduces no utility or
component selector; the sweep lives in `marks.css` precisely to keep it that way.

`src/shorthand/visibility.ts` loses `SIMPLIFIED_MODE_HIDDEN_SECTIONS` and
`FORK_ONLY_SECTIONS` — there is no two-tree swap left to encode.

### Why the token overrides work at all

Tailwind v4 emits its theme variables inside `@layer theme`, and unlayered
declarations outrank any layer. A plain `:root { … }` in `brand/theme.css`
therefore beats Tailwind's own value without an `@theme` block — which matters
because the overlay window does not import Tailwind and gets the same file.

## Testing

Conventions are in [docs/FRONTEND_TESTING.md](../../FRONTEND_TESTING.md). The
repo has no React unit-test harness, so these are Playwright specs.

1. **Exhaustiveness.** Enumerate every `.tsx` under `src/components/settings/**`
   and `src/shorthand/**` **from the filesystem** — the whole of `src/shorthand`,
   not just `settings/` and `dictation/`, since `AppSettings.tsx`,
   `CaptureSettings.tsx`, `DictationSettings.tsx`, `TranscriptionSettings.tsx`
   and `ShowAllSettingsToggle.tsx` sit at its root — and assert each renders
   somewhere in the fork tree with Advanced on (Debug included).

   The test enumerates **leaf controls**, and holds a declared allow-list for
   the files that are not leaves. Without that split it contradicts itself: it
   cannot demand that every file render while the spec also requires
   `GeneralSettings`, `AdvancedSettings`, `PostProcessingSettings`,
   `CaptureSettings` and `TranscriptionSettings` be unregistered. The allow-list
   has three categories, each entry needing a comment: composite screens the
   fork replaces, pure re-exports (`PostProcessingSettingsPrompts.tsx`), and
   dead code (`DebugPaths.tsx`).
   Revision 1 based this on the two `index.ts` files, which omit at least eleven
   components — `MuteWhileRecording`, `PasteMethodSetting`, `TypingToolSetting`,
   `ClipboardHandlingSetting`, `AutoSubmit`, `ShowTrayIcon`, `ThemeSelector`,
   `VolumeSlider`, `ExperimentalToggle`, the acceleration and system-audio
   controls — so passing it proved nothing. This is the test that makes it safe to
   stop rendering upstream's screens. The allow-list of components that
   deliberately render nowhere holds exactly one entry today (`DebugPaths.tsx`);
   adding to it requires a comment saying why.

2. **The per-mode rule.** Every `DictationSettings` field except `enabled` appears
   in both tabs; no field without a counterpart or a mode-specific binding does.
   Asserted against the generated `src/bindings.ts` so it fails when the Rust
   struct gains a field.
3. **Disclosure.** Advanced off renders the default inventory; toggling it reveals
   the advanced rows without changing the visible section list.
4. **Mode isolation.** Editing a Dictation row writes `settings.dictation.*` and
   leaves the `AppSettings` counterpart untouched, and the reverse.
5. **Shortcut safety.** With dictation off, no dictate shortcut row is mounted.
6. **Contrast.** Every pair in the Part 1 table is asserted against its stated
   threshold, computed from the tokens as rendered rather than from hardcoded
   hexes, so a future palette change fails the test instead of failing silently.
7. **Accessibility.** Tabs expose the ARIA tabs pattern and respond to arrow keys;
   sidebar rows expose `aria-current`; an axe scan of each section is clean. The
   repo has `@playwright/test` but no scanner, so this adds `@axe-core/playwright`
   — a third manifest change beyond the two font swaps, counted in Part 4.
8. **Rendered contrast, not just token pairs.** Test 6 checks the tokens; it
   cannot see opacity utilities, which is where the remaining failures are.
   `text-logo-primary/70` is about 3.49:1 on light paper and is used for a 10px
   label (`ModelDropdown.tsx:59`); `text-mid-gray/70` is about 2.95:1 light and
   3.58:1 dark, and carries visible prompt guidance
   (`PostProcessingSettings.tsx:318-323,385-390`). The test samples computed
   colours from the rendered page so composited alpha is measured as the user
   sees it. Both are pre-existing failures; both are in scope because the spec
   claims the settings surface is accessible.
9. **Reduced motion.** With `prefers-reduced-motion: reduce`, the sweep renders
   fully drawn and no animation runs.

Rust is not touched, so `cargo test` is unaffected.

## Out of scope

- The recording overlay's layout, motion and compact-pill/Live-panel behaviour.
  It inherits the palette and typeface; its geometry is separate work, and it is
  the surface users see most.
- `HistorySettings`' hand-rolled card, and the fork copy that removing it would
  require.
- `ui/Button.tsx`'s `danger` variant, still a hardcoded `bg-red-600` ignoring
  `--color-error`.
- Onboarding beyond inheriting the palette and typeface.
- The pre-existing CRLF/LF mismatch that makes `bun run format:check` fail on 86
  files at HEAD. Reformatting would be an enormous conflict surface for no gain.

## As built

Where the implementation diverged from the design above. Every entry was forced
by rendering the thing and looking at it — none of it survived contact with a
screenshot, and none of it was visible to two rounds of Codex review.

### The sweep is narrower than specified

Part 1 gave the sweep to the active sidebar row, the active tab and running
text. It now marks **running text and the active tab only**.

Below roughly a 5:1 aspect ratio the corner radii consume the whole perimeter,
no straight section survives, and the pen line detaches along the entire bottom
edge — the mark reads as a badge with an underline rather than a stroke.
Measured on the rendered boxes: running text 10.4:1 works, a tab label 5.2:1
works, `AI cleanup` 3.0:1 is weak, `Modes` 1.9:1 is a chip, `App` 1.2:1 is a
square. The sidebar marks its selection with an accent icon and a full-weight
label against dimmed neighbours instead.

`mix-blend-mode: multiply` was specified and then removed. A highlighter is
translucent, so it looked right — and on a dark ground it is physically right
and fatal, because a highlighter over black paper deposits nothing. Every dark
mark rendered as a black smudge at about 1.05:1.

### Four rows the rendered UI exposed

None of these are visible in source review:

- The recording-overlay row's description runs to six lines and was the loudest
  thing in the default view, for a secondary setting. It takes a tooltip. The
  "descriptions inline by default" rule holds until one description outweighs
  every control around it.
- The dedicated AI-cleanup hotkey rendered under an AI-cleanup toggle that was
  off. Hidden now, not disabled — a shortcut row is never inert.
- "Shared by both modes" rendered as a heading with nothing beneath it, because
  its only row has three independent reasons to be absent. The heading moved
  inside the guard. Consequence worth knowing: push-to-talk is on by default in
  both modes, so on a fresh install that group renders nowhere and the cancel
  shortcut is reachable only after turning push-to-talk off _and_ finding the
  Advanced switch.
- Fixes applied to one tab and not the other are worse than the original defect,
  because the disagreement is visible in a single flip.

### The advanced switch did nothing observable

At the window size `lib.rs` actually ships — 680x570 — the content pane is 532px,
the default Modes section is 539px, and the first row the switch reveals starts
at y=539. Seven pixels below the fold. Clicking it changed nothing on screen
except a 12px dot in the sidebar footer.

It now scrolls the first revealed row into view, behind `prefers-reduced-motion`.
It stays in the sidebar footer rather than moving beside the content, because it
governs every section and per-section copies would multiply it by seven.

### Copy is sentence case everywhere, or it is worse than not at all

Overriding three labels moved the inconsistency from between-tabs to within a
single screen. All 49 Title Case labels the settings tree renders are overridden
in `FORK_ONLY_STRINGS`, acronyms and proper nouns preserved, and post-processing
settles on one name — AI cleanup — rather than the three it had.

### The exhaustiveness check is a script, not a Playwright spec

`bun run check:settings`. The question is answerable from source, and answering
it in a browser would mean rendering every section against a Tauri backend CI
does not have. It follows `check-branding` and `check-translations`.

It had to be made barrel-aware: `Sidebar` imports one name from the settings
barrel, and following that barrel wholesale made `GeneralSettings`,
`AdvancedSettings` and `PostProcessingSettings` all look reachable — precisely
the claim the script exists to disprove.

Two limits, both found by trying to make it fail rather than by reasoning about
it. It proves reachability, not rendering. And because Debug is still
registered, a row demoted to debug-only would not trip it — deleting
`AlwaysOnMicrophone` from Audio passes, because Debug renders it too. Deleting
`SystemAudioCapture` fails, as it should.

### The preview harness renders the real thing

`brand-preview/` mocks Tauri's IPC seam via `@tauri-apps/api/mocks`, so the real
`Sidebar`, the real `SECTIONS_CONFIG` and the real section components render in
a plain browser. Every defect in this section was found there. It is committed
rather than left as scratch, which is what happened to its predecessor.
