# Follow-stream output

The fork-only `handy --follow-stream` feature lets another process follow live transcription output from an already-running Handy instance.

Two separate things decide whether a follower sees anything, and they must not be confused:

- **Per-mode publication** — whether a given capture's transcript reaches the hub at all. This is each mode's own `follow_stream_enabled` (Meeting's top-level **Follow Live Transcript Output**, Dictation's and Assisted Notes' own Advanced toggles). Meeting and Assisted Notes ship this on; Dictation ships it off, because dictated text has already been delivered where it was wanted.
- **Listener lifetime** — whether the local socket exists at all. The listener is process-wide, not per-mode, and stays up whenever _any_ mode that can currently publish wants it: Meeting's toggle, or an _enabled_ Dictation/Assisted Notes mode with its own publication toggle on. Turning Meeting's toggle off does not tear the socket down while another enabled mode still needs it, and a mode's publication preference does nothing while that mode itself is switched off.

| Mode                                 | Output                                                          |
| ------------------------------------ | --------------------------------------------------------------- |
| `json` (default, also the bare flag) | The full protocol as newline-delimited JSON                     |
| `delta`                              | JSONL, one record per newly-committed suffix                    |
| `text`                               | The plain `me: `/`them: ` rendering of that same committed text |

## NDJSON protocol

The stream is UTF-8, with exactly one JSON object per newline. Every object has a `t` discriminator; consumers should ignore fields they do not recognize so later protocol additions remain compatible. The current protocol is version 1:

```jsonl
{"t":"hello","protocol":1,"version":"0.9.7","capabilities":["toggle-assisted-notes","begin-mode"],"emitted_at":"2026-08-15T14:03:20.100-07:00"}
{"t":"begin","session":1,"streaming":true,"mode":"meeting","emitted_at":"2026-08-15T14:03:20.200-07:00","session_elapsed_ms":0}
{"t":"partial","session":1,"speaker":"me","committed":"hello ","tentative":"wor","emitted_at":"2026-08-15T14:03:21.412-07:00","session_elapsed_ms":1212}
{"t":"final","session":1,"speaker":"me","text":"Hello world.","emitted_at":"2026-08-15T14:03:22.050-07:00","session_elapsed_ms":1850}
{"t":"no_speech","session":1,"emitted_at":"...","session_elapsed_ms":700}
{"t":"cancel","session":1,"emitted_at":"...","session_elapsed_ms":700}
{"t":"error","session":1,"message":"transcription failed","emitted_at":"...","session_elapsed_ms":900}
```

`hello` is always the first event on a connection and reports the protocol and Handy versions. `capabilities` names the control flags this binary's parser accepts (currently just `toggle-assisted-notes`), not which settings are enabled — a follower still gets the app's own settings pane as the single description of behaviour, but this lets it tell an installed binary that predates a control flag from one that merely has the corresponding mode turned off, without guessing from a version number. The `begin-mode` capability says this binary's `begin` records carry a `mode` field. Each `begin` allocates a process-local, monotonically increasing `session` number; `streaming` says whether partial events are available for the selected model. `mode` names the capture mode that produced the session — `meeting`, `assisted-notes` or `dictation` — so a follower can decide whether a session is any of its business. Which modes reach a follower at all is still each mode's own publication setting, described at the top of this document; `mode` says what a delivered session was, not what is enabled. A follower that needs this field must gate on the `begin-mode` capability rather than on the field's absence, because an app that predates it is indistinguishable from one that has simply not started a session yet. A session ends with exactly one of `final`, `no_speech`, `cancel`, or `error`. Connection-level errors instead omit `session` and include a `code`, such as `follower_limit`.

In `partial` events, `committed` is the stable, append-only prefix and `tentative` is the volatile suffix. The `speaker` value is `"me"` for microphone audio and `"them"` for system audio. A single-lane `final` includes that speaker; `final.speaker` is omitted when the final text is a merged, speaker-labelled dual-speaker transcript.

A dual-speaker session can therefore look like this:

```jsonl
{"t":"hello","protocol":1,"version":"0.9.7","capabilities":["toggle-assisted-notes","begin-mode"],"emitted_at":"2026-08-15T14:03:20.100-07:00"}
{"t":"begin","session":42,"streaming":true,"mode":"meeting","emitted_at":"2026-08-15T14:03:20.200-07:00","session_elapsed_ms":0}
{"t":"partial","session":42,"speaker":"me","committed":"Can you hear me?","tentative":"","emitted_at":"2026-08-15T14:03:21.412-07:00","session_elapsed_ms":1212}
{"t":"partial","session":42,"speaker":"them","committed":"Yes, clearly.","tentative":"","emitted_at":"2026-08-15T14:03:22.900-07:00","session_elapsed_ms":2700}
{"t":"final","session":42,"text":"Me: Can you hear me?\nThem: Yes, clearly.","emitted_at":"2026-08-15T14:03:23.010-07:00","session_elapsed_ms":2810}
```

For example, to print only completed transcript text:

```sh
handy --follow-stream | jq -r 'select(.t=="final") | .text'
```

## Timestamps

Every event carries two time fields, stamped once when Handy produces the event:

- `emitted_at` — RFC3339 civil time with millisecond precision and a numeric UTC offset, never `Z`. Use it for display and for correlating a transcript with logs or recordings.
- `session_elapsed_ms` — milliseconds since this session's `begin`, read from a monotonic clock. **Use this as the ordering key.** Wall clocks move backward across NTP corrections, DST transitions, and suspend/resume, so `emitted_at` is not safe to sort by.

`session_elapsed_ms` is absent on events that belong to no session: `hello`, and connection-level `error`. It restarts at zero for every `begin`.

Both fields were added without bumping `protocol`, because they are additive and consumers are already told to ignore unrecognized fields. A bump is reserved for a removal, a rename, or a changed event meaning.

`begin.mode` was added the same way and for the same reason.

Two caveats on what a timestamp means:

- It records when text **became committed**, not when it was spoken. Decoding lags the audio.
- Partial events coalesce per follower (see below), so a slow follower receives the retained snapshot's timestamp and never sees the superseded ones. A single snapshot can therefore carry several commits at once, and its timestamp applies to the whole suffix.

Handy deliberately does not expose the decoder's audio offsets. The microphone and system-audio lanes run independent VAD, so their audio-relative times are not comparable with each other — which is also why there is no WebVTT or subtitle mode: there is no shared media clock to place a cue on, and cue durations would have to be invented.

## Delivery and attachment

Each follower receives every lifecycle event in order and eventually receives the latest partial state for each speaker; intermediate partial snapshots may be coalesced. If a follower falls far enough behind to exceed the bounded event or byte budget, Handy disconnects it rather than delivering a gap. There is no persistence or reconnect behavior.

Up to eight followers may be connected at once. A ninth receives one `error` event with code `follower_limit` and is closed. A follower attached during an active session receives `hello`, then the active `begin`, then the latest available `partial` snapshot for each speaker; earlier events are not replayed. Those replayed lines keep the timestamps they were originally produced with, so a late follower still learns when the session actually began — only its own `hello` is stamped at attach time.

## Delta mode

Run `handy --follow-stream delta` to transform the same NDJSON stream locally into one JSONL record per newly-committed suffix. Delta mode tracks committed text separately for each `(session, speaker)` and immediately emits only the new suffix. Tentative text produces no output.

```jsonl
{"t":"delta","schema":1,"session":42,"speaker":"me","text":"Can you hear me?","emitted_at":"2026-08-15T14:03:21.412-07:00","session_elapsed_ms":1212}
{"t":"delta","schema":1,"session":42,"speaker":"them","text":"Yes, clearly.","emitted_at":"2026-08-15T14:03:22.900-07:00","session_elapsed_ms":2700}
{"t":"end","schema":1,"session":42,"reason":"final","emitted_at":"2026-08-15T14:03:23.010-07:00","session_elapsed_ms":2810}
```

Each session closes with one `end` record whose `reason` is `final`, `no_speech`, `cancel`, or `error`; an `error` also carries its `message`. A connection-level rejection produces an `end` with no `session`.

`schema` versions this format independently of the wire `protocol`, because delta output is produced entirely client-side. It is at 1.

Both timestamp fields are copied straight through from the `partial` the suffix arrived on, and are omitted if the connected Handy did not send them.

```sh
handy --follow-stream delta | jq -r 'select(.t=="delta") | "\(.emitted_at) \(.speaker): \(.text)"'
```

## Text mode

Run `handy --follow-stream text` for the plain human-readable rendering of the same committed text. It prefixes the first output for a speaker with `me: ` or `them: `, inserts a newline and a new prefix when the active speaker changes, and writes a trailing newline when a session ends.

```text
me: Can you hear me?
them: Yes, clearly.
```

## Streaming models

Both `delta` and `text` are built only from `partial.committed`, so both require a streaming-capable model. On a non-streaming `begin` they print an error to stderr and exit non-zero; JSON mode continues to work because it passes the eventual `final` event through unchanged.

## Local transport and security

The follower is read-only and connects to a deterministic per-user local socket. On Windows, Handy creates a named pipe with a protected SDDL DACL granting access only to the current user's SID. On Unix, the listener uses mode `0600` and verifies each peer's effective user ID against Handy's own euid; this credential check also protects Linux abstract sockets where filesystem permissions do not apply. The listener and socket exist only while at least one mode can publish to it — see the note on per-mode publication vs. listener lifetime at the top of this document — and disappear once none can, disconnecting current followers.
