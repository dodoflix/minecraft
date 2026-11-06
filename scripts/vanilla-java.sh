#!/usr/bin/env bash
# Runs a single-file Java program against the unbundled vanilla classes.
#
#   scripts/vanilla-java.sh [-o OUT] Foo.java [args]
#
#   -o OUT   write the result to OUT only when the program succeeds: every argument equal to OUT is replaced by a temp file
#            beside it (programs that take an output path), or the program's stdout is captured when no argument equals OUT.
#            OUT ending in .gz is compressed (programs write plain text).
#
# The program may call net.minecraft.SharedConstants.tryDetectVersion() and net.minecraft.server.Bootstrap.bootStrap().
# Env MC_EXTRA_CP: classpath entries put in front of the vanilla classpath (e.g. tools/extractor/build).
# Needs: java, ref/ (scripts/setup-ref.sh).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
. "$ROOT/scripts/lib.sh"
help_flag "$@"
need java

CP="${MC_EXTRA_CP:+$MC_EXTRA_CP:}$("$ROOT/scripts/vanilla-cp.sh")"
if [ "${1:-}" != -o ]; then exec java -cp "$CP" "$@"; fi

out="${2:?-o needs OUT}"; shift 2
mkdir -p "$(dirname "$out")"
tmp="$(mktemp "$out.XXXXXX")"
trap 'rm -f "$tmp"' EXIT
args=() captured=1
for a in "$@"; do
  if [ "$a" = "$out" ]; then args+=("$tmp"); captured=0; else args+=("$a"); fi
done
if [ $captured = 1 ]; then java -cp "$CP" "${args[@]}" >"$tmp"; else java -cp "$CP" "${args[@]}"; fi
if [[ $out == *.gz ]]; then gzip -9n <"$tmp" >"$tmp.gz"; mv "$tmp.gz" "$tmp"; fi
chmod 644 "$tmp"
mv "$tmp" "$out"
