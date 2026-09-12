# Request socket

The fork-only request socket is a second same-user local socket, next to the read-only [follow-stream](FOLLOW_STREAM.md). Through it, `shorthand-core` and the Obsidian plugin reach three things without ever holding a provider secret themselves: the app's OS credential store, an HTTP proxy that injects the secret bound to a request's provider and origin, and a WebSocket relay that does the same for the ACP network transport. The app stores only secrets, keyed by provider and endpoint origin; non-secret settings stay with the client.

Server side lives in `src-tauri/src/shorthand/request_socket/`. Client side is core's `ShorthandAppClient`.

## Wire contract

Transport: NDJSON over the request socket, UTF-8, one JSON object per `\n`. Max line 32 MiB. The server writes exactly one `hello` first.

Socket location (discovery file, written by the app on listen, removed on clean stop): `<shorthand config dir>/request-socket.json`, mode 0600 on Unix. `<shorthand config dir>` is core's `shorthandConfigDirectory()`: win32 `%APPDATA%\Shorthand`, darwin `~/Library/Application Support/Shorthand`, linux `$XDG_CONFIG_HOME/shorthand` or `~/.config/shorthand`.

```json
{"protocol":1,"path":"\\\\.\\pipe\\shorthand.request.S-1-5-21-…"}
{"protocol":1,"path":"/Users/me/Library/Application Support/Shorthand/request.sock"}
```

Windows: named pipe `shorthand.request.<SID>` with the same protected DACL as follow-stream. Unix: filesystem socket `<config dir>/request.sock`, mode 0600, peer euid checked on accept.

Messages:

```jsonc
// server → client, first line
{"t":"hello","protocol":1,"version":"0.5.0","capabilities":["credential","http-fetch","ws-relay"]}

// client → server
{"id":"r1","method":"credential.set","params":{"slot":SLOT,"secret":"sk-…"}}
{"id":"r2","method":"credential.clear","params":{"slot":SLOT}}
{"id":"r3","method":"credential.status","params":{"slots":[SLOT,…]}}
{"id":"r4","method":"http.fetch","params":{"slot":SLOT,"url":"https://api.openai.com/v1/chat/completions","method":"POST","headers":{"content-type":"application/json"},"body":"<base64>"}}
{"id":"r5","method":"http.abort","params":{"request":"r4"}}
{"id":"r6","method":"ws.open","params":{"slot":SLOT,"url":"wss://agent.example/acp","protocols":[]}}
{"id":"r7","method":"ws.send","params":{"stream":"s1","data":"{\"jsonrpc\":…}"}}
{"id":"r8","method":"ws.close","params":{"stream":"s1","code":1000,"reason":""}}

// server → client, responses
{"id":"r1","ok":true,"result":{}}
{"id":"r3","ok":true,"result":{"statuses":[{"slot":SLOT,"status":"configured"}]}}   // configured | missing | unavailable
{"id":"r4","ok":true,"result":{"status":200,"headers":{"content-type":"application/json"}}}
{"id":"r6","ok":true,"result":{"stream":"s1"}}
{"id":"r1","ok":false,"error":{"code":"origin_mismatch","message":"…"}}

// server → client, events (no id)
{"t":"http.body","request":"r4","data":"<base64 chunk>"}
{"t":"http.end","request":"r4"}
{"t":"http.error","request":"r4","message":"…"}
{"t":"ws.message","stream":"s1","data":"…"}
{"t":"ws.closed","stream":"s1","code":1000,"reason":""}
{"t":"ws.error","stream":"s1","message":"…"}
```

In `credential.status`'s response, each `slot` is the slot exactly as the client sent it in the request, not a canonicalised rewrite — the client matches entries back to what it asked about by comparing that echo against its own bytes. Canonicalisation happens only to derive the keyring lookup and to detect duplicates: two slots in one request that canonicalise to the same entry answer `bad_request` rather than two status entries for one secret. Entries are returned in request order.

`SLOT` shapes (the app derives the keyring entry from these; `origin` is `scheme://host[:port]`, lower-case, no path):

```jsonc
{"kind":"notes-llm","provider":"openai","origin":"https://api.openai.com"}       // provider ∈ openai | anthropic | ollama | openai-compatible
{"kind":"notes-acp","vaultId":"3f9a1c…16 hex","origin":"wss://agent.example"}
```

Error codes: `bad_request`, `unknown_method`, `origin_mismatch`, `credential_missing`, `credential_unavailable`, `too_large`, `timeout`, `upstream`, `limit`.

Auth injection rules in `http.fetch` and `ws.open`: strip any client-supplied `authorization`, `x-api-key`, `cookie`, `proxy-authorization`. Also strip the framing and routing headers the proxy decides itself: `host`, `content-length`, `transfer-encoding`, `connection`, `upgrade`, `te`, `trailer`, `expect`, `keep-alive`. A client-supplied `host` would otherwise let a shared front end route the injected secret to a virtual host the slot never authorised. Then, if the slot has a secret: `notes-llm` + `anthropic` → `x-api-key: <secret>`; every other slot → `authorization: Bearer <secret>`. If the slot has no secret: `notes-llm` with provider `ollama` or `openai-compatible` proceeds unauthenticated (local endpoints); `openai`/`anthropic`/`notes-acp` → `credential_missing`. The request URL's origin must equal `slot.origin` or the call fails `origin_mismatch` before any network I/O. Redirects are never followed.

Request ids are chosen by the client and must be unique per connection while the request is in flight; the server does not check this, and a reused live id makes the later request's abort handle overwrite the earlier one's. Core uses a random UUID per request.

`ws.close` answers `ok`, aborts the relay's reader task at once and reports `ws.closed` with the client's own code and reason, without waiting for the peer to echo its Close frame. Waiting would let an unresponsive peer pin one reader task per open/close cycle. The `ws.open` handshake is bounded by the same 15-minute per-request timeout as `http.fetch`.

Limits: request body 16 MiB, response body 64 MiB, per-request timeout 15 min, 16 connections, 32 in-flight requests per connection, 8 open ws streams per connection.

Keyring entries (service string, user `shorthand`; Windows target `Shorthand/shorthand@<service>` with `persistence=local`):

| slot                   | service                         |
| ---------------------- | ------------------------------- |
| Handy post-process key | `post-process/<provider_id>`    |
| `notes-llm`            | `notes-llm/<provider>/<origin>` |
| `notes-acp`            | `notes-acp/<vaultId>/<origin>`  |

---

Plain `http` is accepted for any host. The app does not decide whether a secret may travel in cleartext; the user does, by the origin they configure. Some corporate setups route provider calls through an unencrypted local proxy, often reached by a DNS name rather than a loopback address, so a loopback-only rule would not even identify it. The slot's origin is still the only place the secret can go.

## Local transport and security

The request socket trusts the same-user boundary, not process identity. On Windows the listener is a named pipe created with the same protected DACL as follow-stream, granting access only to the current user's SID. On Unix the socket file is created mode `0600` inside the config directory and each peer's effective user ID is checked against the app's own on accept. Any process running as the same user can therefore connect, read the credential status, and make authenticated requests through the proxy; no process, including the app's own clients, can read a stored secret back out. The listener is process-wide and unconditional: it starts at startup and stays up for the life of the process. The discovery file is written after the listener is up and removed on a clean stop; a stale discovery file from an unclean shutdown points at a path nothing is listening on, and a client should treat a refused connection as "app not running".

## Limits

| Limit                              | Value  | Error                                                |
| ---------------------------------- | ------ | ---------------------------------------------------- |
| NDJSON line                        | 32 MiB | `too_large`, then the connection is closed           |
| Request body (`http.fetch`)        | 16 MiB | `too_large`                                          |
| Response body (`http.fetch`)       | 64 MiB | `too_large`                                          |
| Per-request timeout                | 15 min | `timeout`                                            |
| Concurrent connections             | 16     | `hello`, then `limit`, then the connection is closed |
| In-flight requests per connection  | 32     | `limit`                                              |
| Open `ws.*` streams per connection | 8      | `limit`                                              |

## Versioning

`protocol` in `hello` and in the discovery file is `1`. It changes only for incompatible framing (the NDJSON envelope, the `id`/`ok`/`result`/`error` shape, or the `t` discriminator). Everything else is additive: a new method or a new optional field is advertised through `capabilities` in `hello`, and a client that does not recognise a capability ignores it. Clients must ignore fields they do not recognise, and must not depend on a method the server did not advertise. Follow-stream once shipped a field addition without a version bump that silently dropped every event downstream; the rule above exists so the request socket cannot repeat that.
