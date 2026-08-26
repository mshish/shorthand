# Shorthand as the umbrella application

Status: design proposed, not approved. No implementation started.

## The goal

One installed thing called Shorthand that gets a user from nothing to a
working capture pipeline: the transcription app itself, `shorthand-core`
for enhancement and sinks, the credentials those sinks need, and — if the
user wants it — the Obsidian plugin in the vault they choose.

Today that journey is four separate acts. The user installs Shorthand,
separately installs `shorthand-config` to complete a Google OAuth flow,
separately obtains the Obsidian plugin, and separately makes sure the two
halves agree on where the binary lives. Each step is documented; none of
them is discoverable from inside the app.

## What already exists

This design is mostly relocation, not invention. The parts are built.

- **`follow_stream/`** (`src-tauri/src/follow_stream/`, `FOLLOW_STREAM.md`)
  is a working fork-only feature: NDJSON over a per-user local socket,
  versioned, off by default, its own module, minimal touch points into
  shared files. `AGENTS.md:22-44` names it the model for how fork-only
  work is shaped. This design follows that shape.
- **`shorthand-config` already compiles core into a bundled sidecar**
  (`scripts/compile-core.sh`) and already has a working Tauri shell spawn
  path (`shorthand-config/src-tauri/src/sidecar.rs::spawn_core`, currently
  unused). The mechanism this design needs is proven; it moves — but see
  Phase 1 for what does not move cleanly.
- **`shorthand-config` already performs the OAuth flow and writes
  credentials** (`src-tauri/src/google/login_flow.rs`,
  `credentials.rs::write_credentials`) to
  `shorthand_config_directory()/google-credentials.json`, mirroring core's
  path logic. Its React surface (`src/ConnectGoogle.tsx`) talks to Rust
  only through `invoke()`, so it ports with little rework.
- **Core reads, never writes.** `src/config.ts:51-60` resolves the config
  directory; `src/google/file-token-provider.ts` reads the credentials
  file. Core needs no change to consume what the app would write.

## Decisions

### 1. The app is the umbrella

Rejected alternative: grow `shorthand-config` into the manager instead,
leaving the fork untouched.

The concern that drove that alternative was that an installer/manager can
never become an upstream PR, which appears to violate `AGENTS.md`'s
boundary rule. That conflates two properties. What the fork must protect
is **mergeability** — `git merge upstream/main` staying clean. That is
delivered by the module shape (own directory, few touch points), and a
well-bounded module merges cleanly whether or not it is liftable.
Liftability is a property `follow_stream` happens to also have.

Upstream Handy has no `src-tauri/src/integrations/`, so it will never
produce a conflicting hunk there.

The residual merge cost is real and accepted: a small number of one-line
hooks in shared files, plus `externalBin`/`resources` entries in
`tauri.conf.json`, which is a file upstream churns.

### 2. Fork-only strings follow the branding transform

New settings strings do not enter `src/i18n/locales/*/translation.json`.
The 24 catalogues stay byte-identical to upstream because they are what
upstream churns most, and because an upstream string containing "Handy"
must arrive renameable rather than pre-rewritten.

Verified: `FORK_ONLY_STRINGS` in `src/shorthand/branding.ts:32` is a
general mechanism for arbitrary new keys, not merely product-name
substitution. It is a `Record<string, string>` written into the catalogue
by `setByPath` *after* substitution runs (`branding.ts:289-291`), so
fork-only strings are authoritative and immune to the rename. It already
carries far more than the product name — whole redesigned sections
(`settings.modes.*`, `settings.aiCleanup.*`) live there today.

Two consequences, both already documented at `branding.ts:27-30`: these
strings are **English only** by design, with i18next's `fallbackLng: "en"`
rendering them in every locale; and they never reach the locale files, so
`check:translations` key-parity checking cannot see them. `bun run
check:branding` remains the guard.

#### 2a. Fork-only strings must become translatable

The English-only property is a deliberate trade in the current design, but
it is **not** an acceptable end state: Shorthand is intended to be fully
internationalised eventually, fork-only strings included. `FORK_ONLY_STRINGS`
has no locale dimension whatsoever — it is a flat `Record<string, string>`
— so it cannot express a translation, and a TypeScript object is the wrong
artifact to hand a translator regardless.

This work is the right moment to fix that, because it is the moment the
fork-only string count jumps. The object already holds ~80 keys; an
installer and its onboarding surface would add many more, and migrating
hundreds later is worse than migrating eighty now.

Proposed: introduce fork-only catalogues at `src/shorthand/locales/<lang>.json`
and migrate `FORK_ONLY_STRINGS` into `en.json` unchanged. The Vite plugin
merges them at exactly the point it merges the object today — *after* brand
substitution, preserving the ordering `branding.ts:15-17` depends on for
fork-only strings to be immune to the rename. Adding `de.json` later is
then purely additive, and translators get ordinary i18next catalogue files
that standard translation tooling understands.

Boundaries that do not change: fork-only catalogues live under
`src/shorthand/`, never `src/i18n/locales/`, so upstream's 24 files stay
byte-identical and `check:translations` parity is untouched.

This needs one new gate — a parity check across the *fork-only* catalogues,
mirroring what `check:translations` does for upstream's. Without it, a
`de.json` missing a key fails silently to English.

Scope note: the migration is mechanical and independent of the three
phases below. It could ship before them, and probably should.

### 3. The app bundles core as a sidecar, for the Obsidian-free path

The app's bundled core is not redundant with the plugin's. The plugin
bundles core at *build* time and runs it in-process; it never spawns a
core binary. The app's sidecar is therefore what enables capture with **no
vault at all** — Google Docs sink, or plain markdown on disk.

Consequence: two independently-versioned copies of core can exist on one
machine. The app does not attempt to control the plugin's copy. It reads
the installed plugin's `manifest.json` version and **warns on
incompatibility** rather than assuming or enforcing.

### 4. Reverse the spawn topology (open — see Open questions)

Current topology runs opposite to "app manages core". Core's
`StreamClient` spawns `shorthand.exe`; the running app is a *server*
(`hub.rs`/`server.rs`) and `shorthand.exe --follow-stream` is a
short-lived *client* that connects to it, which is why it supports 8
concurrent followers.

Correction to an earlier draft of this section: the follower does **not**
reach the running app through `tauri_plugin_single_instance`. `AGENTS.md`
is explicit that `--follow-stream`'s "follower attaches over a per-user
local socket rather than `tauri_plugin_single_instance`" — unlike
`--toggle-transcription` and the other remote-control flags, which do use
the single-instance relay. So the socket, not the single-instance guard,
is what makes the loop terminate.

Naively, app-spawns-core-sidecar produces a loop: app → core → app in
client mode → back to the app's socket. It terminates, but it spawns a
redundant process per follower and is convoluted.

Preferred: an **additive** core entry point that connects to the socket
directly instead of shelling out. Additive exports carry no plugin
follow-through obligation per `shorthand-core/AGENTS.md`.

Hazard: `CLAUDE.md` records that this protocol "has already shipped a
field addition without a version bump that silently dropped every event
downstream." Any change here reads `FOLLOW_STREAM.md` first and bumps
`protocol` deliberately.

### 5. Obsidian plugin: place files, do not automate enabling

Verified against primary sources:

- **No install API exists.** Obsidian's URI reference documents only
  `open`, `new`, `daily`, `unique`, `search`, `choose-vault`.
  `obsidian://show-plugin?id=<id>` is real and first-party — Obsidian's own
  help vault uses it — but undocumented, and it only *opens* the plugin's
  entry in the community browser. The user still clicks Install.
- **Manual placement is officially sanctioned.** help.obsidian.md states
  plugins "can be installed manually at this location"
  (`<vault>/<config>/plugins/<id>/`).
- **Community plugins do not auto-update** — "For security purposes,
  community plugins don't update automatically." There is therefore **no
  background writer** to clobber app-placed files. This removes the
  main objection to placing files directly.
- **The config folder is user-overridable.** `.obsidian` must not be
  hardcoded.
- **Developer policies bind plugins, not external apps**, and explicitly
  do not apply to plugins installed outside the Obsidian directory.
  Separately, plugins "must not install or update themselves or their
  dependencies" — which is why the plugin cannot be the umbrella.

Therefore the app places `manifest.json`, `main.js` and `styles.css` into
the user-chosen vault, sourced from the published GitHub release (the same
assets Obsidian itself would download, so versions match the store).

The app **does not** write `<vault>/<config>/community-plugins.json`. That
file's format is undocumented and community-observed only, and Obsidian
owns it at runtime. Enabling stays a user action, as the plugin README
already documents.

`obsidian://show-plugin?id=shorthand` is offered as a secondary "install
from the store instead" affordance for users who prefer it.

## Architecture

Two new directories, both absent from upstream:

```
src-tauri/src/integrations/     # Rust: sidecar, credentials, vault I/O
src/shorthand/settings/integrations/   # React: the settings surface
```

Registration is cheaper than an earlier draft of this design assumed,
because the settings sidebar is **already fork-owned**. `SHORTHAND_SECTIONS`
in `src/shorthand/sections.ts:38` is the real registration point; it is
spread into upstream's `SECTIONS_CONFIG` in `src/components/Sidebar.tsx`,
and its header states it is "kept in this file so registering or
reordering a section never conflicts with upstream's edits to that
object." Exporting from `src/components/settings/index.ts` does **not**
put a section in the sidebar and is not the path to use.

Fork-only files — no merge cost:

| File | Change |
| --- | --- |
| `src/shorthand/sections.ts` | register the integrations section |
| `src/shorthand/branding.ts` | add keys to `FORK_ONLY_STRINGS` |
| `src/shorthand/settings/integrations/` | the settings surface (new) |
| `src-tauri/src/integrations/` | sidecar, credentials, vault I/O (new) |

Shared files — keep to exactly this list:

| File | Change |
| --- | --- |
| `src-tauri/src/lib.rs` | register the module and its `#[tauri::command]`s |
| `src-tauri/src/settings.rs` | settings struct fields and defaults |
| `src-tauri/tauri.conf.json` | `externalBin` for the sidecar, `resources` if any |

Generated and test surfaces that must be updated, not authored:

- `src/bindings.ts` is generated by tauri-specta; new commands regenerate it.
- `tests/settings-coverage.spec.ts` fails if any leaf setting control stops
  being reachable (`sections.ts:30-32`). New controls need coverage there.

`settingsStore.ts` / `useSettings.ts` follow the existing State Flow
(Zustand → Tauri command → Rust → `tauri-plugin-store`) unchanged.

## Phases

These are independent capabilities sharing one module boundary. Each gets
its own implementation plan; none should be built before Phase 0.

**Phase 0 — unblock bundling.** Not part of this feature, but a hard
prerequisite for all of it. `SIGNING_AND_UPDATES.md:88-90`: the inherited
`signCommand` "must be removed or replaced before a bundled build will
succeed… removing it is not [optional]." The updater still points at
`cjpais/Handy`, where accepting an update replaces Shorthand with Handy
(`:17-19`). No minisign keypair exists. An installer that ships a sidecar
cannot exist until bundled builds work.

**Phase 0b — fork-only catalogues.** Migrate `FORK_ONLY_STRINGS` to
`src/shorthand/locales/en.json`, teach the Vite plugin to merge catalogues
at the same point, add the fork-only parity check. Mechanical, independent
of everything below, and cheapest done before the string count grows. See
Decision 2a.

**Phase 1 — core sidecar.** Relocate `compile-core.sh` into the app's
build, add `externalBin`, add `integrations/core_sidecar.rs`, resolve
Decision 4.

Two gaps in the donor script that relocation does not fix, and that this
phase must close before it can ship (flagged in review, verify both
against `shorthand-config` before planning):

- **Target coverage.** The script reportedly has no `aarch64-unknown-linux-gnu`
  case and always emits an x64 binary on Windows. Shorthand's release matrix
  includes ARM64 targets, which would build with a wrong or missing sidecar.
  Tauri's `externalBin` also expects target-triple-suffixed filenames, which
  the donor script was not written to produce.
- **Private-repo checkout.** `shorthand-core` is private and consumed at a
  pinned tag. The donor workflow obtains it with a deploy key. This design
  specifies neither the checkout, the pin, credential provisioning, nor what
  happens for builds that cannot see repository secrets. A build that
  silently falls back to a stale or absent core is worse than one that fails.

**Phase 2 — credentials and sink settings.** Port `ConnectGoogle.tsx` and
the Rust OAuth flow. Surface the Google Docs sink that core already
supports and nothing currently exposes.

**Phase 3 — Obsidian plugin install.** Vault picker, release download,
file placement, version detection and warning.

## Sequencing hazard: secrets and publication

The fork-migration plan
(`docs/superpowers/plans/2026-08-24-github-fork-migration-and-readme.md`)
puts a secret scan at Task 6a, "the last preventable moment", and
`gh repo fork` at Task 7, which is irreversible.

Phase 2 introduces OAuth client credentials into this repository. Config
injects them at build time via `env!()` and does not commit them. The app
must do the same. **Either complete Phase 2 after publication, or verify
the injection pattern before the Task 6a scan.** Exposure is not
reversible.

Related: the fork-migration plan explicitly rejects the squashed-commit
approach (`:28`) — `git merge-base main upstream/main` is a real shared
commit, and a squash "would destroy [attribution and bisectability] for no
benefit" while risking the merge workflow this design depends on. Nothing
here changes that.

Three defects in that plan surfaced during review of this design. They
belong to that plan, not this one, but this design's Phase 2 depends on
its secret gate actually working, so they are recorded here:

1. **The secret scan targets the wrong clone.** Task 6a runs `git log --all`
   in the working copy while declaring the fresh bare clone the source of
   truth for server refs. Branch history that exists only on the server
   would go public unscanned. Scan the bare clone.
2. **The detector is home-grown and incomplete.** Its patterns miss
   `github_pat_…`, Google API keys, and `client_secret` / `refresh_token`
   assignments — precisely the shapes Phase 2 introduces. Use a maintained
   full-history scanner, keeping manual review of its findings.
3. **Rollback cannot deliver the retry it promises.** Renaming a failed fork
   aside leaves the account owning a fork in Handy's network, and GitHub
   will not create a second one. Rollback must reuse, repair, or detach that
   fork before Task 5 can be retried.

## Correctness constraint: one writer per credentials file

Core documents one writer per credentials file. If the app writes
`google-credentials.json`, `shorthand-config` must stop writing it, or the
two must never be installed together. This is a correctness question, not
a product one, and it needs an explicit answer before Phase 2 ships.

`shorthand-config`'s other role — running core's Google conformance suite
against its Rust writer — is unaffected and can continue regardless.

## Open questions

1. **Decision 4**: additive core entry point, or accept the spawn loop?
2. **Config folder discovery**: `.obsidian` is overridable, and the
   override's storage location was not verified. Default to `.obsidian`,
   detect absence, let the user point at the folder explicitly?
3. **`shorthand-config`'s fate** given the one-writer constraint.
4. **`src/bindings.ts` has no headless export path**, so new Tauri
   commands cannot have their bindings verified in CI. Accept, or fix?
5. ~~English-only settings strings.~~ **Resolved** by Decision 2a: fork-only
   catalogues, migrated as Phase 0b. Shorthand is to be fully
   internationalised eventually, so English-only is a waypoint, not the
   destination.
6. **Sidecar vs. binary discovery.** Core and the plugin locate
   `shorthand.exe` through "Shorthand's install locations". Confirm that
   bundling core *inside* the app's resources does not move anything those
   two rely on finding.

## Out of scope

- Publishing core to npm. The sidecar is compiled, not installed.
- Changing how the plugin consumes core (pinned GitHub tag stays).
- Making the plugin install or manage `shorthand.exe` — barred by
  Obsidian's developer policies.
- Auto-enabling the plugin in a vault.

## Testing gates

Existing gates apply unchanged: `bun run lint`, `bun run format`,
`cargo fmt`, `cargo clippy`, `bun run check:branding`,
`tests/settings-coverage.spec.ts`, and the i18next-only ESLint rule.

New work adds a test pinning the plugin asset list (mirroring the plugin's
own `deliver-to-vault` test at `esbuild.config.mjs:37-55`) so a fourth
asset cannot silently break installs.

Note `bun run check:translations` will not cover any string this design
adds, by construction — see Decision 2. `check:branding` is the only
guard on that surface.
