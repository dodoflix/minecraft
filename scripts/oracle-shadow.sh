#!/usr/bin/env bash
# Prints the directory holding the patched vanilla sources the oracle shadows (Holder, ResourceKey, RandomSource, Util, GossipContainer, RandomSupport).
#
#   scripts/oracle-shadow.sh
#
# They are generated into target/entity-oracle-shadow/src-<stamp> from the decompiled vanilla sources (ref/src,
# `scripts/setup-ref.sh --decompile`) plus crates/mc-game/tests/oracle/shadow.patch, applied without fuzz so it fails loudly
# when the vanilla sources change. A new patch or vanilla source gets a new directory; old ones are left for `target/` cleanup.
# Needs: patch, sha256sum, ref/ with decompiled sources.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
# shellcheck source=scripts/lib.sh
. "$ROOT/scripts/lib.sh"
help_flag "$@"
need patch sha256sum
PATCH="$ROOT/crates/mc-game/tests/oracle/shadow.patch"
SRC="$ROOT/ref/src/net/minecraft"
BASE="$ROOT/target/entity-oracle-shadow"
FILES=(core/Holder resources/ResourceKey util/RandomSource util/Util world/entity/ai/gossip/GossipContainer world/level/levelgen/RandomSupport)
for f in "${FILES[@]}"; do
  [ -f "$SRC/$f.java" ] || die "$SRC/$f.java missing; run scripts/setup-ref.sh --decompile"
done
stamp="$(cd "$SRC" && sha256sum "$PATCH" "${FILES[@]/%/.java}" | sha256sum)"
OUT="$BASE/src-${stamp:0:16}"
if [ ! -d "$OUT" ]; then
  mkdir -p "$BASE"
  tmp="$(mktemp -d "$BASE/.src.XXXXXX")"
  trap 'rm -rf "$tmp"' EXIT
  for f in "${FILES[@]}"; do mkdir -p "$tmp/$(dirname "$f")"; cp "$SRC/$f.java" "$tmp/$f.java"; done
  patch -p1 -F0 -s --no-backup-if-mismatch -d "$tmp" < "$PATCH" >&2 \
    || die "shadow.patch no longer applies to ref/src; rebase it onto the current vanilla sources"
  chmod 755 "$tmp"
  mv "$tmp" "$OUT"
fi
echo "$OUT"
