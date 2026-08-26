# GitHub Fork Migration & README Fork Notes — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Turn `mshish/shorthand` into a GitHub-tracked real fork of `cjpais/Handy` — preserving full commit history (all branches and tags, not just `main`) and the existing `git merge upstream/main` workflow — and bring `README.md` up to date so it explains the fork and documents the fork-only `--follow-stream` flag it's currently missing. Forking `cjpais/Handy` (public) makes the new repo public **immediately and irreversibly on its own** — GitHub does not allow a fork of a public repo to be private. That single fact drives this plan's ordering: everything that must happen before the repo is public (README fixes) happens first, against the current still-private repo.

**Architecture:** Not a code change. Part A is `README.md` edits, committed and pushed to the current (still-private) repo. Part B is a sequence of `gh`/`git` operations against live GitHub state: rename the current repo aside, take a clean bare mirror clone of it, create a real fork of `cjpais/Handy` under a temporary name (this is the moment the repo goes public), mirror-push the bare clone's full history into it, then rename the fork into the `shorthand` name and restore the settings that don't travel with git history.

**Tech Stack:** GitHub CLI (`gh`), git.

**Spec:** This plan doc is self-contained; it was derived from live research (web search + `gh api`/`git` calls against the actual repo) and one adversarial review pass by a second model (via the Codex CLI), which caught three real defects in an earlier draft — see "What changed after review" below. See "Research findings" for the sourced claims it rests on.

## What changed after review (for anyone re-reading this plan)

An earlier version of this plan had the visibility flip as a separate, final, opt-in step ("Task 11: make public"), pushed the README commit only after the migration, and mirror-pushed from this working directory rather than a clean bare clone. A review pass caught three problems with that:

1. **Forking `cjpais/Handy` makes the new repo public the instant it's created** — confirmed against [GitHub's own docs](https://docs.github.com/en/pull-requests/collaborating-with-pull-requests/working-with-forks/about-forks) ("Public repository forks are public... You cannot change the visibility of a fork by itself"). There is no private window after fork creation to defer publishing into — so README work has to happen and be pushed _before_ the fork exists, not after.
2. **This working directory has 158 remote-tracking refs** (`git for-each-ref refs/remotes`), ~150 of them `upstream/*` (other people's development branches on `cjpais/Handy`), plus one branch that exists on `origin` but has no local tracking branch at all (`origin/shorthand-settings-ui`). `git push --mirror` pushes _every_ ref under `refs/`, not just branches and tags you meant to include — mirroring straight from this directory would have dumped all 150 `upstream/*` refs into the fork and silently deleted `shorthand-settings-ui` from it (mirror push deletes remote refs absent locally). Fixed by mirroring from a fresh `git clone --bare` of the renamed-aside old repo instead, which is GitHub's own documented shape for this ([Duplicating a repository](https://docs.github.com/en/repositories/creating-and-managing-repositories/duplicating-a-repository)) and naturally has only `refs/heads/*` and `refs/tags/*`, with every branch the server actually has.
3. **The original phrasing "a real GitHub fork, not a copy" overstated one research citation** — GitHub's duplication doc describes mirroring _without_ forking; it does not itself describe "fork via API, then overwrite content" as a named supported pattern. That combination is this plan's own inference, verified live by the post-push fork-status checks in Task 8 — treat it as a checked assumption, not a cited guarantee.

The task list below is the corrected sequence.

## Research findings (why this plan looks the way it does)

- **GitHub cannot retroactively mark an existing non-fork repo as a fork of something it was never forked from.** GitHub Support can only _repair_ a fork-network link that already existed and broke (e.g. after "Leave fork network"); they confirmed in a 2026 support ticket that they cannot create one from scratch. Source: [github.com/orgs/community/discussions/167393](https://github.com/orgs/community/discussions/167393); mechanics confirmed in [Detaching a fork](https://docs.github.com/en/pull-requests/collaborating-with-pull-requests/working-with-forks/detaching-a-fork).
- **The only thing that creates the "forked from" relationship is GitHub's own Fork button / API / `gh repo fork`.** Plain mirroring (clone --bare + push --mirror) does not establish it by itself — see "What changed after review" point 3.
- **A fork's visibility is tied to its parent's and cannot be set independently.** Verified directly against GitHub Docs (see point 1 above). This is why fork creation, not a later visibility flip, is this plan's actual publish moment.
- **Our local `main` already shares real commit ancestry with `cjpais/Handy`.** Verified directly: `git merge-base main upstream/main` → `549cbde3ebb72459f7f7230783931a45222018a1`, a real shared commit (not a synthetic root). `main` is 95 commits ahead of that point and 14 behind current `upstream/main`. This means mirror-pushing our full history into a fresh fork is not "pushing unrelated history" — it's pushing a superset of what the fork starts with. **This is why the original "single squash commit" idea is unnecessary**: pushing full history costs nothing extra and preserves commit-level attribution and bisectability for every fork-specific change, which a squash would destroy for no benefit.
- **Becoming a real fork changes nothing about the `git merge upstream/main` mechanics** — that already works against any repo URL you can read, fork-flagged or not (confirmed by [Syncing a fork](https://docs.github.com/en/github/collaborating-with-pull-requests/working-with-forks/merging-an-upstream-repository-into-your-fork)). It only unlocks GitHub's fork UI (Sync-fork button, compare-across-forks, PR-to-upstream flow) — which matters for the "upstream some changes back someday" goal.
- **Renaming a GitHub repo auto-redirects issues/stars/git URLs — except once the vacated name is reused by a new repo, at which point the redirect from the old repo stops working** ([Renaming a repository](https://docs.github.com/en/repositories/creating-and-managing-repositories/renaming-a-repository)). This plan deliberately reuses the `shorthand` name for the new fork, which will sever that redirect. Confirmed acceptable: the repo is currently **private with 0 stars, 0 forks, 0 open issues, and no published GitHub Releases** (`gh api repos/mshish/shorthand` / `gh release list`), so there is nothing external to break.
- **`mshish` is a personal GitHub account, not an org** (`gh api users/mshish -q .type` → `User`), so `gh repo fork` needs no `--org` flag — it forks to the authenticated account by default.
- **No repo secrets are currently configured** (`gh api repos/mshish/shorthand/actions/secrets -q .total_count` → `0`), but the workflows in `.github/workflows/*.yml` reference `APPLE_CERTIFICATE`, `APPLE_CERTIFICATE_PASSWORD`, `APPLE_ID`, `APPLE_ID_PASSWORD`, `APPLE_PASSWORD`, `APPLE_TEAM_ID`, `AZURE_CLIENT_ID`, `AZURE_CLIENT_SECRET`, `AZURE_TENANT_ID`, `CACHIX_AUTH_TOKEN`, `KEYCHAIN_PASSWORD`, `TAURI_SIGNING_PRIVATE_KEY`, `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`. None of these need migrating today, but they will not carry over automatically if configured later — flagged in Task 11, not a blocker.
- **Actions are currently disabled on the repo** (`gh api repos/mshish/shorthand/actions/permissions` → `{"enabled":false}`), and GitHub disables Actions by default on newly created forks too — so this is expected to match after migration, but Task 10 verifies it explicitly rather than assuming.
- **Branch protection cannot currently be inspected or set** (`gh api .../branches/main/protection` → 403, "Upgrade to GitHub Pro or make this repository public") — private repos on the free plan don't get it. It becomes available the moment the fork is created (Task 8, since that's the publish moment) — this plan does not set specific rules since that's a policy choice, not something to infer.
- **Settings that git history will not carry across, captured live before migration** (`gh api repos/mshish/shorthand ...`): `has_issues: false`, `has_projects: false`, `has_wiki: false`, `has_discussions: false`, `has_downloads: false`, `allow_merge_commit: true`, `allow_squash_merge: true`, `allow_rebase_merge: true`, `delete_branch_on_merge: false`, `allow_auto_merge: false`. Labels: the 9 GitHub-default labels (`bug`, `documentation`, `duplicate`, `enhancement`, `good first issue`, `help wanted`, `invalid`, `question`, `wontfix`) plus one custom label, `accessibility` (color `f143ab`, description "Barrier affecting people with disabilities"). Collaborators: only the owner (`mshish`). Webhooks: none. Pages: not enabled (404).
- **README gap, confirmed by direct comparison:** `AGENTS.md`'s CLI Parameters table lists `--follow-stream`; `README.md`'s CLI Parameters section does not. `README.md`'s "How to Contribute" section is upstream Handy's own text verbatim — it points at `cjpais/Handy`'s issue tracker, tells the reader to "fork the repository" (a reader on this repo would need to fork _this_ repo, not re-fork Handy), and lists `contact@handy.computer`. It also does not need to say "no template" — `.github/PULL_REQUEST_TEMPLATE.md` exists in this repo and GitHub will pre-fill it into any PR opened against `mshish/shorthand` regardless of what the README says, so the replacement text below doesn't claim otherwise.

## Global Constraints

- Do not delete the pre-migration repo content — rename it aside and keep it as a private backup (reversible), per the user's standing instruction to avoid destructive actions.
- Do not touch the broader "Handy" → "Shorthand" text throughout the rest of `README.md` (binary names, install paths, Homebrew/winget lines, app-data directory paths). `AGENTS.md` explicitly warns against opportunistically extending the rename, and several of those facts (e.g. the actual built binary name) are not independently verifiable from this plan without a full build — out of scope here.
- **README changes must be committed and pushed to the current repo before Task 8 (fork creation)** — once that fork exists, the repo is public; there is no later private window to fix README mistakes in before anyone can see them.
- Every `gh`/`git` command below was checked against `--help` output or live `gh api`/`git` calls against this repo, not assumed from memory; the sequence itself was independently reviewed by a second model before being finalized.
- **Part B's commands assume a POSIX shell** (Git Bash on Windows, Terminal on macOS/Linux) — they use `/tmp`, `grep`, `wc`, `sort`, and `diff` throughout. On Windows, run them through Git Bash, not native PowerShell or `cmd.exe`, or translate each command first.

---

## Task 1: Update `README.md` — add the "About This Fork" section

**Files:**

- Modify: `README.md:1-19` (insert new section after the Discord badge, before `## Why Handy?`)

- [ ] **Step 1: Insert the section**

Insert immediately after line 3 (the Discord badge line) and its following blank line — i.e. the new section goes between the badge and the existing `**A free, open source...**` tagline / `## Why Handy?`:

```markdown
## About This Fork

**Shorthand** (`mshish/shorthand`) is a GitHub-tracked fork of [cjpais/Handy](https://github.com/cjpais/Handy) — recorded in GitHub's fork network, so it can merge upstream Handy's changes and, for changes with nothing fork-specific about them, be offered back to upstream as pull requests.

What's different here:

- **Fork-only features**, built as their own modules and off by default so they stay easy to lift into an upstream PR later — for example [`--follow-stream`](FOLLOW_STREAM.md), which lets another process follow live transcription output.
- **A different visual identity** — see [BRANDING.md](BRANDING.md) for the palette, the mark, and the reasoning behind them.
- **The product name is Shorthand**, not Handy — some inherited code, comments, and documentation below still say "Handy" where the rename hasn't reached yet.

This repository's history includes regular merges from `cjpais/Handy`. See [AGENTS.md](AGENTS.md) for the branch and remote conventions, and this fork's own contribution workflow.
```

- [ ] **Step 2: Verify placement**

```bash
grep -n "^## " README.md | head -5
# Expected order: "## About This Fork" appears after the title/badge, before "## Why Handy?"
```

---

## Task 2: Update `README.md` — document `--follow-stream`

**Files:**

- Modify: `README.md` CLI Parameters section (currently ends around the macOS app-bundle tip, before `## Known Issues & Current Limitations`)

- [ ] **Step 1: Add a new subsection right after the existing "Startup flags" block and its macOS tip callout, before `## Known Issues & Current Limitations`**

````markdown
**Fork-only: live transcript streaming**

```bash
handy --follow-stream        # Follow live transcript events as NDJSON
handy --follow-stream delta  # NDJSON, one record per newly-committed suffix
handy --follow-stream text   # Plain `me: `/`them: ` text as it commits
```
````

Unlike the flags above, `--follow-stream` doesn't control a running instance — it opens a read-only connection to one over a local socket and streams events until you disconnect. Off by default: enable **Follow Live Transcript Output** in Advanced settings first. See [FOLLOW_STREAM.md](FOLLOW_STREAM.md) for the full protocol.

````

- [ ] **Step 2: Verify**

```bash
grep -n "follow-stream" README.md
# Expected: at least 4 matches (the three flag-table lines plus the FOLLOW_STREAM.md link)
````

---

## Task 3: Update `README.md` — fix "How to Contribute"

**Context:** The current section is upstream Handy's own contributing blurb, unedited — wrong repo for the issue link, wrong instruction ("fork the repository" — a reader is already looking at the fork), wrong contact email.

**Files:**

- Modify: `README.md` — the `### How to Contribute` section, near the end, before `## Sponsors`

- [ ] **Step 1: Replace the section body**

Replace:

```markdown
### How to Contribute

1. **Check existing issues** at [github.com/cjpais/Handy/issues](https://github.com/cjpais/Handy/issues)
2. **Fork the repository** and create a feature branch
3. **Test thoroughly** on your target platform
4. **Submit a pull request** with clear description of changes
5. **Join the discussion** - reach out at [contact@handy.computer](mailto:contact@handy.computer)

The goal is to create both a useful tool and a foundation for others to build upon—a well-patterned, simple codebase that serves the community.
```

With:

```markdown
### How to Contribute

Contributing to this fork (`mshish/shorthand`) is ordinary GitHub development: fork it, branch off `main`, test on your target platform, and open a pull request. No community-feedback thread or feature-freeze exemption needed — GitHub will pre-fill the PR description with `cjpais/Handy`'s upstream template; feel free to replace it, since that template's checklist is for PRs aimed at `cjpais/Handy`, not this fork.

If your change belongs upstream instead — a fix or feature with nothing fork-specific about it — see [AGENTS.md § GitHub workflow for AI coding assistants](AGENTS.md#github-workflow-for-ai-coding-assistants) for `cjpais/Handy`'s actual PR template requirements, issue rules, and feature-freeze process before opening anything there.

The goal is to create both a useful tool and a foundation for others to build upon—a well-patterned, simple codebase that serves the community.
```

- [ ] **Step 2: Verify**

```bash
grep -n "contact@handy.computer\|Fork the repository" README.md
# Expected: no matches
```

---

## Task 4: Commit and push the README changes to the current (still-private) repo

**Context:** This must land on `origin` before Task 8 creates the fork — see Global Constraints. Pushing now, while the repo is still private, means any mistake here is caught before anyone outside can see it.

- [ ] **Step 1: Review the diff**

```bash
git diff README.md
```

- [ ] **Step 2: Commit and push — only `README.md`**

This working tree has other files modified from unrelated in-progress work (check with `git status --short` — do not run `git add -A` or `git commit -a` here, or those land in this commit too):

```bash
git add README.md
git status --short
# Expected: exactly one line, "M  README.md" — if anything else is staged, unstage it (git restore --staged <file>) before continuing
git commit -m "docs: explain the fork relationship and document --follow-stream in README"
git push origin main
```

- [ ] **Step 3: Verify the push landed**

```bash
git rev-parse main
git ls-remote origin main
# Expected: identical SHAs
```

---

## Task 5: Rename the current repo aside to free the `shorthand` name

**Context:** We need the name `mshish/shorthand` free for Task 9, but must not lose the existing repo — it's being renamed aside, not deleted, and is about to become the mirror source for Task 6.

- [ ] **Step 1: Rename the current repo**

```bash
gh repo rename shorthand-legacy -R mshish/shorthand -y
```

- [ ] **Step 2: Verify**

```bash
gh repo view mshish/shorthand-legacy --json name,isPrivate -q '{name,isPrivate}'
# Expected: {"name":"shorthand-legacy","isPrivate":true}
```

Note: from this point until Task 9 completes, the local `origin` remote (`https://github.com/mshish/shorthand.git`) is redirecting to `shorthand-legacy`. That redirect breaks the moment Task 9 reclaims the `shorthand` name for the new fork — expected and fine, nothing external depends on the old URL (0 stars/forks/issues confirmed above).

---

## Task 6: Take a clean bare mirror clone of the renamed repo

**Context:** This is the fix from the review pass — mirroring straight from this working directory would also push ~150 unwanted `upstream/*` remote-tracking refs and silently delete the server-only `shorthand-settings-ui` branch (see "What changed after review"). A fresh bare clone of `shorthand-legacy` has exactly its own `refs/heads/*` and `refs/tags/*` — nothing more, nothing less — matching GitHub's own [Duplicating a repository](https://docs.github.com/en/repositories/creating-and-managing-repositories/duplicating-a-repository) shape.

- [ ] **Step 1: Bare-clone it to a scratch location outside this working tree**

```bash
git clone --bare https://github.com/mshish/shorthand-legacy.git /tmp/shorthand-mirror.git
```

- [ ] **Step 2: Record what it contains, for the parity check in Task 8**

```bash
git -C /tmp/shorthand-mirror.git for-each-ref --format='%(refname)' refs/heads refs/tags | wc -l
git -C /tmp/shorthand-mirror.git for-each-ref refs/remotes
# Record the printed count for comparison in Task 8 — don't hardcode an expected number here. The bare
# clone is the source of truth for what should exist (it has 7 branches, not 6: `shorthand-legacy` carries
# `shorthand-settings-ui` on the server even though no local branch tracks it, plus 65 tags — but re-derive
# this from what Step 2 actually prints, not from this comment, in case history has moved since this plan
# was written). The `refs/remotes` line must print nothing — a bare clone of a single-remote source has no
# remote-tracking refs; if it prints anything, stop and investigate before continuing.
```

---

## Task 6a: Scan the full history for secrets before it becomes public

**Context:** Once Task 8's mirror push lands, every commit ever made — not just the tip of `main` — is publicly readable, and GitHub warns that content pushed to a public fork network can remain reachable even if the fork is later deleted or made private again. This is the last point where that's still preventable. A scan was already run once while writing this plan (full `git log --all -p` against private-key headers, AWS/Slack/GitHub-token shapes, and quoted password/API-key assignments) and came back clean — two `aws_secret_access_key=secret`-shaped matches turned out to be a parameter name in vendored example code, not a real value, and three `api_key: "change_post_process_api_key_setting"` matches are a literal placeholder string, not a credential. No `.env`/`.pem`/`.p12`/`.key`/`id_rsa`/`credentials`-named file has ever been added in this history. Re-run before executing, since commits may have landed since:

```bash
git log --all --diff-filter=A --name-only --pretty=format: | sort -u | grep -inE "\.env$|\.pem$|\.p12$|\.pfx$|credentials|secret|\.key$|id_rsa|service-account"
git log --all -p 2>/dev/null | grep -inE "password\s*[:=]\s*['\"][^'\"]{6,}|api[_-]?key\s*[:=]\s*['\"][A-Za-z0-9_\-]{16,}|-----BEGIN [A-Z ]*PRIVATE KEY-----|AKIA[0-9A-Z]{16}|xox[baprs]-[A-Za-z0-9-]+|ghp_[A-Za-z0-9]{30,}|sk-[A-Za-z0-9]{20,}"
```

If either command turns up a real credential (not a placeholder or a parameter name), stop — do not proceed to Task 7 until it's rotated and you've decided how to handle the history (this plan does not cover history rewriting).

---

## Task 7: Create a real fork of `cjpais/Handy` (this is the publish moment)

**Context:** This is the one step that actually establishes GitHub's "forked from" relationship — and, per the research findings, the moment the repo becomes public. There is no way to defer that; accept it here rather than being surprised by it. Forked under a temporary name since `shorthand` is briefly still occupied by the redirect from Task 5.

Two preflight facts, checked live while writing this plan (re-check if much time has passed): `gh api repos/mshish/Handy` → 404, so the account has no pre-existing Handy fork that could collide; `gh api repos/cjpais/Handy/rulesets` → `[]`, so there's no upstream push ruleset that could block the mirror push in Task 8.

- [ ] **Step 1: Fork**

```bash
gh repo fork cjpais/Handy --fork-name shorthand-fork-tmp --default-branch-only --clone=false --remote=false
```

`--default-branch-only` skips fetching Handy's other branches (Task 8 overwrites the fork's content entirely anyway). `--clone=false --remote=false` stop `gh` from touching this working directory's remotes — this plan never modifies `origin`/`upstream` here at all; the bare clone in Task 6 is a separate, throwaway directory.

- [ ] **Step 2: Verify — and confirm it is in fact public**

```bash
gh repo view mshish/shorthand-fork-tmp --json isFork,parent,visibility -q '{isFork,parent:.parent.nameWithOwner,visibility}'
# Expected: {"isFork":true,"parent":"cjpais/Handy","visibility":"PUBLIC"}
```

---

## Task 8: Mirror-push the bare clone's full history into the new fork

- [ ] **Step 1: Push from the bare clone (not this working directory) into the fork**

```bash
git -C /tmp/shorthand-mirror.git push --mirror https://github.com/mshish/shorthand-fork-tmp.git
```

This force-overwrites whatever the fork initially cloned from `cjpais/Handy` with every branch and tag `shorthand-legacy` actually has — no more, no less. `--mirror` is an inherent force-update; no extra flag needed.

- [ ] **Step 2: Verify full ref parity, not just `main`**

```bash
git -C /tmp/shorthand-mirror.git for-each-ref --format='%(refname) %(objectname)' refs/heads refs/tags | sort > /tmp/expected-refs.txt
gh api --paginate repos/mshish/shorthand-fork-tmp/git/refs -q '.[] | "\(.ref) \(.object.sha)"' | sort > /tmp/actual-refs.txt
diff /tmp/expected-refs.txt /tmp/actual-refs.txt
# Expected: no output (identical). The --paginate flag is required, not optional — this endpoint pages
# results by default and there are 72 refs to compare, so a call without it will silently only see the
# first page and falsely report the rest as missing.
```

- [ ] **Step 3: Verify fork status survived the mirror push**

```bash
gh repo view mshish/shorthand-fork-tmp --json isFork,parent -q '{isFork,parent:.parent.nameWithOwner}'
# Expected: {"isFork":true,"parent":"cjpais/Handy"}  (unchanged from Task 7)
```

---

## Task 9: Rename the new fork into the `shorthand` name and restore metadata

**Context:** This is the step that makes `mshish/shorthand` — the name the local `origin` remote already points at — actually be the real fork.

- [ ] **Step 1: Rename**

```bash
gh repo rename shorthand -R mshish/shorthand-fork-tmp -y
```

- [ ] **Step 2: Restore description and the feature toggles captured in Research findings**

```bash
gh repo edit mshish/shorthand \
  --description "Shorthand - live speaker-labelled transcription streamed into note-taking apps. A fork of cjpais/Handy." \
  --enable-issues=false \
  --enable-projects=false \
  --enable-wiki=false \
  --enable-discussions=false
```

`gh repo edit` has no flag for `has_downloads` (checked via `gh repo edit --help`), so restore it through the raw API instead:

```bash
gh api -X PATCH repos/mshish/shorthand -f has_downloads=false
```

The merge-policy fields (`allow_merge_commit`/`allow_squash_merge`/`allow_rebase_merge`: all `true`; `delete_branch_on_merge`/`allow_auto_merge`: both `false`) match what a freshly created GitHub repo gets by default — no action needed, but Step 5 below verifies the final state rather than assuming it held.

- [ ] **Step 3: Recreate the one custom label** (the 9 default labels are auto-created on every new GitHub repo, including this fork; only `accessibility` needs recreating)

```bash
gh label create accessibility -R mshish/shorthand --color f143ab --description "Barrier affecting people with disabilities"
```

- [ ] **Step 4: Verify the local `origin` remote now resolves to the real fork with the right, current content**

```bash
git remote -v
# Expected: origin  https://github.com/mshish/shorthand.git (unchanged from before this plan)

git fetch origin
git rev-parse main
git rev-parse origin/main
# Expected: identical SHAs — origin now points at the real fork, and it includes the README commit from Task 4
```

- [ ] **Step 5: Verify fork status and the full settings snapshot on the final name**

```bash
gh repo view mshish/shorthand --json isFork,parent,defaultBranchRef,visibility -q '{isFork,parent:.parent.nameWithOwner,default:.defaultBranchRef.name,visibility}'
# Expected: {"isFork":true,"parent":"cjpais/Handy","default":"main","visibility":"PUBLIC"}

gh api repos/mshish/shorthand -q '{has_issues,has_projects,has_wiki,has_discussions,has_downloads,allow_merge_commit,allow_squash_merge,allow_rebase_merge,delete_branch_on_merge,allow_auto_merge}'
# Expected: matches the Research findings snapshot exactly — has_issues/projects/wiki/discussions/downloads
# all false, allow_merge_commit/squash_merge/rebase_merge all true, delete_branch_on_merge/allow_auto_merge
# both false. If anything differs, it wasn't restored — go back and fix it before Task 10.

gh api repos/mshish/shorthand/labels -q '.[].name' | sort
# Expected: the same 10 names as the Research findings snapshot (9 GitHub defaults + accessibility)
```

---

## Task 10: Clean up, and confirm Actions state

- [ ] **Step 1: Remove the scratch bare clone**

```bash
rm -rf /tmp/shorthand-mirror.git /tmp/expected-refs.txt /tmp/actual-refs.txt
```

- [ ] **Step 2: Confirm this working directory's remotes are untouched**

```bash
git remote -v
# Expected exactly:
# origin    https://github.com/mshish/shorthand.git (fetch)
# origin    https://github.com/mshish/shorthand.git (push)
# upstream  https://github.com/cjpais/Handy.git (fetch)
# upstream  https://github.com/cjpais/Handy.git (push)
```

- [ ] **Step 3: Confirm Actions permission state matches what you want**

```bash
gh api repos/mshish/shorthand/actions/permissions
# Was {"enabled":false} pre-migration; GitHub also disables Actions by default on new forks, so this is
# expected to already match. If workflows should actually run (e.g. once secrets are configured per
# Task 11), enable explicitly rather than assuming: gh api -X PUT repos/mshish/shorthand/actions/permissions -f enabled=true
```

- [ ] **Step 4 (policy choice, not scripted here):** Branch protection on `main` is now settable (the repo is public) — configure it if desired; this plan doesn't prescribe specific rules.

---

## Task 11: Note the secrets gap for future signing/release CI (no action today)

**Context:** 0 secrets configured today, so there's nothing to migrate right now. This task exists so the gap isn't silently forgotten once release signing is actually set up.

- [ ] **Step 1:** When you configure release signing (per `SIGNING_AND_UPDATES.md`), set these on `mshish/shorthand`: `APPLE_CERTIFICATE`, `APPLE_CERTIFICATE_PASSWORD`, `APPLE_ID`, `APPLE_ID_PASSWORD`, `APPLE_PASSWORD`, `APPLE_TEAM_ID`, `AZURE_CLIENT_ID`, `AZURE_CLIENT_SECRET`, `AZURE_TENANT_ID`, `CACHIX_AUTH_TOKEN`, `KEYCHAIN_PASSWORD`, `TAURI_SIGNING_PRIVATE_KEY`, `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`.

No verification command — this is a forward note, not an action in this plan.

---

## Rollback

If something goes wrong **between Task 5 and Task 9** (i.e. after the old repo is renamed aside but before the new fork is safely renamed into place and verified), the old repo's content is not lost — it's sitting at `mshish/shorthand-legacy`, untouched by anything done to the fork. To recover:

1. If a fork was created under `shorthand-fork-tmp` and something about it is wrong (bad mirror push, wrong content), rename it aside instead of deleting it, in case you want to inspect what went wrong: `gh repo rename shorthand-fork-tmp-broken -R mshish/shorthand-fork-tmp -y`.
2. Rename the legacy repo back into place: `gh repo rename shorthand -R mshish/shorthand-legacy -y`.
3. Verify: `gh repo view mshish/shorthand --json isFork,isPrivate -q '{isFork,isPrivate}'` should show `{"isFork":false,"isPrivate":true}` — i.e. you're back to the exact pre-migration state, and can re-attempt from Task 5.

**This restores the repo name and metadata — it does not undo publication.** The moment Task 7's fork is created it is public (see Global Constraints), and if Task 8 already ran, every commit in the mirrored history was pushed to a public repo, however briefly. Renaming that fork aside or deleting it afterward does not guarantee those commits are unreachable — GitHub's own guidance is that content pushed into a public fork network can remain accessible elsewhere even once the fork is gone. This is exactly why Task 6a's secret scan happens _before_ Task 7, not as a rollback step: rollback can undo the naming and repo-settings mess, it cannot undo exposure.

There is deliberately no rollback step for **after** Task 9 completes and its verification passes — at that point the migration is done and confirmed correct.

---

## Open questions for the user (not assumed)

1. **`shorthand-legacy` naming** — Task 5 picks this name for the archived pre-fork repo; change it if you'd prefer something else.
2. **How long to keep `shorthand-legacy`** — this plan keeps it indefinitely as a private backup; no deletion step is included.
3. **Whether to enable Actions / configure signing secrets now or later** — Task 10/11 leave this as an explicit follow-up rather than bundling it into the migration.
