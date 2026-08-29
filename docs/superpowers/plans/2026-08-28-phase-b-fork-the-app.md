# Phase B: Fork the app — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

> **This plan supersedes `2026-08-24-github-fork-migration-and-readme.md`.** That plan's approach is sound and most of its researched copy is reused verbatim below, but four of its facts and procedures are wrong or stale (see § "What this corrects"). Execute this document, not that one. Mark the old plan superseded as the first thing you do.

**Goal:** Turn `mshish/shorthand` into a GitHub-tracked real fork of `cjpais/Handy` with full history preserved, publishing it in the process, and bring `README.md` up to date so it is true the moment anyone can read it.

**Architecture:** Three parts. Part 1 (Tasks 1–4) fixes `README.md` while the repository is still private — the last window in which a documentation mistake is invisible. Part 2 (Tasks 5–10) performs the migration: snapshot settings, rename aside, bare mirror clone, scan, fork, mirror-push, rename into place. Task 8 is the irreversible publish moment. Part 3 (Task 11) enables Actions, which Phase C needs and which nothing has needed until now.

**Tech Stack:** `gh` CLI, git (bare clones, mirror push), gitleaks 8.30.1. Part 2's commands assume a **POSIX shell** — Git Bash on Windows. They use `/tmp`, `grep`, `wc`, `sort` and `diff` throughout. Do not run them in PowerShell or `cmd.exe` without translating them first.

**Spec:** `docs/superpowers/specs/2026-08-28-public-release-design.md`

**Precondition:** Phase A complete. `mshish/shorthand-core` and `mshish/shorthand-obsidian-plugin` both return `200` anonymously.

## Global Constraints

Every task's requirements implicitly include these.

- **Forking a public repository publishes yours immediately and irreversibly.** GitHub does not permit a fork of a public repository to be private, and visibility is tied to the parent's. Task 8 is therefore the publish moment — not a later visibility flip. Everything that must be true before publication happens before Task 8.
- **Do not delete the pre-migration repository.** Rename it aside and keep it as a private backup.
- **No agent runs Tasks 6, 8, 9 or 10.** `gh repo rename`, `gh repo fork` and `git push --mirror` are executed by Claude after the user confirms each individually.
- **Re-derive every count at run time.** Branch counts, tag counts and divergence numbers are printed by the commands below and compared against each other. Numbers quoted in prose anywhere in this plan are dated observations, not expected values — history has moved since they were taken and will move again.
- **Shorthand's voice is not Handy's.** The plugin repo's `docs/settings-copy-style.md` is the source. It is scoped to a settings tab, so take the rules that generalise: **rule 3** — describe the consequence, not the mechanism, and keep internal vocabulary out of user-facing prose; **rule 7** — "folder" not "directory", "note" for a Markdown file, American spelling; **rule 8** — sentence case headings; **rule 9** — second person, present tense, active voice, no "we". Rule 1's ethos carries: say it once, plainly, and link rather than inline the background. This repository's `README.md` is inherited Handy copy throughout and reads in Handy's register — longer, more mechanism-forward, more marketing. **The sections this plan rewrites come out in Shorthand's register**, which means the replacement copy is shorter than what it replaces, not merely more accurate. Do not extend that rewrite to sections this plan does not name; that is the rename trap below in a different costume.
- **Do not extend the Handy → Shorthand rename.** `AGENTS.md` is explicit: renaming something upstream did not rename adds conflict surface for no gain. This plan touches `README.md`'s fork-specific sections only, and leaves inherited binary names, install paths and app-data directory paths alone.
- **Keep the diff mergeable.** Phase C merges 25 upstream commits into this tree. Every unnecessary edit to an upstream line made now is a conflict paid for then.
- **Codex's writable root is the shell's working directory at dispatch time, not the path in the brief.** Observed 2026-08-28 in Phase A: a dispatch made from the wrong directory came back "Blocked by workspace permissions" having changed nothing. It fails closed rather than editing the wrong tree, but it costs a round trip. `cd /d/tools/shorthand-repos` and confirm with `pwd` in the same turn as the dispatch.
- **Between Tasks 6 and 10, the local `origin` remote is a trap.** It still reads `https://github.com/mshish/shorthand.git`, which GitHub silently redirects to `shorthand-legacy` after the rename. A `git push origin` in that window lands in the backup repository and reports success. Every command in that window addresses its repository by explicit URL for exactly this reason; do not add one that relies on `origin`. Task 10 Step 4 repoints it.

## What this corrects in the 2026-08-24 plan

1. **Stale divergence counts.** That plan states 95 ahead / 14 behind and hardcodes "7 branches, 65 tags" in a comment. Measured 2026-08-28: **132 ahead, 25 behind**. Every count is re-derived at run time here.
2. **The secret scan targeted the wrong clone.** Its Task 6a ran `git log --all` in the working copy while declaring the bare clone the source of truth for server refs. Branch history existing only on the server would have gone public unscanned. Task 7 scans the bare clone.
3. **The detector was home-grown and incomplete.** Its patterns missed `github_pat_…`, Google API keys, and `client_secret` / `refresh_token` assignments. Replaced with gitleaks, with human review of findings retained.
4. **Rollback could not deliver the retry it promised.** Renaming a failed fork aside leaves the account owning a fork in Handy's network, and GitHub will not create a second one from the same account. § Rollback now handles that.

---

## File Structure

| File | Responsibility | Task |
| --- | --- | --- |
| `README.md:1-19` | title, badge — the "About this fork" section is inserted after it | 1 |
| `README.md` CLI Parameters | startup flags — `--follow-stream` is currently undocumented here despite being in `AGENTS.md` | 2 |
| `README.md` § How to Contribute | currently upstream's text verbatim, pointing at upstream's tracker and email | 3 |
| `docs/superpowers/plans/2026-08-24-github-fork-migration-and-readme.md` | superseded; gets a header saying so | 0 |

---

## Task 0: Mark the superseded plan

**Files:**
- Modify: `docs/superpowers/plans/2026-08-24-github-fork-migration-and-readme.md:1`

**Interfaces:**
- Consumes: nothing.
- Produces: nothing later tasks depend on. Done first so no one executes the wrong document.

- [ ] **Step 1: Add a superseded header**

Insert immediately after the title line:

```markdown
> **SUPERSEDED 2026-08-28** by `2026-08-28-phase-b-fork-the-app.md`. Four of
> this plan's facts and procedures are wrong or stale: the divergence counts
> (95/14, now 132/25), the secret scan targets the working copy rather than the
> bare clone, the home-grown detector misses several credential shapes, and the
> rollback cannot deliver its promised retry. Do not execute this document.
```

- [ ] **Step 2: Commit**

```bash
git add docs/superpowers/plans/2026-08-24-github-fork-migration-and-readme.md
git commit -m "docs: mark the fork-migration plan superseded"
```

---

## Task 1: Add the "About this fork" section to `README.md`

**Repository:** `D:/tools/shorthand-repos/shorthand-app`.

**Files:**
- Modify: `README.md:1-19` — insert after the Discord badge, before `## Why Handy?`

**Interfaces:**
- Consumes: nothing.
- Produces: a README section Tasks 2 and 3 sit alongside. No code interface.

- [ ] **Step 1: Insert the section**

Insert immediately after line 3 (the Discord badge) and its following blank line — between the badge and the existing tagline / `## Why Handy?`:

```markdown
## About this fork

Shorthand is a fork of [Handy](https://github.com/cjpais/Handy), a local speech-to-text app. Everything Handy does, this does.

What it adds:

- **[`--follow-stream`](FOLLOW_STREAM.md)** — another program reads the transcript while you are still talking. The [Obsidian plugin](https://github.com/mshish/shorthand-obsidian-plugin) uses this to keep meeting notes current during the meeting.
- **A different look** — see [BRANDING.md](BRANDING.md).

Handy's own changes still land here regularly. Some inherited code and documentation below still says "Handy" where the rename has not reached. [AGENTS.md](AGENTS.md) covers how this fork tracks upstream and how to contribute.
```

Note the register. This is shorter than the section it replaces in an earlier draft of this plan, and deliberately so — see Global Constraints. It drops "recorded in GitHub's fork network", "lift into an upstream PR later" and "built as their own modules": that is repository mechanism, it belongs in `AGENTS.md`, and a reader deciding whether to download this does not need it. Sentence case heading, second person, no "we".

- [ ] **Step 1a: Verify the "what it adds" list against what actually ships**

The two bullets above are the differentiators this plan could verify on 2026-08-28. **Do not add others from memory.** In particular, system-audio capture on Linux and macOS is on an unmerged branch (`feat/system-audio-linux-macos`) and must not be claimed here.

```bash
grep -n "fork-only\|Fork-only" AGENTS.md
```

Read what that returns and confirm the list is complete and true. If a shipped fork-only feature is missing, add a bullet in the same register — consequence first, mechanism in the linked doc. If a bullet names something not actually shipped, remove it.

- [ ] **Step 2: Verify placement**

```bash
grep -n "^## " README.md | head -5
```

Expected: `## About this fork` appears after the title and badge, before `## Why Handy?`.

---

## Task 2: Document `--follow-stream` in `README.md`

`AGENTS.md`'s CLI Parameters table lists `--follow-stream`; `README.md` does not mention it at all (verified: zero matches). It is the fork's headline feature and the thing `shorthand-core` consumes.

**Files:**
- Modify: `README.md` — CLI Parameters section, after the startup-flags block and its macOS tip, before `## Known Issues & Current Limitations`

**Interfaces:**
- Consumes: Task 1's section (adjacent, not depended on).
- Produces: nothing later tasks depend on.

- [ ] **Step 1: Add the subsection**

````markdown
**Fork-only: live transcript streaming**

```bash
handy --follow-stream        # Follow live transcript events as NDJSON
handy --follow-stream delta  # NDJSON, one record per newly-committed suffix
handy --follow-stream text   # Plain `me: `/`them: ` text as it commits
```

The flags above control a running Shorthand. This one reads from it: you get transcript events until you disconnect, and Shorthand carries on regardless. Turn on **Follow live transcript output** under Advanced first — it is off by default. [FOLLOW_STREAM.md](FOLLOW_STREAM.md) has the protocol.
````

The toggle name is copied from `src/shorthand/locales/en.json:5` and is sentence case there. Do not Title Case it here — a reader scanning the settings pane for "Follow Live Transcript Output" is looking for a control that does not exist under that name. Verify before committing:

```bash
grep -n "followStream.label" src/shorthand/locales/en.json
```

- [ ] **Step 2: Verify**

```bash
grep -c "follow-stream" README.md
```

Expected: at least 4 (three flag lines plus the `FOLLOW_STREAM.md` link). It was 0 before this task.

---

## Task 3: Fix "How to Contribute"

The current section is upstream Handy's blurb, unedited: wrong issue tracker, an instruction to "fork the repository" aimed at a reader who is already looking at the fork, and upstream's contact email.

**Files:**
- Modify: `README.md` — `### How to Contribute`, before `## Sponsors`

**Interfaces:**
- Consumes: nothing.
- Produces: nothing later tasks depend on.

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

Fork it, branch off `main`, test on your platform, open a pull request. Nothing else is required — GitHub pre-fills the description with Handy's template, and you can replace it, because that checklist is for pull requests aimed at Handy.

Sending a change to Handy instead is a different process with real requirements. [AGENTS.md](AGENTS.md#github-workflow-for-ai-coding-assistants) has them.
```

Handy's closing line — "The goal is to create both a useful tool and a foundation for others to build upon…" — is deleted rather than kept. It is upstream's mission statement in upstream's voice, it tells a contributor nothing about how to contribute, and retaining it while rewriting the instructions above it would leave the section half in each register.

- [ ] **Step 2: Verify**

```bash
grep -n "contact@handy.computer\|Fork the repository\|foundation for others to build upon" README.md
```

Expected: no matches. The third pattern catches Handy's mission-statement line, which Step 1 deletes rather than keeps.

---

## Task 4: Commit and push the README changes while still private

This must land on `origin` before Task 8. Pushing now, while private, means a mistake is caught before anyone outside can see it.

**Files:**
- Modify: none beyond Tasks 1–3.

- [ ] **Step 1: Confirm only `README.md` is staged**

```bash
git add README.md && git status --short
```

Expected: exactly one line, `M  README.md`. Anything else staged is unrelated work — unstage it with `git restore --staged <file>` before continuing. The working tree currently carries unrelated modifications to `.nix/bun-lock-hash`, `.nix/bun.nix` and `src/bindings.ts`; none belong in this commit.

- [ ] **Step 2: Commit and push**

```bash
git commit -m "docs: explain the fork, document --follow-stream, fix contributing

README described upstream Handy throughout: it pointed contributors at
cjpais/Handy's tracker and email, told a reader already looking at the fork
to fork the repository, and never mentioned --follow-stream despite it being
the fork's headline feature and shorthand-core's only interface."
git push origin main
```

- [ ] **Step 3: Verify the push landed**

```bash
git rev-parse HEAD && git rev-parse origin/main
```

Expected: identical SHAs.

---

## Review batch 2

Tasks 0–4 are one review batch: four documentation edits and a commit, all in `README.md`.

Dispatch a **Sonnet 5 reviewer** with read-only tools over `origin/main~5..origin/main`. Rubric: Tasks 0–4 above. Ask it specifically to check that no edit touched an upstream-owned line outside the three named sections — Phase C merges 25 upstream commits into this tree, and every stray edit here is a conflict paid for there.

Claude adjudicates. `superpowers:receiving-code-review` applies.

**This is the last review batch before the irreversible step.**

---

## Task 5: Capture the live repository settings snapshot

Git history does not carry repository settings. Capture them from the live repository immediately before the migration, not from any number written in this plan.

**Executed by Claude.**

- [ ] **Step 1: Snapshot settings, labels and remotes to a file**

```bash
snap=/tmp/shorthand-migration && mkdir -p "$snap"
gh api repos/mshish/shorthand > "$snap/repo.json"
gh api repos/mshish/shorthand/labels --paginate > "$snap/labels.json"
gh api repos/mshish/shorthand/actions/permissions > "$snap/actions.json"
gh api repos/mshish/shorthand/hooks > "$snap/hooks.json" 2>/dev/null || echo '[]' > "$snap/hooks.json"

node -p "
  const r = require('$snap/repo.json');
  JSON.stringify({has_issues:r.has_issues,has_projects:r.has_projects,has_wiki:r.has_wiki,
    has_discussions:r.has_discussions,has_downloads:r.has_downloads,
    allow_merge_commit:r.allow_merge_commit,allow_squash_merge:r.allow_squash_merge,
    allow_rebase_merge:r.allow_rebase_merge,delete_branch_on_merge:r.delete_branch_on_merge,
    allow_auto_merge:r.allow_auto_merge,description:r.description},null,1)
"
node -p "require('$snap/labels.json').map(l=>l.name+' '+l.color).join('\n')"
```

Observed 2026-08-28 — **compare, do not assume**: all five `has_*` false; all three `allow_*_merge` true; `delete_branch_on_merge` and `allow_auto_merge` false; the 9 GitHub-default labels plus one custom, `accessibility` (`f143ab`, "Barrier affecting people with disabilities"); Actions `{"enabled":false}`; no webhooks.

- [ ] **Step 2: Record the pre-migration description**

`gh repo fork` sets its own description. Task 10 restores this one.

Currently: `Shorthand - live speaker-labelled transcription streamed into note-taking apps. A fork of cjpais/Handy.`

Confirm from `repo.json` rather than trusting that line.

---

## Task 6: Rename the current repository aside

**Executed by Claude. Requires explicit user confirmation.**

Frees the `shorthand` name for the fork, and keeps the pre-migration content as a private backup.

- [ ] **Step 1: Confirm with the user**

State that `mshish/shorthand` becomes `mshish/shorthand-legacy`, stays private, and is kept indefinitely as a backup. Note that reusing the `shorthand` name in Task 10 will sever GitHub's rename redirect from the legacy repository — acceptable, since it has 0 stars, 0 forks, 0 open issues and no published releases.

- [ ] **Step 2: Rename**

```bash
gh repo rename shorthand-legacy -R mshish/shorthand -y
```

- [ ] **Step 3: Verify**

```bash
gh repo view mshish/shorthand-legacy --json name,isPrivate
```

Expected: `{"name":"shorthand-legacy","isPrivate":true}`.

---

## Task 7: Bare mirror clone, then scan its full history

**Reviewed alone.** The scan is not batched with anything.

The bare clone is the source of truth for what exists on the server, and it is what gets scanned. A working-copy scan cannot see branch history that exists only on the server — `shorthand-legacy` is known to carry at least one such branch.

- [ ] **Step 1: Bare-clone the renamed repository**

```bash
snap=/tmp/shorthand-migration
git clone --bare https://github.com/mshish/shorthand-legacy.git "$snap/mirror.git"
```

- [ ] **Step 2: Record what the clone contains**

```bash
git -C "$snap/mirror.git" for-each-ref --format='%(refname)' refs/heads | wc -l
git -C "$snap/mirror.git" for-each-ref --format='%(refname)' refs/tags  | wc -l
git -C "$snap/mirror.git" for-each-ref --format='%(refname)' refs/remotes
```

Record both counts — Task 9 compares against them. **Do not hardcode expected values**; derive them from what this prints. The `refs/remotes` line must print nothing: a bare clone of a single-remote source has no remote-tracking refs. If it prints anything, stop and investigate.

- [ ] **Step 3: Scan the full history with gitleaks**

gitleaks was installed in Phase A Task 1. Confirm it is still available, then scan.

```bash
gitleaks version
gitleaks detect --source "$snap/mirror.git" --no-banner \
  --report-format json --report-path "$snap/app-findings.json"
echo "exit: $?"
```

Exit `1` means findings, which is a result to investigate, not a tool failure. Exit `126`/`127` is a tool failure.

- [ ] **Step 4: Summarise findings**

```bash
node -e "
  const r = require('$snap/app-findings.json');
  console.log(r.length + ' findings');
  for (const x of r) console.log([x.RuleID, x.File, x.StartLine, x.Commit.slice(0,8)].join('  '));
" 2>/dev/null || echo "(no findings file — gitleaks found nothing)"
```

- [ ] **Step 5: STOP. Human reviews every finding**

Do not classify anything as a false positive. Present the summary and the report path, and wait.

This is the last preventable moment. After Task 8, every commit in this history is public, and rollback cannot undo that — GitHub's own guidance is that content pushed into a public fork network can remain reachable after the fork is gone.

If any finding is a real secret: **rotate the credential regardless of whether history is rewritten.** A private repository is not evidence the value was never exposed.

Note for context: the workflows in `.github/workflows/` reference `APPLE_*`, `AZURE_*`, `CACHIX_AUTH_TOKEN`, `KEYCHAIN_PASSWORD` and `TAURI_SIGNING_*` by name. Those are **references to secrets, not secrets** — no repository secrets are configured (`total_count: 0`). Expect gitleaks to be quiet about them; if it flags one, read the actual matched value before concluding anything.

---

## Task 8: Create the fork — the publish moment

**Executed by Claude. Requires explicit user confirmation. Irreversible.**

**Preconditions:** Task 7's findings cleared by the user. Task 4 pushed. Task 5 snapshot captured.

- [ ] **Step 1: Confirm with the user, stating plainly what cannot be undone**

State: this creates a public fork of `cjpais/Handy` under `mshish`, and Task 9 pushes 132 commits of history into it. A fork of a public repository cannot be private. From this point the history is public, and making the repository private later does not un-publish what was read, cached or forked in the meantime.

Wait for an unambiguous yes.

- [ ] **Step 2: Fork**

`mshish` is a personal account, not an org (`gh api users/mshish -q .type` → `User`), so no `--org` flag. Forking into a temporary name avoids a race with the `shorthand` name Task 10 claims.

```bash
gh repo fork cjpais/Handy --fork-name shorthand-fork-tmp --clone=false --remote=false
```

- [ ] **Step 3: Verify the fork relationship exists**

This is the only thing that creates a "forked from" link. GitHub Support cannot add one retroactively to a repository that was never forked, and plain mirroring does not establish it.

```bash
gh repo view mshish/shorthand-fork-tmp --json isFork,parent,visibility \
  -q '{isFork,parent:.parent.nameWithOwner,visibility}'
```

Expected: `{"isFork":true,"parent":"cjpais/Handy","visibility":"PUBLIC"}`.

---

## Task 9: Mirror-push the full history into the fork

**Executed by Claude.**

The fork currently holds upstream's history. This pushes the superset — every branch and tag from the legacy repository, including the 132 fork commits sitting on the shared merge base.

- [ ] **Step 1: Push all refs**

```bash
snap=/tmp/shorthand-migration
git -C "$snap/mirror.git" push --mirror https://github.com/mshish/shorthand-fork-tmp.git
```

- [ ] **Step 2: Compare the fork's refs against the clone's, ref by ref**

`--paginate` is required, not optional: this endpoint pages by default, and a call without it silently sees only the first page and falsely reports the rest as missing.

```bash
git -C "$snap/mirror.git" for-each-ref --format='%(refname)' | sort > "$snap/local-refs.txt"
gh api --paginate repos/mshish/shorthand-fork-tmp/git/refs -q '.[].ref' | sort > "$snap/remote-refs.txt"
diff "$snap/local-refs.txt" "$snap/remote-refs.txt"
```

Expected: no output. Lines present remotely but not locally are upstream's own refs and are fine; investigate any line present locally but not remotely — that is history that failed to push.

- [ ] **Step 3: Confirm the fork relationship survived the mirror push**

```bash
gh repo view mshish/shorthand-fork-tmp --json isFork,parent -q '{isFork,parent:.parent.nameWithOwner}'
```

Expected: unchanged from Task 8 Step 3.

---

## Task 10: Rename the fork into place and restore metadata

**Executed by Claude.**

- [ ] **Step 1: Rename**

```bash
gh repo rename shorthand -R mshish/shorthand-fork-tmp -y
```

- [ ] **Step 2: Restore the description and settings from the Task 5 snapshot**

`gh repo fork` set its own description; the snapshot has the real one.

```bash
gh repo edit mshish/shorthand \
  --description "Shorthand - live speaker-labelled transcription streamed into note-taking apps. A fork of cjpais/Handy." \
  --enable-issues=false --enable-projects=false --enable-wiki=false \
  --enable-merge-commit=true --enable-squash-merge=true --enable-rebase-merge=true \
  --delete-branch-on-merge=false --enable-auto-merge=false
```

- [ ] **Step 3: Restore the custom label**

The 9 GitHub defaults come with any repository. `accessibility` does not.

```bash
gh api -X POST repos/mshish/shorthand/labels \
  -f name=accessibility -f color=f143ab \
  -f description="Barrier affecting people with disabilities"
```

- [ ] **Step 4: Point the local checkout at the fork and confirm `upstream` is intact**

```bash
cd /d/tools/shorthand-repos/shorthand-app
git remote set-url origin https://github.com/mshish/shorthand.git
git remote -v
```

Expected exactly:

```
origin    https://github.com/mshish/shorthand.git (fetch)
origin    https://github.com/mshish/shorthand.git (push)
upstream  https://github.com/cjpais/Handy.git (fetch)
upstream  https://github.com/cjpais/Handy.git (push)
```

- [ ] **Step 5: Confirm the local checkout and the fork agree**

```bash
git fetch origin && git rev-parse HEAD && git rev-parse origin/main
```

Expected: identical SHAs, and that SHA includes Task 4's README commit.

- [ ] **Step 6: Verify the end state against the snapshot**

```bash
gh repo view mshish/shorthand --json isFork,parent,defaultBranchRef,visibility \
  -q '{isFork,parent:.parent.nameWithOwner,default:.defaultBranchRef.name,visibility}'
gh api repos/mshish/shorthand -q '{has_issues,has_projects,has_wiki,has_discussions,has_downloads,allow_merge_commit,allow_squash_merge,allow_rebase_merge,delete_branch_on_merge,allow_auto_merge}'
gh api repos/mshish/shorthand/labels --paginate -q '.[].name' | sort
```

Expected: `isFork: true`, `parent: cjpais/Handy`, `default: main`, `visibility: PUBLIC`; settings matching `$snap/repo.json`; the 10 label names matching `$snap/labels.json`. Anything that differs was not restored — fix it here rather than noting it.

---

## Task 11: Enable Actions

Actions are disabled at the repository level, and GitHub also disables them by default on new forks — so the fork is expected to arrive disabled and match the pre-migration state. Phase C is the first thing that needs them, and enabling is a deliberate act rather than an assumption.

- [ ] **Step 1: Confirm the current state**

```bash
gh api repos/mshish/shorthand/actions/permissions
```

Expected: `{"enabled":false, ...}`.

- [ ] **Step 2: Confirm with the user before enabling**

Enabling means pushes to `main` trigger `main-build.yml` across seven platforms, and pull requests trigger `test.yml`. That consumes Actions minutes and will surface failures that have been latent while nothing ran. That is the point — Phase C depends on CI compiling what no local machine can — but it should not be a surprise.

- [ ] **Step 3: Enable**

```bash
gh api -X PUT repos/mshish/shorthand/actions/permissions -f enabled=true
gh api repos/mshish/shorthand/actions/permissions
```

Expected: `{"enabled":true, ...}`.

---

## Phase gate

- [ ] **Step 1: The repository is a public fork with complete history**

```bash
gh repo view mshish/shorthand --json isFork,parent,visibility -q '{isFork,parent:.parent.nameWithOwner,visibility}'
curl -s -o /dev/null -w '%{http_code}\n' https://api.github.com/repos/mshish/shorthand
```

Expected: `{"isFork":true,"parent":"cjpais/Handy","visibility":"PUBLIC"}` and `200`.

- [ ] **Step 2: The ref comparison from Task 9 Step 2 is still empty**

Re-run it against the renamed repository. A rename does not move refs, but this is the check that would catch it if something did.

- [ ] **Step 3: An anonymous clone builds**

```bash
tmp=$(mktemp -d) && git -C "$tmp" clone --depth 1 https://github.com/mshish/shorthand.git s
cd "$tmp/s" && bun install && bun run build
```

Expected: pass. This is the frontend build only — a full `tauri build` is Phase D's concern and still blocked by the inherited `signCommand`.

- [ ] **Step 4: The legacy backup is intact**

```bash
gh repo view mshish/shorthand-legacy --json name,isPrivate
```

Expected: `{"name":"shorthand-legacy","isPrivate":true}`.

---

## Rollback

Recoverable **only between Tasks 6 and 10**, and only for naming and repository settings. The legacy content is safe at `mshish/shorthand-legacy` throughout.

1. If a fork exists under `shorthand-fork-tmp` and something about it is wrong, **do not delete it and re-fork.** GitHub will not create a second fork of `cjpais/Handy` from the same account, so deleting it forecloses the retry this rollback is for. Instead:
   - **Preferred:** repair it in place. A bad mirror push is fixed by re-running Task 9; the fork relationship survives it.
   - If the fork itself is unusable, rename it aside (`gh repo rename shorthand-fork-broken -R mshish/shorthand-fork-tmp -y`) and note that **Task 8 cannot simply be retried** — the account already owns a fork in Handy's network. Detaching that fork, or forking from a different account, becomes a prerequisite. Resolve this before attempting Task 8 again.
2. Rename the legacy repository back into place:
   ```bash
   gh repo rename shorthand -R mshish/shorthand-legacy -y
   ```
3. Verify:
   ```bash
   gh repo view mshish/shorthand --json isFork,isPrivate -q '{isFork,isPrivate}'
   ```
   Expected: `{"isFork":false,"isPrivate":true}` — the exact pre-migration state.

**This restores naming and settings. It does not undo publication.** From Task 8 onward the fork is public, and once Task 9 has run every mirrored commit has been pushed to a public repository, however briefly. That is precisely why Task 7's scan comes before Task 8 rather than appearing here.

There is deliberately no rollback after Task 10 verifies. At that point the migration is done and confirmed.

---

## Known consequences, carried deliberately

**The rename redirect from `shorthand-legacy` breaks.** GitHub redirects a renamed repository's URLs until the vacated name is reused, and Task 10 reuses it. Acceptable: nothing external points at it.

**Phase C's conflict resolution will be public.** The spec records this as a chosen trade. The compensating control is that the merge lands through a pull request with CI green, never a direct push to `main`.

**Signing secrets do not carry across.** The workflows reference `APPLE_*`, `AZURE_*`, `CACHIX_AUTH_TOKEN`, `KEYCHAIN_PASSWORD` and `TAURI_SIGNING_*`. None are configured now and none migrate automatically. Phase D configures `TAURI_SIGNING_PRIVATE_KEY` and its password; the rest stay unset, which is correct given no Authenticode and no Apple signing.

**Branch protection becomes available.** Private repositories on the free plan cannot set it; a public one can. This plan does not set rules — that is a policy choice, and it is open question 3 in the spec.
