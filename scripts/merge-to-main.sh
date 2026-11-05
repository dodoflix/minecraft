#!/usr/bin/env bash
# Compile-gated integration. 1) rebase on main + `cargo check --workspace --tests` (warms the incremental check cache,
# outside the merge lock); 2) under the merge lock: rebase again onto the then-current main, re-check (fast: incremental),
# fast-forward main and push. The second check is serialized so it cannot livelock and main never receives an
# unchecked combination. Usage (from your worktree): ./scripts/merge-to-main.sh
set -euo pipefail
MAIN=/home/dodo/Projects/minecraft
CL="$MAIN/scripts/cargo-locked.sh"
branch=$(git rev-parse --abbrev-ref HEAD)
if [ "$branch" = main ] || [ "$branch" = HEAD ]; then echo "run from your own worktree branch" >&2; exit 2; fi
if [ "$(git rev-parse --show-toplevel)" = "$MAIN" ]; then echo "never run in the main checkout" >&2; exit 2; fi

git rebase -q main
"$CL" check -q --workspace --tests
(
  flock 9
  git rebase -q main
  MC_MERGE_SLOT=1 "$CL" check -q --workspace --tests
  git -C "$MAIN" merge -q --ff-only "$branch"
  git -C "$MAIN" push -q origin main
) 9>/tmp/mc-merge.lock
echo "merged $branch into main at $(git rev-parse --short main)"
