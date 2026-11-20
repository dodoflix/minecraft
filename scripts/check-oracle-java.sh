#!/usr/bin/env bash
# Compile-checks the in-process oracle Java sources (duplicate `case` labels, missing symbols, ...) against the vanilla classes
# without running anything.
#
#   scripts/check-oracle-java.sh [FILE.java ...]
#
# Default: EntityOracleDriver.java, VanillaLevel.java, MenuOracleBase.java and EnchantAnvilOracle.java (crates/mc-game/tests/oracle) and
# DataFixerOracle.java, SchemaWalk.java, GrammarSample.java, SampleGen.java and FileFixVerbs.java (crates/mc-datafix/tests/oracle). Exit 0 iff everything compiles.
# Needs: java, javac, ref/ with decompiled sources (scripts/setup-ref.sh --decompile).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
# shellcheck source=scripts/lib.sh
. "$ROOT/scripts/lib.sh"
help_flag "$@"
need java javac
T="$ROOT/crates/mc-game/tests/oracle"
CP="$("$ROOT/scripts/vanilla-cp.sh")"
OUT="$(mktemp -d)"; trap 'rm -rf "$OUT"' EXIT
D="$ROOT/crates/mc-datafix/tests/oracle"
if [ $# -eq 0 ]; then
  set -- "$T/EntityOracleDriver.java" "$T/VanillaLevel.java" "$T/MenuOracleBase.java" "$T/EnchantAnvilOracle.java" \
    "$D/DataFixerOracle.java" "$D/SchemaWalk.java" "$D/GrammarSample.java" "$D/SampleGen.java" "$D/FileFixVerbs.java"
fi
# the shadow RandomSupport (reseedUniquifier) must shadow the vanilla class: compile it first and put it in front of the classpath
mkdir -p "$OUT/shadow"
javac -nowarn -cp "$CP" -d "$OUT/shadow" "$("$ROOT/scripts/oracle-shadow.sh")/world/level/levelgen/RandomSupport.java" 2>/dev/null
javac -proc:none -nowarn -Xlint:none -cp "$OUT/shadow:$CP" -sourcepath "$T:$D" -d "$OUT" "$@"
echo "oracle java sources compile: $*"
