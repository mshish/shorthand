#!/usr/bin/env bash
# Fork-only. Compiles shorthand-core's `shorthand-notes` CLI into one
# self-contained binary and puts it where Tauri's `externalBin` looks for it:
# src-tauri/binaries/shorthand-notes-<target triple>[.exe]. The app runs it
# for the Google Docs notes target (src-tauri/src/shorthand/google/runner.rs).
#
# Usage: scripts/build-notes-engine.sh <core checkout> [rust target triple]
# The triple defaults to the host's. Bun cross-compiles, so any triple in the
# table below works from any host.
#
# The binary embeds the Bun runtime, so it adds roughly 100 MB or more to
# each installer.
set -euo pipefail

CORE_DIR="${1:?usage: $0 <shorthand-core checkout> [target triple]}"
TRIPLE="${2:-}"
if [ -z "$TRIPLE" ]; then
  TRIPLE="$(rustc -vV | sed -n 's/^host: //p')"
fi

case "$TRIPLE" in
  aarch64-apple-darwin) BUN_TARGET="bun-darwin-arm64" ;;
  x86_64-apple-darwin) BUN_TARGET="bun-darwin-x64" ;;
  x86_64-unknown-linux-gnu) BUN_TARGET="bun-linux-x64" ;;
  aarch64-unknown-linux-gnu) BUN_TARGET="bun-linux-arm64" ;;
  x86_64-pc-windows-msvc) BUN_TARGET="bun-windows-x64" ;;
  # Windows on Arm runs the x64 build under emulation.
  aarch64-pc-windows-msvc) BUN_TARGET="bun-windows-x64" ;;
  *) echo "build-notes-engine: no Bun target for $TRIPLE" >&2; exit 1 ;;
esac

EXT=""
case "$TRIPLE" in *windows*) EXT=".exe" ;; esac

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
OUT_DIR="$ROOT/src-tauri/binaries"
OUT="$OUT_DIR/shorthand-notes-$TRIPLE$EXT"
mkdir -p "$OUT_DIR"

(
  cd "$CORE_DIR"
  bun install --frozen-lockfile
  bun build bin/shorthand-notes.ts --compile --target="$BUN_TARGET" --outfile "$OUT"
)
chmod +x "$OUT" 2>/dev/null || true
echo "notes engine: $OUT"
