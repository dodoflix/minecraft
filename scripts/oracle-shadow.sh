#!/usr/bin/env bash
# Prints the directory holding the patched vanilla sources the oracle shadows (Holder, RandomSource, RandomSupport).
# They are generated into target/entity-oracle-shadow/src from the decompiled vanilla sources (ref/src, `scripts/setup-ref.sh --decompile`)
# plus crates/mc-game/tests/oracle/shadow.patch, applied without fuzz so it fails loudly when the vanilla sources change.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
PATCH="$ROOT/crates/mc-game/tests/oracle/shadow.patch"
SRC="$ROOT/ref/src/net/minecraft"
OUT="$ROOT/target/entity-oracle-shadow/src"
FILES=(core/Holder util/RandomSource world/level/levelgen/RandomSupport)
for f in "${FILES[@]}"; do
  [ -f "$SRC/$f.java" ] || { echo "oracle-shadow: $SRC/$f.java missing; run scripts/setup-ref.sh --decompile" >&2; exit 1; }
done
stamp="$(cd "$SRC" && sha256sum "$PATCH" "${FILES[@]/%/.java}" | sha256sum)"
if [ "$(cat "$OUT/.stamp" 2>/dev/null)" != "$stamp" ]; then
  rm -rf "$OUT"
  for f in "${FILES[@]}"; do mkdir -p "$OUT/$(dirname "$f")"; cp "$SRC/$f.java" "$OUT/$f.java"; done
  if ! patch -p1 -F0 -s --no-backup-if-mismatch -d "$OUT" < "$PATCH" >&2; then
    rm -rf "$OUT"
    echo "oracle-shadow: shadow.patch no longer applies to ref/src; rebase it onto the current vanilla sources" >&2
    exit 1
  fi
  echo "$stamp" > "$OUT/.stamp"
fi
echo "$OUT"
