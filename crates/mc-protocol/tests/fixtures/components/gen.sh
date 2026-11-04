#!/usr/bin/env bash
# Regenerate golden fixtures from the real vanilla classes.
#   gen.sh                  all corpus/*.txt -> golden/<same name>, plus registries.golden
#   gen.sh corpus/10-x.txt  only the given corpus files (paths relative to this dir)
#   gen.sh --registries     only registries.golden
# Corpus line: kind<TAB>component-name-or-"-"<TAB>payload  (see ComponentGolden.java for kinds).
set -euo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(cd "$HERE/../../../../.." && pwd)"
CP="$("$ROOT/scripts/vanilla-cp.sh")"
OUT="$(mktemp -d)"
trap 'rm -rf "$OUT"' EXIT
javac -nowarn -cp "$CP" -d "$OUT" "$ROOT/tools/extractor/src/extractor/Setup.java" "$HERE/ComponentGolden.java" 2>&1 | grep -v "^Note:" || true
mkdir -p "$HERE/golden"
if [[ "${1:-}" == "--registries" ]]; then
  java -cp "$OUT:$CP" ComponentGolden registries "$HERE/registries.golden" 2>/dev/null
  exit 0
fi
if [[ $# -eq 0 ]]; then
  java -cp "$OUT:$CP" ComponentGolden registries "$HERE/registries.golden" 2>/dev/null
  set -- "$HERE"/corpus/*.txt
fi
for f in "$@"; do
  [[ -f "$f" ]] || f="$HERE/$f"
  java -cp "$OUT:$CP" ComponentGolden "$f" "$HERE/golden/$(basename "$f")" 2>/dev/null
  echo "$(basename "$f"): $(wc -l < "$HERE/golden/$(basename "$f")") records"
done
