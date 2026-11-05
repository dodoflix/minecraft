#!/usr/bin/env bash
# Compile-gated integration.
# 1) In your worktree, unlocked: rebase on main + `cargo check --workspace --tests` (catches your own errors early).
# 2) Under the merge lock: rebase onto the then-current main, then re-check the exact result in ONE persistent gate
#    worktree (/home/dodo/Projects/mc-wt/MergeGate, its own incremental target). Consecutive merges differ only by one
#    branch's diff there, so the locked check is short; then fast-forward main and push.
# Usage (from your worktree): ./scripts/merge-to-main.sh
set -euo pipefail
MAIN=/home/dodo/Projects/minecraft
GATE=/home/dodo/Projects/mc-wt/MergeGate
CL="$MAIN/scripts/cargo-locked.sh"
branch=$(git rev-parse --abbrev-ref HEAD)
if [ "$branch" = main ] || [ "$branch" = HEAD ]; then echo "run from your own worktree branch" >&2; exit 2; fi
if [ "$(git rev-parse --show-toplevel)" = "$MAIN" ]; then echo "never run in the main checkout" >&2; exit 2; fi

git rebase -q main
"$CL" check -q --workspace --tests
(
  flock 9
  git rebase -q main
  sha=$(git rev-parse HEAD)
  [ -d "$GATE" ] || { git -C "$MAIN" worktree add -q --detach "$GATE" main && ln -sfn "$MAIN/ref" "$GATE/ref"; }
  git -C "$GATE" checkout -q --detach "$sha"
  (cd "$GATE" && MC_MERGE_SLOT=1 "$CL" check -q --workspace --tests)
  git -C "$MAIN" merge -q --ff-only "$branch"
  git -C "$MAIN" push -q origin main
) 9>/tmp/mc-merge.lock
echo "merged $branch into main at $(git rev-parse --short main)"
