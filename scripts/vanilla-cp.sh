#!/usr/bin/env bash
# Prints the Java classpath of the unbundled vanilla 26.3 server (for java/javac golden-value checks).
#
#   scripts/vanilla-cp.sh
#
# Needs: find, paste, ref/ (scripts/setup-ref.sh).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
# shellcheck source=scripts/lib.sh
. "$ROOT/scripts/lib.sh"
help_flag "$@"
need find paste
B="$ROOT/ref/bundle/META-INF"
[ -d "$B" ] || die "$B missing; run scripts/setup-ref.sh"
find "$B/versions" "$B/libraries" -name '*.jar' | sort | paste -sd:
