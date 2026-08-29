Read @AGENTS.md

The rest is deliberately not imported — open it only when the work calls for it.

- `FOLLOW_STREAM.md` — before touching the `--follow-stream` protocol or its
  record shapes. `shorthand-core` is a live consumer, and the protocol has
  already shipped a field addition without a version bump that silently dropped
  every event downstream.
- `BUILD.md` — before build, packaging, or platform-specific work, and for
  running the CI workflows locally with `act`.
- `SIGNING_AND_UPDATES.md` — before anything touching release signing or the
  updater.
- `BRANDING.md` — before renaming, or when a change touches user-visible naming
  inherited from upstream Handy.
- `docs/FRONTEND_TESTING.md` — before adding or changing frontend tests.
