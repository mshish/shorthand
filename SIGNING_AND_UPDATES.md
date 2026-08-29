# Signing and updates

Fork-only. Update signing is set up and the fork owns its own updater; code
signing is not, on either platform.

## Where things stand

| Thing                        | State                                                      | Consequence                                                    |
| ---------------------------- | ---------------------------------------------------------- | -------------------------------------------------------------- |
| `plugins.updater.endpoints`  | `mshish/shorthand` releases                                | the updater offers Shorthand's own releases                    |
| `plugins.updater.pubkey`     | this fork's minisign public key                            | only this fork can produce updates the app will accept         |
| `update_checks_enabled`      | defaults `true`                                            | the app checks on its own                                      |
| `bundle.windows.signCommand` | removed                                                    | bundling succeeds; installers are unsigned, SmartScreen warns  |
| macOS `signingIdentity`      | `"-"` (ad-hoc)                                             | runs on the machine that built it, nowhere else                |
| GitHub Actions               | enabled; all nine inherited workflows active               | builds run on pull requests and pushes to main                 |
| Repository visibility        | public                                                     | release assets are reachable by the updater without auth       |

**The update-hijack risk this file used to open with is gone.** The endpoint and
the public key both belong to this fork now, so an update prompt offers
Shorthand rather than upstream Handy. What is left is ordinary code signing,
which costs money and warns on first run without it.

## What is actually still missing

- **Windows Authenticode.** Installers are unsigned. SmartScreen warns once;
  "More info → Run anyway" works.
- **macOS Developer ID and notarization.** This is why macOS is out of
  `release.yml`'s matrix: the build itself now succeeds, but an unsigned `.app`
  is quarantined on download and needs `xattr -dr com.apple.quarantine` before
  it will open. macOS is also out of the automatic build matrix in
  `main-build.yml`, separately, on runner cost.
- **An exercised update.** The updater has never been run end to end against a
  real release. Do not assume it works until an install has upgraded itself.

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

## Where releases are hosted, and why that question is settled

Tauri's updater fetches the endpoint over plain HTTPS, so the assets have to be
reachable without authentication. That used to be the hard constraint here: the
repository was private, its release assets needed an authenticated request, and
this file weighed a separate public releases repo, object storage, and making
the source public against each other.

**The repository is public now, so option three was taken.** The endpoint is
`https://github.com/mshish/shorthand/releases/latest/download/latest.json` and
it resolves for anyone. Nothing further is needed.

Recorded because the alternatives are worth knowing if that ever reverses: a
separate public releases repo holding only `latest.json` and the artifacts is
what most private-source projects do, and object storage (R2, S3) is the option
with no GitHub involvement at all. Custom request headers carrying a token are
the one option to refuse outright — the token ships inside the app binary,
readable by anyone who downloads it.

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

| Option                | Rough cost     | Notes                                                                                                                                                            |
| --------------------- | -------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Unsigned              | free           | SmartScreen warns on first run; "More info → Run anyway" works. Perfectly fine for personal use.                                                                 |
| Azure Trusted Signing | ~$10/month     | What upstream uses. Needs an identity check; individuals were eligible at launch but availability has moved around — verify current terms before planning on it. |
| OV certificate        | ~$200–400/year | Since the 2023 CA/Browser Forum rules the key must live on a hardware token or cloud HSM, which makes CI signing genuinely awkward.                              |
| EV certificate        | ~$400+/year    | Same hardware constraint, but carries SmartScreen reputation immediately rather than earning it.                                                                 |

For a fork you run yourself, unsigned is the honest default. Revisit if you ever
hand a build to someone else.

## macOS

Only relevant if you build for macOS at all. Notarization requires the Apple
Developer Program ($99/year); the release workflow expects `APPLE_CERTIFICATE`,
`APPLE_ID`, `APPLE_PASSWORD` and `APPLE_TEAM_ID`. Ad-hoc signing (`"-"`, the
current setting) produces something that runs on the machine that built it and
nowhere else.

## CI, and the nine inherited workflows

Actions is **on**, and all nine workflow files inherited from upstream are
active. The per-workflow disabling this section used to plan for never happened;
the workflows were adapted instead.

What runs automatically today:

- `main-build.yml` — Windows and Linux on every pull request and every push to
  `main`. No macOS: cost on the automatic path, and no Developer ID on the
  release path. `sign-updates` is on for `main` and off for pull requests,
  because GitHub withholds secrets from a fork's pull request and the build
  fails hard on an empty `TAURI_SIGNING_PRIVATE_KEY`.
- `test.yml` — `cargo test`, ubuntu-24.04 only.
- `code-quality.yml`, `playwright.yml`, `nix-check.yml` — frontend and Nix.

`build-test.yml` and `pr-test-build.yml` remain `workflow_dispatch` and are the
only way to build macOS on demand.

Because the repository is public, Actions minutes are free, so the macOS
exclusion in `main-build.yml` is now about runner time rather than billing.

## Build commands, for reference

```sh
# Run the app in debug, with hot reload. Also regenerates src/bindings.ts,
# which nothing else does — cargo build and cargo test never reach the
# specta export.
bun run tauri dev

# Debug binary, without launching:  src-tauri/target/debug/shorthand.exe
bun run tauri build --debug --no-bundle

# Release binary, unsigned:         src-tauri/target/release/shorthand.exe
# --no-bundle skips installer creation. Bundling works now that signCommand
# is gone; this is just the faster path when you only want the binary.
bun run tauri build --no-bundle
```

A full `bun run tauri build` (with bundling) succeeds — `signCommand` was
removed, so nothing tries to authenticate to an account this fork has no access
to. The installer it produces is unsigned.

`beforeBuildCommand` is `bun run build`, so every one of these runs the
frontend build first — meaning the branding transform in
`src/shorthand/branding.ts` applies, and its review warnings print.

## If you pick up code signing

1. Windows: decide between staying unsigned and Azure Trusted Signing (see the
   table above). Unsigned is the honest default for a fork you run yourself.
2. macOS: an Apple Developer Program membership ($99/year) is the entry price.
   With a Developer ID in hand, add the two macOS rows back to `release.yml`'s
   matrix and set `sign-binaries: true` there.
3. Either way, cut a release and verify an actual install upgrades itself. The
   updater path is configured but has never been exercised.

Keep the minisign private key backed up off this machine. Losing it means
existing installs can never be updated again — they reject anything signed by a
different key, and the only remedy is a manual reinstall.
