# Brand brief — Shorthand

Written to hand to a session doing logo ideation. It is the context that is not
in the code: what the product does, how it feels to use, where a mark has to
survive, and which directions have already been tried and rejected.

[BRANDING.md](BRANDING.md) is the authority on the current visual system —
palette, type, the sweep, the rules learned by screenshotting. This file is the
brief _behind_ it. Where the two disagree, BRANDING.md is what shipped.

The ideation this brief was written for has since run and been approved: clay
artwork of a bird perched on a fountain pen, with a coral wing and underline and
the name set in a soft serif. The pack lives in `brand-assets/`. Do not brief
this again.

---

## 1. What it is, in one paragraph

Shorthand is a desktop app that turns speech into written text, locally. You
press a hotkey, talk, and the words arrive — either typed into whatever window
you were already in, or captured as a meeting transcript and streamed live to a
note-taking tool. It runs on your machine: the speech models are downloaded and
executed locally, nothing is sent anywhere, and by default it does not even keep
a copy of what you said.

It is a fork of [Handy](https://github.com/cjpais/Handy), rebranded and extended.

---

## 2. What it actually does

### The loop

    hotkey → record → voice-activity detection → local speech model
           → optional AI cleanup → text lands

Three to fifteen seconds for a dictation. An hour for a meeting. The same loop
either way.

### Two modes, two rhythms

This distinction matters more than anything else in this document, because the
two halves of the product feel completely different and the mark has to hold
both.

**Dictation** is a _burst_. You are mid-sentence in an email, a commit message,
a message to someone. You hold a key, say the thing, let go, and the text is
in the box. Two seconds. It happens dozens of times a day and you stop noticing
it. The app is never on screen. Success is that you forget it exists.

**Meetings** is a _vigil_. You start it at the top of a call and it listens for
an hour. It can capture system audio alongside your microphone on Windows, so
both sides of the conversation are transcribed and speaker-labelled. The
transcript streams out live over a local socket while the meeting is still
happening, so a note-taker downstream is building the note as people talk. You
are not watching it. You are in the meeting. Success is that when you look up,
the record is already written.

One is _fast_. One is _faithful_. Both are unattended.

### Local, and quiet about it

- Speech models (Whisper family, Parakeet, Moonshine, SenseVoice) run on-device.
- `save_recordings` and `save_transcripts` both default to **off** — the app's
  resting state is to leave no trace.
- The only network egress is optional: AI cleanup, if you point it at an API,
  and update checks, which can be switched off.

This is not a privacy _pitch_. There is no shield iconography, no lock, no
"your data never leaves your device" banner. It is simply how the thing is
built, and the restraint is part of the character.

### AI cleanup

Optional post-processing: an LLM tidies the raw transcript — filler words,
punctuation, whatever your prompt asks for. You write and keep your own prompt
library. Per-mode, so a meeting summary and a dictated sentence can be handled
differently.

### Why the fork exists

`--follow-stream` — a live transcript event stream over a per-user local socket.
It is the reason this is a fork rather than a config of upstream. Downstream,
`shorthand-core` consumes that stream and an Obsidian plugin turns it into notes
as the meeting happens.

So the full arc is: **speech → transcript → a note someone will actually reread.**
The app is the first third of that. It is a capture instrument feeding a
writing system.

---

## 3. Where the mark has to survive

Read this before sketching. Several otherwise-good directions die here.

| Context                    | Size         | Constraint                                                                                                                           |
| -------------------------- | ------------ | ------------------------------------------------------------------------------------------------------------------------------------ |
| **System tray / menu bar** | 16–32px      | The primary home. The app lives here.                                                                                                |
| macOS menu bar             | 16px         | **Template mode** — `set_icon_with_as_template(…, true)`. Alpha only. Every pixel is on or off. No colour, no gradient, no two-tone. |
| Windows tray               | 16px         | On a taskbar that may be light or dark.                                                                                              |
| App icon                   | up to 1024px | Sliced by `tauri icon` into `.ico`, `.icns`, Windows Square tiles, iOS AppIcon, Android mipmaps.                                     |
| Sidebar icon               | 24px         | Fills with `currentColor`, inheriting the row's ink.                                                                                 |
| Wordmark lockup            | ~22px cap    | See below.                                                                                                                           |

Hard rules that fall out of that:

1. **Monochrome or it does not ship.** One colour, set by the consumer. The
   current component is literally `<path fill="currentColor" />`. Anything
   needing two tones to be legible cannot be the mark.
2. **It must read at 16px.** Most people will only ever see it at 16px, in
   peripheral vision, next to a wifi icon.
3. **Three tray states exist** — `Idle`, `Recording`, `Transcribing`. A mark
   that can express "listening" and "working" as variations of itself is worth
   more than one that needs three unrelated glyphs.
4. **The lockup is settled.** The old wordmark substituted the mark _for_ the
   initial S — `[mark]horthand` — because a separate bug beside "Shorthand"
   would have printed two S's and said nothing. The approved artwork answered
   the question differently: the mark stacks above the complete word
   `Shorthand`, with a coral sweep beneath. See BRANDING.md.
5. **This no longer applies.** SVG cannot vary `stroke-width` along a path, so
   the old pointed-pen "s" needed its spine offset both sides by a generated
   width profile (`scripts/gen-brand-mark.ts`, ~200 coordinates). That mark and
   its generator are both deleted. The current mark is approved artwork,
   transcribed once into `src/shorthand/brand/mark.paths.ts`, not generated.

---

## 4. The moment of use

Worth designing for the actual moment rather than the category.

**Dictation.** You are already writing. You hit a wall — the sentence is in your
head but typing it is friction, or your hands are busy, or you are on a laptop
in a chair with bad posture. You hold a key and say it. There is a beat of
_will it get it right_, then the words appear and you carry on. The good version
of this feeling is **relief with a small kick of delight**. It worked, it was
fast, you did not have to type.

**Meetings.** You are about to be busy for an hour and you do not want to spend
that hour taking notes, because taking notes means not listening. You start
capture and forget it. The good version of this feeling is **trust you can stop
thinking about** — the sense of having set something reliable running.

The failure modes are equally specific: dictation that mangles a name, or a
meeting where you discover afterwards it was not recording. Both are betrayals
of exactly the trust above.

---

## 5. What it should feel like

The brief, stated by the maintainer: **playful but useful simplicity.**

Unpacked into things a mark can actually be judged against:

- **Written, not recorded.** The output is _text_. Almost every voice product
  reaches for a microphone, a waveform, a soundwave — the input. Shorthand's
  subject is the output: the mark on the page. This is the single strongest
  differentiator available and the current direction already leans on it.
- **Fast and confident.** One gesture, no hesitation, no correction. A stroke
  laid down in one pass. Nothing tentative, nothing fussy.
- **Made by a hand.** The counterweight to how machine-generated the underlying
  thing is. The whole visual system is built on this: paper, ink, a highlighter
  sweep, a pen line. Warmth against the fact that it is a neural net.
- **Quiet by default, with one surprise.** The UI is almost entirely achromatic;
  colour appears only on the thing that is currently live. Playfulness is _one_
  well-placed moment, not general jauntiness. Never a mascot, never a face,
  never a wink.
- **It disappears.** The product's ideal state is being forgotten. The mark
  should be comfortable being small, peripheral, and unremarkable in the tray —
  and reward attention when it is finally looked at up close.

Adjacent feelings to avoid: clerical, archival, bureaucratic, "enterprise
transcription", medical dictation, legal steno, courtroom. Those are all
adjacent to the subject matter and all wrong for the tone. The last direction
died on exactly this.

---

## 6. The word

"Shorthand" is not an invented product name. It is a real writing system —
Pitman, Gregg, Teeline — designed so a human could capture speech in real time.
Gregg in particular is nothing but flowing pen curves: loops, hooks, a single
continuous line, radically simplified so it can be written as fast as people
talk.

That is _precisely what this app does_, four hundred years later, and it is a
deep and mostly untouched well:

- a continuous line that never lifts
- radical compression — a whole word as one stroke
- notation, marks, a system of symbols rather than letters
- the _speed_ of the hand keeping up with the mouth
- shorthand is also, colloquially, "a quick way of saying a longer thing"

The current mark takes one sip from this well (a pen-written S). There is a lot
left in it.

---

## 7. What existed when this brief was written

Superseded. This describes the system the ideation was briefed against, not
what ships now — see BRANDING.md for the current mark, wordmark and palette.

**The mark.** A lowercase "s" written with a pointed pen: one continuous stroke
that swells through each bowl and thins to a point at the entry, the waist and
the exit. Three cubic segments plus a width profile, generated to an outline.
It fills with `currentColor` and serves as the sidebar icon, the wordmark's
initial, and the tray artwork.

**The wordmark.** `[mark]horthand` — the mark standing in for the initial S, set
in the app's own typeface (Atkinson Hyperlegible Next) rather than as outlines.

**The system** (see BRANDING.md for the reasoning):

| Role                    | Light     | Dark      |
| ----------------------- | --------- | --------- |
| paper                   | `#faf8f2` | `#12141a` |
| ink                     | `#12151f` | `#eceef4` |
| ink at writing strength | `#12459e` | `#6aa9f5` |
| ink at full strength    | `#1e5bd6` | same      |
| highlighter (fork-only) | `#ffb0c4` | `#f59ab2` |

Type is **Atkinson Hyperlegible Next**, drawn by the Braille Institute so
characters cannot be confused with one another. Chosen because the app's entire
subject is the legibility of a written record, and because it carries warmth and
quirk without being a novelty face.

The one motif is a **highlighter sweep** — a rose stroke with a blue pen line
under it, marking the live thing and nothing else.

---

## 8. Already tried and rejected

Do not re-propose these. Each was chosen for a reason and then killed for a
reason, and both are worth knowing.

| Direction                                                     | Why it was chosen                                              | Why it died                                                                                                                                        |
| ------------------------------------------------------------- | -------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------- |
| **Teal accent**                                               | shipped first, inherited                                       | too common; the default "friendly tech" colour                                                                                                     |
| **Copying-pencil violet** (`#7645ad`, IBM Plex, halved radii) | archival, indelible, a real stationery reference               | said _clerical and bureaucratic_ — the exact tone the product is not                                                                               |
| **Blue alone**                                                | conventional, safe, accessible                                 | a lone blue accent is the default of most software written this decade                                                                             |
| **Chartreuse highlighter** (`#e8f35c`)                        | "the complement of blue, which is why highlighters are yellow" | not actually the complement (64° vs a needed ~38°), clashed with warm paper, and a yellow highlighter is the least surprising object in stationery |
| **Apricot highlighter** (`#ffc48a`)                           | inside the paper's own hue family, harmonious                  | differed from the page in chroma alone — read as a tint _of_ the page rather than a mark _on_ it                                                   |
| **Blue ink + rose highlighter**                               | made "a marked-up transcript" literal and non-generic          | not killed — it shipped and worked. Superseded once the clay-bird ideation returned approved artwork with its own colour source                    |
| **Arbitrary status icons**                                    | conventional                                                   | rejected: symbols should be derived from the subject, not picked from a set                                                                        |

The standing principle behind most of those rejections: **derive the choice from
the subject.** A decision that could have been made for any app is the wrong
decision for this one.

Also ruled out by role rather than taste: green (reads _success_), amber and
orange (spoken for by `--color-warning`), violet (the direction above).

---

## 9. Where there is room

Written before the ideation ran. Status below reflects what the approved
artwork settled and what it left open.

1. **Shorthand-as-notation.** Settled. `direction.md` describes "the connected
   S" as part of the approved mark — the well this item pointed at is what the
   artwork drew from.
2. **The state system.** Still only partly settled. The regenerated tray icons
   keep one mark placement for Idle and add a badge in the strip beneath it for
   Recording and Transcribing, rather than three states of the mark itself —
   the mark does not change shape between states, only the badge does. Open.
3. **The two modes.** Still entirely open. Nothing in the identity distinguishes
   the dictation burst from the meetings vigil.
4. **The lockup.** Settled. The approved artwork stacks the mark above the
   complete word `Shorthand`, with a coral sweep beneath, rather than
   substituting for the initial S.
5. **Speech → text as a single move.** Answered, and not as this brief
   predicted — the strong prior here was that the spoken half would not be
   represented. The approved artwork represents both: the bird carries the
   thought, the fountain pen commits it.
