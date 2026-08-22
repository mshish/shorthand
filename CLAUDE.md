Read @AGENTS.md

This is a fork of cjpais/Handy. `main` is the fork's branch and the default; there is no
local upstream mirror, so take upstream by merging the `upstream/main` remote-tracking ref.
`AGENTS.md` leads with that and with keeping the diff mergeable, which is the constraint most
changes here are actually paying for.

The rest is deliberately not imported — open it only when the work calls for it.

- `FOLLOW_STREAM.md` — before touching the `--follow-stream` protocol or its
  record shapes. `shorthand-core` is a live consumer, and the protocol has
  already shipped a field addition without a version bump that silently dropped
  every event downstream.
- `BUILD.md` — before build, packaging, or platform-specific work.
- `SIGNING_AND_UPDATES.md` — before anything touching release signing or the
  updater.
- `BRANDING.md` — before renaming, or when a change touches user-visible naming
  inherited from upstream Handy.
- `docs/FRONTEND_TESTING.md` — before adding or changing frontend tests.
