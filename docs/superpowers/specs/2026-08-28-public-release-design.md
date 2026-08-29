# Going public: core, plugin, app — design

Status: design proposed, awaiting approval. No implementation started.

## The goal

Get three of the four Shorthand repositories public and produce a Windows
installer a friend can download, install, and receive updates for — without
an Obsidian directory listing and without a code-signing certificate.

Success is a friend on Windows who can: install Shorthand, drop the plugin
into a vault by hand, capture a meeting, and later accept an in-app update
that actually installs.

Explicitly **not** in scope: the Obsidian community directory submission,
Authenticode code signing, and the umbrella-installer work
(`2026-08-25-shorthand-umbrella-design.md`). Those come after, and each
depends on this landing first.

## What is true today

Verified 2026-08-28 against the live repositories, not assumed.

| Fact | Value |
| --- | --- |
| Repository visibility | all four private |
| `mshish/shorthand` | `isFork: false` — never forked from `cjpais/Handy` |
| Merge base with upstream | `549cbde`, a real shared commit |
| Divergence | **132 ahead, 25 behind** (the fork-migration plan's "95/14" is stale) |
| Upstream head | `c6fa60d`, tag `v0.9.6` |
| App version | `0.9.5` — Handy's number, in `tauri.conf.json` |
| App identifier | `com.mshish.shorthand`, productName `Shorthand` |
| Updater endpoint | `cjpais/Handy` releases — **accepting an update installs Handy** |
| Updater pubkey | upstream's minisign key; no Shorthand keypair exists |
| `signCommand` | `trusted-signing-cli … -a CJ-Signing -c cjpais-dev`, blocks bundling |
| Repo secrets | none configured |
| GitHub Actions | disabled at the repository level |
| Tags in the app repo | 65, including upstream's `v0.9.5` and `v0.9.6` |
| Plugin core pin | `github:mshish/shorthand-core#0.13.0` |
| `shorthand-core` | `0.13.0` tagged and pushed; 31 tags |
| Plugin id / name | `shorthand` / `Shorthand` — both already correct |
| Plugin version | `0.1.0`, tagged |

## Decisions

### 1. Mirror the full history into the fork; do not squash

A squash was offered and declined. It buys nothing here: `git merge-base main
upstream/main` resolves to a real shared commit, so the fork's history is
already a superset of upstream's rather than unrelated history that needs
flattening. Squashing would destroy commit-level attribution and bisect
across 132 commits to make the merge work no better than it already does.

All branches and all 65 tags mirror across.

### 2. No Authenticode. Minisign only

These are two different things both called "signing" and only one of them
is load-bearing here.

**Minisign** is what the Tauri updater verifies. It is free, self-managed,
and auto-updates do not function without it. Shorthand needs its own keypair.

**Authenticode** is what stops SmartScreen warning. It costs money or an
approval process, and its absence degrades first-run experience without
breaking anything. Deferred.

Consequence for friends: Windows SmartScreen warns once on install, and they
click through "More info → Run anyway". That is the accepted cost.

The inherited `signCommand` is removed outright rather than replaced.
`SIGNING_AND_UPDATES.md:88-90` records that a bundled build cannot succeed
while it is present, and this fork cannot authenticate to that Azure account.

#### 2a. `sign-binaries` conflates the two and must be split

`.github/workflows/build.yml:527-538` gates Apple signing, Azure/Authenticode
signing **and** `TAURI_SIGNING_PRIVATE_KEY` behind a single `sign-binaries`
input. Setting it `false` to skip Authenticode would also disable minisign,
which silently produces a release whose updater artifacts nothing will accept.

The input splits into `sign-binaries` (platform code signing, `false` for now)
and `sign-updates` (minisign, `true`). This is the single change on which
auto-updates depend, and it is reviewed alone.

### 3. Merge upstream after the fork, in the open

Merging first, while still private, was offered on the grounds that conflict
resolution across 25 commits and 273 diverged files is where mistakes happen,
and that mistakes made privately stay private. Declined in favour of doing
the merge in the open after forking.

Recorded so the trade is visible rather than forgotten: **conflict resolution
for this merge will be public history.** A botched resolution, a bad revert,
or a temporarily broken tree is visible to anyone watching. The compensating
control is that the merge lands through a pull request with CI green before it
reaches `main`, not by pushing to `main` directly.

Being a GitHub fork changes nothing about `git merge upstream/main` mechanics —
that already works today against any readable URL. What forking unlocks is the
Sync-fork UI, compare-across-forks, and the PR-to-upstream flow.

### 4. All seven build targets, best-effort

`release.yml:46` already sets `fail-fast: false`, and the workflow creates a
**draft** release before the matrix runs. A failing macOS or ARM job therefore
does not abort the others; the draft collects whatever succeeded and is
published by hand after review.

No platform is dropped from the matrix. Windows x64 is the only target that
must succeed for this design to have met its goal.

One consequence needs verifying rather than assuming: seven parallel jobs each
write `latest.json` to the same release. Whether `tauri-action` merges platform
entries or clobbers the asset determines whether every platform but the last
one to finish silently sees no updates. **This is a gate, not a note** — see
Phase D.

### 5. Publish three repositories; `shorthand-config` stays private

`shorthand-core` must be public because the plugin resolves it as a `github:`
dependency and an anonymous `npm install` fails otherwise. The plugin and the
app must be public to be installable.

`shorthand-config` is required by none of that. It holds the Google OAuth flow
and injects client credentials at build time, which makes it the highest
secret-exposure risk of the four, and keeping it private costs nothing now.
Its fate is settled later, by the umbrella design's one-writer-per-credentials
question.

### 6. The plugin repository is renamed to `shorthand-obsidian-plugin`

Verified against Obsidian's published guidance: the constraints bind the
plugin **id** and **name** — neither should contain the word "Obsidian" —
and say nothing about the repository name. `id: "shorthand"` and
`name: "Shorthand"` are already correct and are not touched.

Renaming now is free. Renaming after a directory listing exists is not, which
is why it happens in this push rather than the next one. GitHub auto-redirects
the old URL until some other repository claims the vacated name.

The local working directory `D:/tools/obsidian-shorthand` and the workspace
`CLAUDE.md` map both name the old path. Both are updated, as the last task of
Phase A, so nothing is renamed out from under work in progress.

### 7. Shorthand takes its own version line with prefixed tags

The app reports `0.9.5` — Handy's number — and `release.yml` tags `v${version}`
read from `tauri.conf.json`. Since the mirror brings upstream's `v0.9.5` and
`v0.9.6` tags into the fork, **the release workflow as written would attempt a
tag that already exists.** This is a hard blocker, not a cosmetic one.

Shorthand resets to `0.1.0` and tags `shorthand-v0.1.0`. Upstream's `v0.9.x`
tags stay untouched and keep meaning "the Handy release we merged", which is
useful information in a fork. There is no namespace in which the two can
collide.

`release.yml`'s tag construction changes accordingly, and `asset-prefix`
changes from `handy` to `shorthand`.

## Execution model

### The loop

1. Claude writes a granular task brief from the plan — files, interfaces,
   acceptance test.
2. `Agent(subagent_type: "codex:codex-rescue")` implements it, write-capable.
   `--write` is the runtime's default; read-only is the opt-out.
3. Codex **commits after each task**. Bisect keeps working and a single
   rejected task is reverted rather than unpicked from its neighbours.
4. After a batch of tasks, one **Sonnet 5** reviewer subagent reviews
   `base..HEAD` with every brief in the batch as its rubric and the per-task
   commits visible, so it can attribute a defect to the change that caused it.
5. Claude adjudicates. `superpowers:receiving-code-review` applies: a
   questionable finding is verified, not implemented reflexively.
6. Codex applies accepted fixes on a resumed session.
7. Human gate at every phase boundary.

### Review batching

A review batch is the run of tasks between two gates that touch a coherent
surface, capped at **5 tasks or ~400 changed lines**, whichever comes first.
Batches never span a phase gate.

The cost is real and accepted: a defect in the first task of a batch is not
caught until the last has built on it. Two things keep it cheap — batching by
shared file or concern, so the reviewer sees a whole surface rather than a
slice, and the per-task commits, which make unwinding a revert.

**Reviewed alone, never batched**, because their blast radius exceeds their diff:

- the secret-scan tooling and its findings;
- minisign keypair generation and how the private key reaches CI;
- the updater `endpoint` and `pubkey` change.

Reviewers get read-only tools. A reviewer that can edit is not a second opinion.

### Codex session lifetime

**Resume (`--resume`)** for the next task in the same batch and same repository,
and for applying that batch's review fixes — Codex already holds the context of
what it built.

**Reset (`--fresh`)** on a new review batch; on **any change of repository**,
since Codex acts in whatever tree it is launched in and a carried session is a
live hazard across four repos; after any irreversible `gh` operation that
changes the repository's identity or remotes, because Codex's picture of the
world is then wrong; and whenever its output shows it working from a stale read.

Every brief names its repository explicitly.

### What Codex does not get

Codex gets code and documentation. The irreversible GitHub operations stay with
Claude, executed one at a time after explicit human confirmation:

- `gh repo fork`
- `git push --mirror`
- `gh repo rename`
- `gh release edit --draft=false`
- any visibility change

A write-capable agent that misreads "rename the repo aside" costs an afternoon.
One that misreads "fork it" is permanent.

Separately, **the secret scan is advisory to a human, not a gate an agent
clears.** Codex may run the scanner and format its findings. Deciding that a
hit is a false positive is exactly the failure mode that publishes a key, and
that decision is the user's.

## Phases

Each phase gets its own implementation plan in the repository it acts on.
No phase begins before its predecessor's gate passes.

### Phase A — publish core and the plugin

Acts on `shorthand-core` and `obsidian-shorthand`.

1. Install a maintained full-history secret scanner (gitleaks; neither it nor
   trufflehog is present on this machine) and run it against **bare clones** of
   both repositories, not the working copies. A working-copy scan cannot see
   branch history that exists only on the server.
2. Human reviews every finding.
3. Documentation truth pass on **core**, whose docs assert the opposite of
   what is about to be true in four places: `AGENTS.md:36` is headed "This
   repo is private, and pushing needs no permission" and `:38` premises a
   single-user private repository; `README.md:48` calls the package private
   and unpublished; `README.md:55` documents a bun workaround whose stated
   reason — "404s on a private repository" — stops holding the moment core
   is public. That last one is the shape the global working agreement warns
   about: a workaround that outlives its constraint. Establish whether the
   bun behaviour still bites a public repo before deleting or keeping it,
   and record the real reason either way.
4. Flip `shorthand-core` public.
5. Rename `obsidian-shorthand` → `shorthand-obsidian-plugin`; flip public.
6. Documentation truth pass on the plugin — this is Task 1 of the existing
   marketplace-submission plan, and only that task. `README.md:3` points at
   `https://github.com/cjpais/Shorthand`, which does not exist; the BRAT
   paragraph claims a token is needed because the repository is private;
   `AGENTS.md` premises a single-user private repository.
7. Cut plugin release `0.1.0` with `main.js`, `manifest.json`, `styles.css`
   attached, so manual install and BRAT both work.
8. Rename the local directory and update the workspace `CLAUDE.md` map.

**Gate:** a clean clone of the plugin, by an anonymous user, runs
`npm install && npm run build && npm test` green with `shorthand-core`
resolving to `0.13.0`.

### Phase B — fork the app

Acts on `shorthand-app`. This is the irreversible phase.

Reuses `2026-08-24-github-fork-migration-and-readme.md` with four corrections:

1. **Stale divergence counts** — the plan says 95 ahead / 14 behind; it is now
   132 / 25, and the branch and tag counts must be re-derived at run time
   rather than read from the plan's comments.
2. **The secret scan targets the wrong clone** — Task 6a runs `git log --all`
   in the working copy while declaring the bare clone the source of truth.
   Scan the bare clone.
3. **The detector is home-grown and incomplete** — its patterns miss
   `github_pat_…`, Google API keys, and `client_secret` / `refresh_token`
   assignments. Replace with gitleaks, keeping human review of findings.
4. **Rollback cannot deliver the retry it promises** — renaming a failed fork
   aside leaves the account owning a fork in Handy's network, and GitHub will
   not create a second one. Rollback must reuse, repair, or detach that fork
   before the fork step can be retried.

The plan's own ordering is otherwise sound and is kept: README fixes land while
still private, then rename aside, bare mirror clone, scan, fork, mirror-push,
rename into place.

**Gate:** `gh repo view mshish/shorthand` reports
`isFork: true, parent: cjpais/Handy, visibility: PUBLIC`, and a ref-by-ref
comparison of the bare clone against the fork's refs is empty.

### Phase C — merge upstream to v0.9.6

Acts on `shorthand-app`, in public, on a branch.

25 upstream commits into 132 commits of divergence across 273 files. Merged on
a branch, landed through a pull request, never pushed straight to `main`.

Actions are disabled at the repository level and must be enabled before CI can
gate anything — this is the first phase that needs them.

Prerequisite within this phase, from `2026-08-27-plan-c-system-audio-macos.md`
and `2026-08-28-plan-d-ci-verification-gaps.md`: macOS-only code cannot be
type-checked on any machine here. CI must compile what no local developer can
before the merge is trusted.

**Gate:** the seven-target build matrix runs on the merge branch and Windows
x64 is green; other targets' failures are triaged and recorded, not silently
accepted.

### Phase D — release pipeline and the first update-capable build

Acts on `shorthand-app`.

1. Remove `signCommand` outright. macOS `signingIdentity` stays at the
   inherited ad-hoc `"-"`, which is the correct value given Decision 2 — it
   is left alone deliberately, not by omission, and a comment says so.
2. Split `sign-binaries` into `sign-binaries` (false) and `sign-updates` (true).
   **Reviewed alone.**
3. Generate the minisign keypair. Private key and its password become
   `TAURI_SIGNING_PRIVATE_KEY` / `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` repository
   secrets, with an offline backup — losing this key means no existing install
   can ever accept another update. **Reviewed alone.**
4. Repoint `plugins.updater.endpoints` at
   `mshish/shorthand/releases/latest/download/latest.json` and replace `pubkey`
   with Shorthand's. **Reviewed alone.** Until this lands, the live risk stands:
   accepting an update prompt replaces Shorthand with Handy.
5. Reset `tauri.conf.json` version to `0.1.0`; change `release.yml`'s tag
   construction to `shorthand-v${version}` and `asset-prefix` to `shorthand`.
6. Cut the draft release, review it, publish.

**Gate — and this is the one that decides whether the goal was met:** install
the published build on a clean Windows machine, publish a `shorthand-v0.1.1`
containing a visible change, and confirm the running app offers it and installs
it. `latest.json` must list every platform that built, which is also the check
that settles Decision 4's open question about whether seven parallel jobs merge
or clobber it.

## Risks carried deliberately

**Public conflict resolution.** Decision 3. Mitigated by merging on a branch
behind a PR.

**No Authenticode.** Every Windows tester sees a SmartScreen warning on first
run. Told in advance, this is friction; discovered cold, it reads as malware.
The release notes and install instructions must say so plainly.

**No macOS developer machine.** macOS builds may fail and cannot be debugged
locally. Accepted for a Windows-first friend test.

**Minisign key custody.** A single point of unrecoverable failure. If the key
is lost, every installed copy is stranded at its current version with no
in-app path forward.

**`shorthand-config` diverges.** It keeps writing `google-credentials.json`
while core reads it. The one-writer constraint is untouched by this design and
still needs an answer before the umbrella work.

## Open questions

1. **Where the minisign private key is backed up.** A password manager entry is
   the obvious answer; it needs to be an actual decision, not an assumption.
2. **`shorthand-legacy` retention.** The fork-migration plan keeps the
   pre-fork repository indefinitely as a private backup and includes no
   deletion step. Confirm that is still wanted.
3. **Branch protection on `main`.** It becomes available the moment the
   repository is public. This design does not set rules, because that is a
   policy choice rather than something to infer.
4. **Whether Phase C's merge lands before or after the first release** if the
   merge turns out to be genuinely hostile. The stated order is before. The
   fallback — ship `0.1.0` from pre-merge `main` and deliver the merge as
   `0.1.1` through the updater, which also exercises the update path on a real
   payload — is available and does not require re-planning.
