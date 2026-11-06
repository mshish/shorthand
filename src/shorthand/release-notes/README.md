# Shorthand release notes

Fork-only. The What's New modal reads its notes from this directory, not from
`src/content/release-notes/`, which holds Handy's notes: they describe Handy's
releases under Handy's name and version numbers, and a Shorthand version that
reached one of them (0.9.0, 0.9.7, 0.10.0) would have shown them as its own.
Upstream syncs keep adding files there; nothing reads them.

To write a note for a Shorthand release, add `<version>.md` here, following the
format in `src/content/release-notes/README.md`. Images go in
`public/release-notes/<version>/`.
