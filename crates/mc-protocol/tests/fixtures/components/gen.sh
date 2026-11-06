#!/usr/bin/env bash
# Regenerates the component golden fixtures from the real vanilla classes (ComponentGolden.java).
# Usage: crates/mc-protocol/tests/fixtures/components/gen.sh [--registries | corpus/NN-x.txt ...]
#   (no argument)           all corpus/*.txt -> golden/<same name>, plus registries.golden
#   corpus/10-x.txt ...     only the given corpus files (paths relative to this dir)
#   --registries            only registries.golden
# Output: crates/mc-protocol/tests/fixtures/components/golden/<corpus name>.txt (<name>.txt.gz when over 1 MiB), crates/mc-protocol/tests/fixtures/components/registries.golden
#   (each file is written to a temp dir first and moved into place only when ComponentGolden succeeds)
# Corpus line: kind<TAB>component-name-or-"-"<TAB>payload  (see ComponentGolden.java for kinds).
# Needs: java, javac, ref/ (scripts/setup-ref.sh).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/../../../../.." && pwd)"
# shellcheck source=scripts/lib.sh
. "$ROOT/scripts/lib.sh"
help_flag "$@"
need java javac
HERE="$ROOT/crates/mc-protocol/tests/fixtures/components"
CP="$("$ROOT/scripts/vanilla-cp.sh")"
OUT="$(mktemp -d)"
trap 'rm -rf "$OUT"' EXIT
javac -nowarn -cp "$CP" -d "$OUT" "$ROOT/tools/extractor/src/extractor/Setup.java" "$HERE/ComponentGolden.java"
mkdir -p "$HERE/golden"

# gen INPUT DEST: runs ComponentGolden INPUT into a temp file, moves it to DEST only on success.
gen() {
  java -cp "$OUT:$CP" ComponentGolden "$1" "$OUT/result.tmp" 2>"$OUT/stderr.log" || { cat "$OUT/stderr.log" >&2; die "ComponentGolden $1 failed"; }
  records="$(wc -l <"$OUT/result.tmp")"
  place_fixture "$OUT/result.tmp" "$2"
}

if [[ "${1:-}" == "--registries" ]]; then
  gen registries "$HERE/registries.golden"
  exit 0
fi
if [[ $# -eq 0 ]]; then
  gen registries "$HERE/registries.golden"
  set -- "$HERE"/corpus/*.txt
fi
for f in "$@"; do
  [[ -f "$f" ]] || f="$HERE/$f"
  gen "$f" "$HERE/golden/$(basename "$f")"
  echo "$(basename "$f"): $records records"
done
