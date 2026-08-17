# Signing and updates

Fork-only. Nothing here is set up yet — this is the briefing for the session
that does it.

## Where things stand

| Thing | State | Consequence |
| --- | --- | --- |
| `plugins.updater.endpoints` | points at `cjpais/Handy`'s `latest.json` | Shorthand offers to install upstream Handy over itself |
| `plugins.updater.pubkey` | upstream's minisign public key | only upstream can produce updates this build will accept |
| `update_checks_enabled` | defaults `true` | the offer appears unprompted |
| `bundle.windows.signCommand` | `trusted-signing-cli … -a CJ-Signing -c cjpais-dev` | bundling fails; this fork cannot authenticate to that account |
| macOS `signingIdentity` | `"-"` (ad-hoc) | fine locally, not distributable |
| GitHub Actions | disabled at the repository level | nothing runs, nothing fails, no minutes burned |

**The live risk is the first row.** Until it changes, decline any update prompt.
Accepting one replaces Shorthand with Handy.

A one-line interim mitigation, if the prompt becomes annoying before this work
happens: flip the fork's default for `update_checks_enabled` to `false` in
`src-tauri/src/settings.rs`, the same way `paste_method` was defaulted. It
doesn't fix anything, it just stops the app asking.

## Two different things both called "signing"

Worth separating before choosing anything, because they have separate costs and
neither implies the other.

**Update signing (minisign).** Tauri's updater verifies that an update artifact
was produced by whoever holds the private key matching `pubkey`. Free, entirely
self-managed, and required for the updater to work at all. This is the one that
matters for auto-updates.

**Code signing (Authenticode on Windows, Developer ID on macOS).** Tells the
operating system the binary comes from a verified identity. Costs money, needs
an identity check, and is what makes SmartScreen and Gatekeeper stop warning.
Entirely optional if you're the only user.

You can have update signing without code signing. The result auto-updates fine
and shows a SmartScreen warning on first run.

## The gotcha that shapes everything: the repo is private

Tauri's updater fetches the endpoint over plain HTTPS. A private GitHub repo's
release assets need an authenticated request, so `mshish/shorthand`'s releases
are not reachable by the updater as-is.

Real options, in rough order of sanity:

1. **A separate public releases repo** (e.g. `mshish/shorthand-releases`)
   holding only `latest.json` and the artifacts. Source stays private. This is
   what most private-source projects do.
2. **Object storage** — Cloudflare R2 or S3 with a public bucket. No GitHub
   involvement, trivially cheap at this volume, and you control cache headers.
3. **Make `mshish/shorthand` public.** Simplest, and the fork will eventually
   contribute upstream anyway — but it publishes the divergence.
4. **Custom headers with a token.** Tauri v2's updater supports request
   headers, so a PAT could authenticate. Do not do this: the token ships inside
   the app binary, readable by anyone who downloads it.

Decide this first. It determines the endpoint URL, which determines everything
downstream.

## Update signing, concretely

Generate a keypair:

```sh
bun run tauri signer generate -- -w "$HOME/.tauri/shorthand.key"
```

That writes the private key to that path and prints the public key. Then:

- Put the **public** key in `src-tauri/tauri.conf.json` at
  `plugins.updater.pubkey`, replacing upstream's.
- Point `plugins.updater.endpoints` at whichever channel you chose above.
- Supply the **private** key at build time via `TAURI_SIGNING_PRIVATE_KEY` and
  `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`.

Keep the private key out of the repo. Losing it means existing installs can
never be updated again — they will reject anything signed by a different key,
and the only remedy is a manual reinstall. Back it up somewhere you'd still
have after losing this machine.

## Windows code signing options

`bundle.windows.signCommand` must be removed or replaced before a bundled build
will succeed. Replacing it is optional; removing it is not.

| Option | Rough cost | Notes |
| --- | --- | --- |
| Unsigned | free | SmartScreen warns on first run; "More info → Run anyway" works. Perfectly fine for personal use. |
| Azure Trusted Signing | ~$10/month | What upstream uses. Needs an identity check; individuals were eligible at launch but availability has moved around — verify current terms before planning on it. |
| OV certificate | ~$200–400/year | Since the 2023 CA/Browser Forum rules the key must live on a hardware token or cloud HSM, which makes CI signing genuinely awkward. |
| EV certificate | ~$400+/year | Same hardware constraint, but carries SmartScreen reputation immediately rather than earning it. |

For a fork you run yourself, unsigned is the honest default. Revisit if you ever
hand a build to someone else.

## macOS

Only relevant if you build for macOS at all. Notarization requires the Apple
Developer Program ($99/year); the release workflow expects `APPLE_CERTIFICATE`,
`APPLE_ID`, `APPLE_PASSWORD` and `APPLE_TEAM_ID`. Ad-hoc signing (`"-"`, the
current setting) produces something that runs on the machine that built it and
nowhere else.

## Re-enabling CI without inheriting nine upstream workflows

Actions is disabled as a **repository setting**, deliberately: upstream ships
nine workflow files, and deleting them would be a permanent diff that conflicts
on every merge. The setting achieves the same thing with a zero-line diff.

Turning Actions back on to get a release workflow would also start all nine.
The way out is per-workflow disabling, which is also a setting rather than a
file change:

```sh
# Turn Actions back on for the repo
gh api -X PUT repos/mshish/shorthand/actions/permissions -F enabled=true

# List the inherited workflows and their ids
gh api repos/mshish/shorthand/actions/workflows -q '.workflows[] | "\(.id)\t\(.name)\t\(.path)"'

# Disable each upstream one individually
gh api -X PUT repos/mshish/shorthand/actions/workflows/<id>/disable
```

Then add a Shorthand-only release workflow as a new file, which conflicts with
nothing because upstream has no file at that path.

The alternative — keep Actions off entirely and cut releases locally with
`bun run tauri build` plus `gh release create` — is less machinery and works
fine for a one-person project. Worth considering before building CI you then
have to maintain across merges.

## Build commands, for reference

```sh
# Run the app in debug, with hot reload. Also regenerates src/bindings.ts,
# which nothing else does — cargo build and cargo test never reach the
# specta export.
bun run tauri dev

# Debug binary, without launching:  src-tauri/target/debug/shorthand.exe
bun run tauri build --debug --no-bundle

# Release binary, unsigned:         src-tauri/target/release/shorthand.exe
# --no-bundle skips installer creation, which is where signCommand runs,
# so this succeeds today despite the broken signing config.
bun run tauri build --no-bundle
```

A full `bun run tauri build` (with bundling) will fail until `signCommand` is
dealt with.

`beforeBuildCommand` is `bun run build`, so every one of these runs the
frontend build first — meaning the branding transform in
`src/shorthand/branding.ts` applies, and its review warnings print.

## Order of work, when you pick this up

1. Choose the distribution channel — public releases repo, object storage, or
   make the source public. Everything else depends on it.
2. Generate the minisign keypair; back the private key up off this machine.
3. Update `pubkey` and `endpoints` in `src-tauri/tauri.conf.json`.
4. Remove or replace `bundle.windows.signCommand`.
5. Decide local releases versus CI. If CI, re-enable Actions and disable the
   nine inherited workflows individually before pushing anything.
6. Cut a release and verify an actual install upgrades itself — an updater that
   has never been exercised end to end should not be assumed to work.
