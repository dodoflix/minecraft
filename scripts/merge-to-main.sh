#!/usr/bin/env bash
# Compile-gated integration: rebase the current worktree branch on main, `cargo check` the workspace libs and tests
# through the build slots, then fast-forward main under the merge lock and push. Retries if main moved meanwhile.
# Usage (from your worktree): ./scripts/merge-to-main.sh
set -euo pipefail
MAIN=/home/dodo/Projects/minecraft
CL="$MAIN/scripts/cargo-locked.sh"
branch=$(git rev-parse --abbrev-ref HEAD)
[ "$branch" = main ] || [ "$branch" = HEAD ] && { echo "run from your own worktree branch" >&2; exit 2; }
[ "$(git rev-parse --show-toplevel)" = "$MAIN" ] && { echo "never run in the main checkout" >&2; exit 2; }
for attempt in $(seq 1 8); do
  git rebase -q main
  base=$(git rev-parse main)
  "$CL" check -q --workspace --tests
  if (
    flock 9
    [ "$(git rev-parse main)" = "$base" ] || exit 75
    git -C "$MAIN" merge -q --ff-only "$branch"
    git -C "$MAIN" push -q origin main
  ) 9>/tmp/mc-merge.lock; then
    echo "merged $branch into main at $(git rev-parse --short main)"; exit 0
  else
    rc=$?; [ $rc = 75 ] || exit $rc
    echo "main moved during check; retrying ($attempt)" >&2
  fi
done
echo "gave up after 8 attempts (main keeps moving)" >&2; exit 1
