# Follow-stream output

The fork-only `handy --follow-stream` feature lets another process follow live transcription output from an already-running Handy instance.

Two separate things decide whether a follower sees anything, and they must not be confused:

- **Per-mode publication** — whether a given capture's transcript reaches the hub at all. This is each mode's own `follow_stream_enabled` (Meeting's top-level **Follow Live Transcript Output**, Dictation's and Assisted Notes' own Advanced toggles). Meeting and Assisted Notes ship this on; Dictation ships it off, because dictated text has already been delivered where it was wanted.
- **Listener lifetime** — whether the local socket exists at all. The listener is process-wide, not per-mode, and is **unconditional**: it starts once at startup and stays up for the life of the process, regardless of any mode's publication setting.

These two used to be coupled — the listener only ran while some mode's publication setting could use it — but that made the socket unavailable in exactly the case a `refused` record exists to explain: if every publishing mode is off, "the mode is disabled" has no transport to arrive on, and the follower's documented attach-first flow (see "Level-triggered attachment" below) can never observe it. Making the listener unconditional is a **lifetime** change only; it does not change **who receives transcripts**. A follower can still attach at any time — disabled or not — and will always see `capture_state` plus any applicable `begin`/`refused`/`start_failed` records for the connection itself; whether a given capture's transcript ever reaches it is still entirely decided by that capture's own publication toggle, resolved before the start path calls the hub.

| Mode                                 | Output                                                          |
| ------------------------------------ | --------------------------------------------------------------- |
| `json` (default, also the bare flag) | The full protocol as newline-delimited JSON                     |
| `delta`                              | JSONL, one record per newly-committed suffix                    |
| `text`                               | The plain `me: `/`them: ` rendering of that same committed text |

## NDJSON protocol

The stream is UTF-8, with exactly one JSON object per newline. Every object has a `t` discriminator; consumers should ignore fields they do not recognize and skip record types they do not support so later protocol additions remain compatible. The current protocol is version 1:

```jsonl
{"t":"hello","protocol":1,"version":"0.9.7","capabilities":["toggle-assisted-notes","start-assisted-notes","stop-assisted-notes","begin-mode","capture-state","refused","refused-publication-disabled","start-failed","start-failed-code"],"emitted_at":"2026-08-15T14:03:20.100-07:00"}
{"t":"capture_state","phase":"idle","emitted_at":"2026-08-15T14:03:20.100-07:00"}
{"t":"begin","session":1,"streaming":true,"mode":"meeting","emitted_at":"2026-08-15T14:03:20.200-07:00","session_elapsed_ms":0}
{"t":"partial","session":1,"speaker":"me","committed":"hello ","tentative":"wor","emitted_at":"2026-08-15T14:03:21.412-07:00","session_elapsed_ms":1212}
{"t":"final","session":1,"speaker":"me","text":"Hello world.","emitted_at":"2026-08-15T14:03:22.050-07:00","session_elapsed_ms":1850}
{"t":"no_speech","session":1,"emitted_at":"...","session_elapsed_ms":700}
{"t":"cancel","session":1,"emitted_at":"...","session_elapsed_ms":700}
{"t":"error","session":1,"message":"transcription failed","emitted_at":"...","session_elapsed_ms":900}
```

`hello` is always the first event on a connection and reports the protocol and Handy versions. `capabilities` names the capabilities this binary supports: a control flag appears here as the CLI flag minus its `--`, while other capabilities name a feature of the wire format instead of a flag. `begin-mode` means `begin` records carry `mode`; `capture-state`, `refused`, and `start-failed` mean those record types exist (each capability name is kebab-case regardless of the record's own `t` spelling). `refused-publication-disabled` is narrower: it says `refused.reason` can be `publication-disabled`. Likewise, `start-failed-code` says `start_failed` carries the machine-readable `code` field described below; `start-failed` alone only promises the record type. Capabilities advertise what this binary can do, not which settings are enabled — a follower still gets the app's own settings pane as the single description of behaviour, but can tell a binary that predates a capability from one whose corresponding mode is merely switched off, without guessing from a version number. Each `begin` allocates a process-local, monotonically increasing `session` number; `streaming` says whether partial events are available for the selected model. `mode` names the capture mode that produced the session — `meeting`, `assisted-notes` or `dictation` — so a follower can decide whether a session is any of its business. Which modes reach a follower at all is still each mode's own publication setting, described at the top of this document; `mode` says what a delivered session was, not what is enabled. A follower that needs this field must gate on the `begin-mode` capability rather than on the field's absence, because an app that predates it is indistinguishable from one that has simply not started a session yet. A session ends with exactly one of `final`, `no_speech`, `cancel`, or `error`. Connection-level errors instead omit `session` and include a `code`, such as `follower_limit`.

`capture_state` always follows `hello` — see "Connection state: `capture_state` and `begin`" below. `refused` and `start_failed` are two more connection-level records described in "Explicit start/stop commands" below. None carries `session_elapsed_ms`, because none is a session event; `capture_state.session`, when present, identifies the active publication but does not change the record's connection-snapshot semantics.

In `partial` events, `committed` is the stable, append-only prefix and `tentative` is the volatile suffix. The `speaker` value is `"me"` for microphone audio and `"them"` for system audio. A single-lane `final` includes that speaker; `final.speaker` is omitted when the final text is a merged, speaker-labelled dual-speaker transcript.

A dual-speaker session can therefore look like this:

```jsonl
{"t":"hello","protocol":1,"version":"0.9.7","capabilities":["toggle-assisted-notes","start-assisted-notes","stop-assisted-notes","begin-mode","capture-state","refused","refused-publication-disabled","start-failed","start-failed-code"],"emitted_at":"2026-08-15T14:03:20.100-07:00"}
{"t":"capture_state","phase":"recording","mode":"meeting","publishing":true,"session":42,"emitted_at":"2026-08-15T14:03:20.100-07:00"}
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

`session_elapsed_ms` is absent on events that belong to no session: `hello`, `capture_state`, `refused`, `start_failed`, and connection-level `error`. It restarts at zero for every `begin`.

Both fields were added without bumping `protocol`, because they are additive and consumers are already told to ignore unrecognized fields. Protocol 1 also uses `hello.capabilities` for record vocabulary: the `idle` → `capture_state` replacement remains version 1 because the capability list advertises which record exists before the follower reads it. A bump is reserved for an incompatible framing or semantic change that cannot be discovered from `hello` before parsing the affected record.

`begin.mode` was added the same way and for the same reason.

Two caveats on what a timestamp means:

- It records when text **became committed**, not when it was spoken. Decoding lags the audio.
- Partial events coalesce per follower (see below), so a slow follower receives the retained snapshot's timestamp and never sees the superseded ones. A single snapshot can therefore carry several commits at once, and its timestamp applies to the whole suffix.

Handy deliberately does not expose the decoder's audio offsets. The microphone and system-audio lanes run independent VAD, so their audio-relative times are not comparable with each other — which is also why there is no WebVTT or subtitle mode: there is no shared media clock to place a cue on, and cue durations would have to be invented.

## Delivery and attachment

Each follower receives every lifecycle event in order and eventually receives the latest partial state for each speaker; intermediate partial snapshots may be coalesced. If a follower falls far enough behind to exceed the bounded event or byte budget, Handy disconnects it rather than delivering a gap. There is no persistence or reconnect behavior.

Up to eight followers may be connected at once. A ninth receives one `error` event with code `follower_limit` and is closed. A successful follower always receives `hello`, then `capture_state`. If a published session is active it next receives that session's `begin`, then the latest available `partial` snapshot for each speaker; earlier events are not replayed. Those replayed lines keep the timestamps they were originally produced with, so a late follower still learns when the session actually began — only its own `hello` and `capture_state` are stamped at attach time. See the next section for the snapshot fields and ordering.

## Connection state: `capture_state` and `begin`

A follower always learns the coordinator's current phase immediately after `hello`, whether or not the capture is published:

```jsonl
{"t":"hello","protocol":1,"version":"0.9.7","capabilities":["toggle-assisted-notes","start-assisted-notes","stop-assisted-notes","begin-mode","capture-state","refused","refused-publication-disabled","start-failed","start-failed-code"],"emitted_at":"2026-08-15T14:03:20.100-07:00"}
{"t":"capture_state","phase":"idle","emitted_at":"2026-08-15T14:03:20.100-07:00"}
```

`phase` is `idle`, `recording`, or `processing`. While idle, `mode`, `publishing`, and `session` are omitted. While recording or processing, `mode` is `meeting`, `assisted-notes`, or `dictation`, and `publishing` is always present. `publishing:false` means the capture is real but its resolved per-mode `follow_stream_enabled` value keeps it off this wire; there is consequently no `session` and no `begin` to wait for.

For a published capture, `session` is present exactly when the hub still has an active publication. The same ID appears on the immediately following replayed `begin`:

```jsonl
{"t":"capture_state","phase":"recording","mode":"assisted-notes","publishing":true,"session":42,"emitted_at":"2026-08-15T14:03:20.100-07:00"}
{"t":"begin","session":42,"streaming":true,"mode":"assisted-notes","emitted_at":"2026-08-15T14:03:19.900-07:00","session_elapsed_ms":0}
```

Phase and mode come from the transcription coordinator's authoritative `Stage`, not from whether the hub happens to have a session: a non-publishing recording has no hub session but must still report `recording`. The coordinator resolves publication where settings are available and passes that value into the settings-free hub. The hub contributes only the active session ID it owns, which is what guarantees the ID matches the replayed `begin`.

`capture_state` only ever appears immediately after `hello`, never mid-connection. It replaces the older `idle` record because `idle` could describe only one state and forced a follower to infer that an idempotent `--start-assisted-notes` no-op meant "already recording." With this snapshot, an already-running Assisted Notes capture is directly observable before the command is issued; no post-command edge is required to prove the requested state already held.

## Explicit start/stop commands

`--start-assisted-notes` and `--stop-assisted-notes` start or stop an Assisted Notes capture on a running instance, the same as `--toggle-assisted-notes`, but without toggle semantics: each is idempotent, so a caller can retry one without risking flipping the capture the wrong way. `--toggle-assisted-notes` still exists — fork-only and harmless for manual, interactive use — but a scripted or programmatic caller should prefer the explicit pair.

| Command                   | Assisted Notes idle | Assisted Notes already capturing | A different mode capturing | Assisted Notes disabled     | Assisted Notes enabled but not publishing |
| -------------------------- | -------------------- | --------------------------------- | --------------------------- | ----------------------------- | ------------------------------------------- |
| `--start-assisted-notes`  | starts it            | no-op (success)                   | refused (`busy`)            | refused (`mode-disabled`)    | refused (`publication-disabled`)            |
| `--stop-assisted-notes`   | no-op (success)      | stops it                          | no-op (success)             | no-op (success)              | no-op (success)                             |

A refusal is reported on the wire instead of being left for the caller to infer from a timeout:

```jsonl
{"t":"refused","mode":"assisted-notes","reason":"mode-disabled","emitted_at":"2026-08-15T14:03:20.100-07:00"}
{"t":"refused","mode":"assisted-notes","reason":"busy","emitted_at":"2026-08-15T14:03:20.100-07:00"}
{"t":"refused","mode":"assisted-notes","reason":"publication-disabled","emitted_at":"2026-08-15T14:03:20.100-07:00"}
```

`refused` carries no request id — this protocol is one-way, with no request/response correlation — so it only says "a start or stop for `mode` was just declined, and why", not which of possibly several outstanding commands it answers. A follower tells its own command's outcome apart from anyone else's by having attached first, read current state, then issued the command and watched the same connection for the resulting state change or `refused` — see "Level-triggered attachment" below.

`publication-disabled` was added after `busy` and `mode-disabled` shipped, as a value for the existing `reason` string field rather than a shape change — but that is not, on its own, grounds to call `reason` an open set a follower must already tolerate. Protocol 1's contract (see the top of this document) is that consumers ignore *fields* they do not recognize; it says nothing about an existing field's own values being open-ended, and this document cannot retroactively grant an older follower behaviour it was never written to have. What actually makes `publication-disabled` safe to add without bumping the protocol is the `hello` capability list: `refused-publication-disabled` (distinct from `refused`, which only says the record type exists — see the capabilities table above and the `hello` example) tells a follower whether the connected binary can send this specific reason at all, in advance of ever seeing one. A follower that gets a `reason` it does not recognise — whether because it predates `refused-publication-disabled` or a future capability — should treat the refusal as an unexplained decline (the command was refused, but not why) rather than fail to parse the record; that is the follower's fallback for an unrecognised value, not license to assume every binary that can send `refused` can send every reason it might ever carry. Adding `publication-disabled` and its capability did not bump `FOLLOW_PROTOCOL_VERSION`: the record's shape did not change, and capability-described vocabulary changes remain within protocol 1 (see the `idle` → `capture_state` precedent above).

Be clear about what that capability does and does not buy, though: this protocol is one-way, so a follower cannot negotiate the value away. The capability lets a *new* follower discover in advance that this binary may send `publication-disabled`; it cannot stop an *older* follower with a closed `reason` enum from receiving one and rejecting the record. That residual risk is accepted deliberately rather than designed away, on the grounds that no follower deployed today can hit it: `shorthand-core`'s `parseWireRecord` requires a numeric `session` on every record except `hello` and a session-less `error`, so it drops every `refused` record whatever its reason. The alternative — bumping the protocol — would break every existing follower outright over a purely additive value, which is strictly worse. If a strict third-party follower ever does exist, that calculus changes and this decision should be revisited.

`--start-assisted-notes` refuses with `publication-disabled` when Assisted Notes is enabled but its own `--follow-stream` publication toggle (the Modes pane's per-mode switch, `assisted_notes.follow_stream_enabled`) is off. Forwarding the start anyway would begin a real capture that never emits `begin` — the same switch `hub.begin` itself is gated on — leaving a follower with no way to observe it, distinguish it from a lost command, or learn what to fix. `--stop-assisted-notes` is never refused for this reason, the same as it is never refused for the mode being disabled (table above): it must still be able to end a capture already running, even one whose publication toggle was flipped off mid-capture.

A command can be accepted (the CLI flag exits 0, and the running instance receives it) and still fail to actually start a capture — no input device, a denied microphone permission. That produces `start_failed` rather than `begin`:

```jsonl
{"t":"start_failed","mode":"assisted-notes","code":"no-input-device","message":"No input device found","emitted_at":"2026-08-15T14:03:20.100-07:00"}
```

`start_failed` is deliberately its own record rather than a session-less `error`. It carries `mode` for the same reason `begin` does — without it a follower watching one mode could misattribute a different mode's failure to itself — and, like `begin`, only reaches a follower when that mode's resolved `follow_stream_enabled` publication toggle is on: a non-publishing mode's failures are exactly what that setting exists to keep off the wire.

`code` is stable kebab-case machine vocabulary; `message` retains the human-readable platform detail. The start path currently produces these classifications:

- `microphone-permission-denied` — the existing recorder classifier recognised an access-denied, permission-denied, or Windows `0x80070005` microphone error.
- `no-input-device` — the existing recorder classifier recognised no default input device, including CoreAudio's equivalent preferred-config failure.
- `audio-capture-failed` — another error from VAD loading, device resolution/configuration/opening, recorder availability, or recorder worker start. Those layers currently return strings rather than a stronger typed cause, so this is deliberately one catch-all.

The `start-failed-code` capability tells a follower the field is present. As with `refused.reason`, the values are an open set for parsing: a follower that sees an unrecognised code must still accept `start_failed` and treat it as an unexplained failure, preserving `message` for display or logs, rather than reject the record. The capability advertises the field, not a closed promise that these are the only values a future binary can send.

## Level-triggered attachment

**Attach first, read current state, then issue a command — and treat every record on this connection as state to react to, not as a one-shot signal to wait for.** A follower that instead arms a timer for a single expected edge before issuing its command can lose the very confirmation it is waiting for.

That is not hypothetical; it is the bug this design replaces. The previous integration spawned a second process to send `--toggle-assisted-notes`, then — only after that spawned process exited — opened a *separate* follow-stream connection and waited for `begin`. Handy emits `begin` roughly 20ms *before* the spawned process actually exits, so by the time the waiting connection attached, `begin` had already been sent and was gone. The wait timed out, the integration cancelled the capture it had itself just started, and told the user to enable a setting that was already enabled.

The fix is not a faster timer; it is this ordering. Attach before issuing any command, so `capture_state` and any active `begin` are already known when the command goes out. If `capture_state` already reports the requested mode recording, an idempotent start is already satisfied and deliberately emits no new edge. Otherwise issue an *explicit* command rather than a toggle, so a retry sent because the first attempt's confirmation seemed lost can never fire the wrong edge. Then watch that same, already-open connection for `begin`, `refused`, or `start_failed` — never a fresh connection racing an event that may already have been sent and missed. A future "simplification" back to wait-for-one-edge-on-a-fresh-connection reintroduces exactly this race; the fix is the ordering, not the timeout value.

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

The follower is read-only and connects to a deterministic per-user local socket. On Windows, Handy creates a named pipe with a protected SDDL DACL granting access only to the current user's SID. On Unix, the listener uses mode `0600` and verifies each peer's effective user ID against Handy's own euid; this credential check also protects Linux abstract sockets where filesystem permissions do not apply. The listener and socket exist for the life of the process — see the note on per-mode publication vs. listener lifetime at the top of this document — regardless of which modes, if any, currently publish to it.
