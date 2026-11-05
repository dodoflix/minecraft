#!/usr/bin/env bash
# Memory-safe cargo for parallel agents: only COMPILATION holds one of N machine-wide slots
# (systemd-oomd killed the session when many mc-game builds overlapped); test execution runs outside the lock.
# Usage: scripts/cargo-locked.sh <cargo subcommand> [args...]   e.g. scripts/cargo-locked.sh test -p mc-game --test foo
set -euo pipefail
SLOTS="${MC_BUILD_SLOTS:-2}"
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

# check/clippy may run on an ephemeral cloud builder (scripts/builders/): each live builder has lanes; a lane is held by
# flock for the whole remote run. Waiters register in /tmp/mc-queue so the autoscaler can see the backlog.
BUILDERS_DIR="$(dirname "$(readlink -f "$0")")/builders"
with_slot_or_remote() { # cargo args...
  mkdir -p /tmp/mc-queue /tmp/mc-builders; local qf="/tmp/mc-queue/$$"; : > "$qf"
  trap 'rm -f "$qf"' RETURN
  while :; do
    for reg in /tmp/mc-builders/*.ip; do
      [ -e "$reg" ] || continue
      local name ip; name=$(basename "$reg" .ip); ip=$(cat "$reg")
      for lane in $(seq 1 "${BUILDER_LANES:-8}"); do
        exec {fd}>"/tmp/mc-remote.$name.$lane"
        if flock -n "$fd"; then
          rm -f "$qf"; touch "/tmp/mc-builders/$name.last"
          local rc=0
          "$BUILDERS_DIR/remote-cargo.sh" "$ip" "$lane" "$@" 2>&1 | sed -u "s#/w/src/#$PWD/#g" || rc=${PIPESTATUS[0]}
          touch "/tmp/mc-builders/$name.last"; exec {fd}>&-
          [ "$rc" = 255 ] || return $rc   # 255 = ssh/transport failure: fall back to local below
          echo "[cargo-locked] builder $name unreachable, falling back to local" >&2
          with_slot cargo "$@"; return $?
        fi
        exec {fd}>&-
      done
    done
    local avail_kb; avail_kb=$(awk '/MemAvailable/{a=$2} /SwapFree/{s=$2} END{print a+s}' /proc/meminfo)
    if [ "$avail_kb" -ge $(( ${MC_MIN_FREE_GB:-8} * 1048576 )) ]; then
      for i in $(seq 1 "$SLOTS"); do
        exec {fd}>"/tmp/mc-build.lock.$i"
        if flock -n "$fd"; then
          rm -f "$qf"; local rc=0
          nice -n 10 cargo "$@" || rc=$?
          exec {fd}>&-; return $rc
        fi
        exec {fd}>&-
      done
    fi
    sleep 2
  done
}

case "${1:-}" in
  test|bench)
    # Run the WHOLE test command (build + run, incl. its JVMs) on a builder test lane (1..MC_REMOTE_TEST_LANES). Waiters
    # register in /tmp/mc-queue so the autoscaler adds builders; local fallback only after MC_REMOTE_WAIT_S without any
    # free lane (default 60 min) or when the command is configured through MC_* env vars (paths into this worktree).
    if ! env | grep -qE '^MC_(SERVER_BIN|RECORD|SEED|SCENARIO|GOLDEN|CMDDIFF|TEXT)'; then
      mkdir -p /tmp/mc-queue /tmp/mc-builders; qf="/tmp/mc-queue/$$"; : > "$qf"; trap 'rm -f "$qf"' EXIT
      deadline=$(( $(date +%s) + ${MC_REMOTE_WAIT_S:-3600} ))
      while [ "$(date +%s)" -lt "$deadline" ]; do
        for reg in /tmp/mc-builders/*.ip; do
          [ -e "$reg" ] || continue
          name=$(basename "$reg" .ip)
          for lane in $(seq 1 "${MC_REMOTE_TEST_LANES:-6}"); do
            exec {tfd}>"/tmp/mc-remote.$name.$lane"
            if flock -n "$tfd"; then
              rm -f "$qf"; touch "/tmp/mc-builders/$name.last"; rc=0
              "$BUILDERS_DIR/remote-cargo.sh" "$(cat "$reg")" "$lane" "$@" 2>&1 | sed -u "s#/w/src/#$PWD/#g" || rc=${PIPESTATUS[0]}
              touch "/tmp/mc-builders/$name.last"; exec {tfd}>&-
              [ "$rc" = 255 ] || exit $rc
              break 3
            fi
            exec {tfd}>&-
          done
        done
        sleep 3
      done
      rm -f "$qf"
    fi
    # Local: compile under a slot (--no-run goes before any `--` test-binary args), then run without holding it.
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
    if [ -n "${MC_MERGE_SLOT:-}" ]; then
      # Merge gate (serialized by the merge lock): prefer a builder's dedicated "gate" lane, whose warm cache tracks main.
      for reg in /tmp/mc-builders/*.ip; do
        [ -e "$reg" ] || continue
        name=$(basename "$reg" .ip)
        exec {gfd}>"/tmp/mc-remote.$name.gate"
        if flock -n "$gfd"; then
          touch "/tmp/mc-builders/$name.last"; rc=0
          "$BUILDERS_DIR/remote-cargo.sh" "$(cat "$reg")" gate "$@" 2>&1 | sed -u "s#/w/src/#$PWD/#g" || rc=${PIPESTATUS[0]}
          touch "/tmp/mc-builders/$name.last"; exec {gfd}>&-
          [ "$rc" = 255 ] || exit $rc
          break
        fi
        exec {gfd}>&-
      done
      with_slot cargo "$@"
    else with_slot_or_remote "$@"; fi
    ;;
  *)
    with_slot cargo "$@"
    ;;
esac
