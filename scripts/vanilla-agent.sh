#!/usr/bin/env bash
# Execution-coverage agent for vanilla JVMs (tools/vanilla-agent): which methods and basic blocks of net.minecraft.* ran.
#
#   scripts/vanilla-agent.sh build       compile target/vanilla-agent/agent.jar (when a source is newer)
#   scripts/vanilla-agent.sh opts        print the JVM options (one per line) that attach the agent; nothing when it is off
#   scripts/vanilla-agent.sh selftest    run the agent on a small program and check the markers
#   scripts/vanilla-agent.sh universe OUT.tsv.gz   every method of the server jar's net.minecraft classes with its marker count,
#                                        caller count and whether only debug-flag code calls it (the baseline of a coverage report)
#
# Env MC_VANILLA_AGENT: unset, empty or 0 = off; 1 = on, files in target/vanilla-coverage; any other value = the output directory.
# Env MC_VANILLA_AGENT_MODE: flag (default, 1 byte per marker, races are harmless) or count (hit counts, approximate under threads).
# Env MC_VANILLA_AGENT_LABEL: file name prefix of the run (the launching script sets a default); MC_VANILLA_AGENT_CONTROL: control file.
# Output: DIR/LABEL.PID.cov.gz at JVM exit, and on demand: write `dump [PATH]` or `reset` into DIR/LABEL.PID.ctl.
# Needs: java, javac, jar (JDK 25 or newer).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
# shellcheck source=scripts/lib.sh
. "$ROOT/scripts/lib.sh"
help_flag "$@"
SRC="$ROOT/tools/vanilla-agent"
OUT="$ROOT/target/vanilla-agent"
JAR="$OUT/agent.jar"
HITS="$OUT/hits.jar"

build() {
  need javac jar
  if [ -f "$JAR" ] && [ -z "$(find "$SRC/src" -name '*.java' -newer "$JAR" -print -quit)" ]; then return 0; fi
  mkdir -p "$ROOT/target"
  local tmp; tmp="$(mktemp -d "$ROOT/target/vanilla-agent.XXXXXX")"
  javac -nowarn -d "$tmp/classes" "$SRC"/src/mcagent/*.java
  printf 'Premain-Class: mcagent.Agent\n' >"$tmp/manifest"
  # The marker array lives on the boot class path: the server's bundler loads the game classes in a loader that does not
  # delegate to the system loader.
  jar --create --file "$tmp/hits.jar" -C "$tmp/classes" mcagent/Hits.class
  rm "$tmp/classes/mcagent/Hits.class"
  jar --create --file "$tmp/agent.jar" --manifest "$tmp/manifest" -C "$tmp/classes" .
  mkdir -p "$OUT"
  mv "$tmp/hits.jar" "$HITS"
  mv "$tmp/agent.jar" "$JAR"
  rm -rf "$tmp"
}

case "${1:-}" in
  build) build ;;
  opts)
    case "${MC_VANILLA_AGENT:-}" in "" | 0) exit 0 ;; 1) dir="$ROOT/target/vanilla-coverage" ;; *) dir="$(realpath -m "$MC_VANILLA_AGENT")" ;; esac
    build
    echo "-Xbootclasspath/a:$HITS"
    echo "-javaagent:$JAR=out=$dir,mode=${MC_VANILLA_AGENT_MODE:-flag}"
    # instrumented methods grow past HotSpot's 8000 byte limit for compiling a method at all
    echo "-XX:-DontCompileHugeMethods"
    ;;
  universe)
    need java
    build
    inner="$(find "$ROOT/ref/bundle/META-INF/versions" -name 'server-*.jar' | head -1)"
    [ -n "$inner" ] || die "no server jar under ref/bundle (scripts/setup-ref.sh)"
    java -cp "$JAR" mcagent.Universe "$inner" "${2:?universe needs OUT.tsv.gz}"
    ;;
  selftest)
    need java javac
    build
    mkdir -p "$ROOT/target"
    tmp="$(mktemp -d "$ROOT/target/vanilla-agent-selftest.XXXXXX")"
    trap 'rm -rf "$tmp"' EXIT
    javac -nowarn -cp "$JAR:$HITS" -d "$tmp" "$SRC"/selftest/agenttest/Sample.java "$SRC"/selftest/Selftest.java
    MC_VANILLA_AGENT_LABEL=selftest java "-Xbootclasspath/a:$HITS" "-javaagent:$JAR=out=$tmp,prefix=agenttest/" -cp "$tmp" Selftest "$tmp/self.cov.gz"
    ;;
  *) usage >&2; exit 2 ;;
esac
