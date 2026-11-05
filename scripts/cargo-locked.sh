#!/usr/bin/env bash
# Memory-safe cargo for parallel agents: only COMPILATION holds one of N machine-wide slots
# (systemd-oomd killed the session when many mc-game builds overlapped); test execution runs outside the lock.
# Usage: scripts/cargo-locked.sh <cargo subcommand> [args...]   e.g. scripts/cargo-locked.sh test -p mc-game --test foo
set -euo pipefail
SLOTS="${MC_BUILD_SLOTS:-4}"
export CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-6}"

with_slot() {
  # merge-to-main.sh's serialized re-check uses its own slot (only one merge holds the merge lock at a time).
  if [ -n "${MC_MERGE_SLOT:-}" ]; then
    local rc=0
    flock /tmp/mc-build.lock.merge nice -n 10 "$@" || rc=$?
    return $rc
  fi
  while :; do
    # Backpressure: don't start a new compile while RAM+swap headroom is low (oomd kills at 90% of both).
    avail_kb=$(awk '/MemAvailable/{a=$2} /SwapFree/{s=$2} END{print a+s}' /proc/meminfo)
    if [ "$avail_kb" -lt $(( ${MC_MIN_FREE_GB:-8} * 1048576 )) ]; then sleep 10; continue; fi
    for i in $(seq 1 "$SLOTS"); do
      exec {fd}>"/tmp/mc-build.lock.$i"
      if flock -n "$fd"; then
        local rc=0
        nice -n 10 "$@" || rc=$?
        exec {fd}>&-
        return $rc
      fi
      exec {fd}>&-
    done
    sleep 2
  done
}

case "${1:-}" in
  test|bench)
    # Compile under a slot (--no-run goes before any `--` test-binary args), then run without holding it.
    pre=(); post=(); seen=0
    for a in "$@"; do if [ $seen = 1 ]; then post+=("$a"); elif [ "$a" = "--" ]; then seen=1; post+=("$a"); else pre+=("$a"); fi; done
    with_slot cargo "${pre[@]}" --no-run
    nice -n 10 cargo "$@"
    ;;
  run)
    build_args=()
    for a in "${@:2}"; do [ "$a" = "--" ] && break; build_args+=("$a"); done
    with_slot cargo build "${build_args[@]}"
    nice -n 10 cargo "$@"
    ;;
  check|clippy)
    # Check-only incremental caches are small (metadata); rechecks after a rebase drop from ~8 min to <1 min.
    export CARGO_INCREMENTAL=1
    with_slot cargo "$@"
    ;;
  *)
    with_slot cargo "$@"
    ;;
esac
