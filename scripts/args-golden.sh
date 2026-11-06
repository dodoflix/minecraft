#!/usr/bin/env bash
# Regenerates the argument-type goldens from the real vanilla classes.
# Usage:
#   scripts/args-golden.sh [TYPE ...]     (default: every type)
#
# Output: crates/mc-game/tests/fixtures/args/<type>_golden.txt (the generator writes into a temp dir; the files are moved
# into place only when it succeeds)
# Needs: java, ref/ (scripts/setup-ref.sh) and the extractor build (scripts/extract.sh; MC_EXTRACTOR_BUILD overrides its directory).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
# shellcheck source=scripts/lib.sh
. "$ROOT/scripts/lib.sh"
help_flag "$@"
need java
export MC_EXTRA_CP="${MC_EXTRACTOR_BUILD:-$ROOT/tools/extractor/build}"
D="$ROOT/crates/mc-game/tests/fixtures/args"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
"$ROOT/scripts/vanilla-java.sh" "$D/ArgsGolden.java" "$tmp" "$@"
mv "$tmp"/*_golden.txt "$D/"
