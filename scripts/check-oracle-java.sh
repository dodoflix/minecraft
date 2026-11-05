#!/usr/bin/env bash
# Compile-check the in-process oracle Java sources (duplicate `case` labels, missing symbols, ...) against the vanilla classes without
# running anything: scripts/check-oracle-java.sh [FILE.java ...]   (default: EntityOracleDriver.java + VanillaLevel.java + the other
# scenario drivers it is used with). Needs ref/ and java. Exit 0 iff everything compiles.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
T="$ROOT/crates/mc-game/tests/oracle"
CP="$("$ROOT/scripts/vanilla-cp.sh")"
OUT="$(mktemp -d)"; trap 'rm -rf "$OUT"' EXIT
if [ $# -eq 0 ]; then set -- "$T/EntityOracleDriver.java" "$T/VanillaLevel.java"; fi
# the shadow RandomSupport (reseedUniquifier) must shadow the vanilla class: compile it first and put it in front of the classpath
mkdir -p "$OUT/shadow"
javac -nowarn -cp "$CP" -d "$OUT/shadow" "$T/shadow/net/minecraft/world/level/levelgen/RandomSupport.java" 2>/dev/null
javac -proc:none -nowarn -Xlint:none -cp "$OUT/shadow:$CP" -sourcepath "$T" -d "$OUT" "$@"
echo "oracle java sources compile: $*"
