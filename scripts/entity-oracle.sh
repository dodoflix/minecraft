#!/usr/bin/env bash
# In-process VANILLA level oracle for entity/player/damage goldens (see crates/mc-game/tests/oracle/VanillaLevel.java).
#
#   scripts/entity-oracle.sh SCENARIO.txt            run a DSL script (EntityOracleDriver.java header lists the commands);
#                                                    the fixture goes to stdout, vanilla/log noise to stderr
#   scripts/entity-oracle.sh SCENARIO.txt -o OUT     same, written to OUT (only if the run succeeded)
#   scripts/entity-oracle.sh MyScenario.java [args]  run your own Java program against the harness classes
#   scripts/entity-oracle.sh --regen                 regenerate every crates/mc-game/tests/fixtures/entity_oracle/*.txt from its
#                                                    scenarios/*.txt twin
# Needs ref/ with decompiled sources (setup-ref.sh --decompile). The shadow classes Holder/RandomSource/RandomSupport (vanilla + tests/oracle/shadow.patch,
# see oracle-shadow.sh) are generated and compiled into target/entity-oracle-shadow on demand.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
T="$ROOT/crates/mc-game/tests"
if [ "${1:-}" = --regen ]; then
  for sc in "$T"/oracle/scenarios/*.txt; do
    n="$(basename "$sc")"
    echo "== $n" >&2
    "$0" "$sc" -o "$T/fixtures/entity_oracle/$n"
  done
  exit 0
fi
[ $# -ge 1 ] || { sed -n 2,13p "$0" >&2; exit 2; }
CP="$("$ROOT/scripts/vanilla-cp.sh")"
SHADOW="$ROOT/target/entity-oracle-shadow"
SHADOW_SRC="$("$ROOT/scripts/oracle-shadow.sh")"
SERVER_JAR="$(find "$ROOT/ref/bundle/META-INF/versions" -name 'server-*.jar' | head -1)"
PATCHED="$SHADOW/$(basename "$SERVER_JAR")"
# The vanilla jar is signed, so a shadow class cannot live in the same package from another classpath entry: build a copy of the
# server jar without signature files and with the shadow RandomSupport class replacing the vanilla one.
if [ ! -f "$PATCHED" ] || [ -n "$(find "$SHADOW_SRC" -name '*.java' -newer "$PATCHED" -print -quit)" ]; then
  mkdir -p "$SHADOW/classes"
  rm -rf "$SHADOW/classes"; mkdir -p "$SHADOW/classes"
  javac -nowarn -cp "$CP" -d "$SHADOW/classes" $(find "$SHADOW_SRC" -name '*.java') 2>/dev/null
  python3 - "$SERVER_JAR" "$PATCHED" "$SHADOW/classes" <<'PY'
import sys, zipfile, os
src, dst, classes = sys.argv[1:]
repl = {}
for d, _, fs in os.walk(classes):
    for f in fs:
        full = os.path.join(d, f)
        repl[os.path.relpath(full, classes)] = open(full, 'rb').read()
with zipfile.ZipFile(src) as zi, zipfile.ZipFile(dst + '.tmp', 'w', zipfile.ZIP_DEFLATED) as zo:
    for it in zi.infolist():
        n = it.filename
        if n.startswith('META-INF/') and n.endswith(('.SF', '.RSA', '.DSA')):
            continue
        zo.writestr(n, repl.pop(n) if n in repl else zi.read(n))
    for n, data in repl.items():
        zo.writestr(n, data)
os.replace(dst + '.tmp', dst)
PY
fi
CP="${CP//"$SERVER_JAR"/"$PATCHED"}"
JAVA_OPTS_ORACLE=(-Xmx2G -Dio.netty.noUnsafe=false -Dlog4j2.configurationFile="$T/oracle/log4j2-oracle.xml")
scenario="$(realpath "$1")"; shift
out=""
if [ "${1:-}" = -o ]; then out="$(realpath -m "$2")"; shift 2; fi
# vanilla writes banned-*.json/ops.json/whitelist.json into the working directory
RUN="$ROOT/target/entity-oracle-run"; mkdir -p "$RUN"; cd "$RUN"
if [[ "$scenario" == *.java ]]; then
  exec java "${JAVA_OPTS_ORACLE[@]}" -cp "$CP" "$scenario" "$@"
fi
if [ -n "$out" ]; then
  mkdir -p "$(dirname "$out")"
  tmp="$(mktemp)"
  java "${JAVA_OPTS_ORACLE[@]}" -cp "$CP" "$T/oracle/EntityOracleDriver.java" "$scenario" "$@" > "$tmp"
  mv "$tmp" "$out"
  wc -l "$out" >&2
else
  exec java "${JAVA_OPTS_ORACLE[@]}" -cp "$CP" "$T/oracle/EntityOracleDriver.java" "$scenario" "$@"
fi
