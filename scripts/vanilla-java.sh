#!/usr/bin/env bash
# Run a single-file Java program against vanilla classes: scripts/vanilla-java.sh Foo.java [args]
# The program may call net.minecraft.SharedConstants.tryDetectVersion(); net.minecraft.server.Bootstrap.bootStrap();
set -euo pipefail
CP="$("$(dirname "$0")/vanilla-cp.sh")"
exec java -cp "$CP" "$@"
