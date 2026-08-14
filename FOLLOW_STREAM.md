# Follow-stream output

The fork-only `handy --follow-stream` feature lets another process follow live transcription output from an already-running Handy instance. It is off by default; enable **Follow Live Transcript Output** in Advanced settings before connecting. The bare flag and `json` mode write newline-delimited JSON (NDJSON), while `delta` mode writes append-only committed text.

## NDJSON protocol

The stream is UTF-8, with exactly one JSON object per newline. Every object has a `t` discriminator; consumers should ignore fields they do not recognize so later protocol additions remain compatible. The current protocol is version 1:

```jsonl
{"t":"hello","protocol":1,"version":"0.9.5"}
{"t":"begin","session":1,"streaming":true}
{"t":"partial","session":1,"speaker":"me","committed":"hello ","tentative":"wor"}
{"t":"final","session":1,"speaker":"me","text":"Hello world."}
{"t":"no_speech","session":1}
{"t":"cancel","session":1}
{"t":"error","session":1,"message":"transcription failed"}
```

`hello` is always the first event on a connection and reports the protocol and Handy versions. Each `begin` allocates a process-local, monotonically increasing `session` number; `streaming` says whether partial events are available for the selected model. A session ends with exactly one of `final`, `no_speech`, `cancel`, or `error`. Connection-level errors instead omit `session` and include a `code`, such as `follower_limit`.

In `partial` events, `committed` is the stable, append-only prefix and `tentative` is the volatile suffix. The `speaker` value is `"me"` for microphone audio and `"them"` for system audio. A single-lane `final` includes that speaker; `final.speaker` is omitted when the final text is a merged, speaker-labelled dual-speaker transcript.

A dual-speaker session can therefore look like this:

```jsonl
{"t":"hello","protocol":1,"version":"0.9.5"}
{"t":"begin","session":42,"streaming":true}
{"t":"partial","session":42,"speaker":"me","committed":"Can you hear me?","tentative":""}
{"t":"partial","session":42,"speaker":"them","committed":"Yes, clearly.","tentative":""}
{"t":"final","session":42,"text":"Me: Can you hear me?\nThem: Yes, clearly."}
```

For example, to print only completed transcript text:

```sh
handy --follow-stream | jq -r 'select(.t=="final") | .text'
```

## Delivery and attachment

Each follower receives every lifecycle event in order and eventually receives the latest partial state for each speaker; intermediate partial snapshots may be coalesced. If a follower falls far enough behind to exceed the bounded event or byte budget, Handy disconnects it rather than delivering a gap. There is no persistence or reconnect behavior.

Up to eight followers may be connected at once. A ninth receives one `error` event with code `follower_limit` and is closed. A follower attached during an active session receives `hello`, then the active `begin`, then the latest available `partial` snapshot for each speaker; earlier events are not replayed.

## Delta mode

Run `handy --follow-stream delta` to transform the same NDJSON stream locally. Delta mode tracks committed text separately for each `(session, speaker)` and immediately prints only the newly committed suffix. It prefixes the first output for a speaker with `me: ` or `them: `, inserts a newline and a new prefix when the active speaker changes, and writes a trailing newline for terminal events. Other events and tentative text produce no output.

Delta mode requires a streaming-capable model because it is built only from `partial.committed`. On a non-streaming `begin`, it prints an error to stderr and exits non-zero; JSON mode continues to work because it passes the eventual `final` event through unchanged.

## Local transport and security

The follower is read-only and connects to a deterministic per-user local socket. On Windows, Handy creates a named pipe with a protected SDDL DACL granting access only to the current user's SID. On Unix, the listener uses mode `0600` and verifies each peer's effective user ID against Handy's own euid; this credential check also protects Linux abstract sockets where filesystem permissions do not apply. The listener and socket exist only while the setting is enabled, and disabling it disconnects current followers.
