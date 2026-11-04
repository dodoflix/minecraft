#!/usr/bin/env bash
# Prints the Java classpath for the unbundled vanilla 26.3 server (for jshell/javac golden-value checks).
set -euo pipefail
B="$(cd "$(dirname "$0")/.." && pwd)/ref/bundle/META-INF"
find "$B/versions" "$B/libraries" -name '*.jar' | sort | paste -sd:
