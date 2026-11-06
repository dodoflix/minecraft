#!/usr/bin/env bash
# Checks commit subjects against Conventional Commits (CONTRIBUTING.md): `type(scope)!: subject`.
#
#   scripts/check-commits.sh [--title TITLE] [RANGE]
#
#   RANGE    git revision range to check, for example `origin/main..HEAD` (default: no commits)
#   --title  also check a pull request title (merge commits are not exempt there)
#
# Types: feat fix perf refactor docs test build ci chore revert style. The scope is optional; when present it is a
# crate name without the `mc-` prefix (read from `cargo metadata`) or one of ci, deps, scripts, tools, docs.
# Subjects over 72 characters only warn; over 100 characters, a trailing period, or a missing type fail.
# Merge commits are skipped. Exit status: 0 ok, 1 a subject failed, 2 usage error.
# Only new commits are checked (CI: a pull request's range; the merge gate: main..HEAD). `d0016913..origin/main` is the
# oldest range that passes; subjects up to and including `d0016913` are history and are not rewritten.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
# shellcheck source=scripts/lib.sh
. "$ROOT/scripts/lib.sh"
help_flag "$@"
need git cargo python3
cd "$ROOT"

TITLE="" RANGE=""
while [ $# -gt 0 ]; do
  case "$1" in
    --title) [ $# -ge 2 ] || { echo "--title needs a value" >&2; exit 2; }; TITLE=$2; shift 2 ;;
    -*) echo "unknown option: $1" >&2; exit 2 ;;
    *) RANGE=$1; shift ;;
  esac
done

crates="$(cargo metadata --no-deps --format-version 1 | python3 -c '
import json, sys
print(*sorted({p["name"].removeprefix("mc-") for p in json.load(sys.stdin)["packages"]}))')"
scopes=" $crates ci deps scripts tools docs "
types=" feat fix perf refactor docs test build ci chore revert style "
re='^([a-z]+)(\(([a-z0-9-]+)\))?!?: (.+)$'

bad=0
# check WHAT SUBJECT: prints the problem and records a failure, or warns about a long subject.
check() {
  local what=$1 s=$2 msg=""
  if ! [[ $s =~ $re ]]; then
    msg="not 'type(scope): subject'"
  elif [[ $types != *" ${BASH_REMATCH[1]} "* ]]; then
    msg="unknown type '${BASH_REMATCH[1]}'"
  elif [ -n "${BASH_REMATCH[3]}" ] && [[ $scopes != *" ${BASH_REMATCH[3]} "* ]]; then
    msg="unknown scope '${BASH_REMATCH[3]}' (crates without mc-, or ci deps scripts tools docs)"
  elif [ ${#s} -gt 100 ]; then
    msg="subject is ${#s} characters, the limit is 100"
  elif [[ $s == *. ]]; then
    msg="subject ends with a period"
  fi
  if [ -n "$msg" ]; then
    echo "FAIL  $what: $s"
    echo "        $msg"
    bad=1
  elif [ ${#s} -gt 72 ]; then
    echo "warn  $what: subject is ${#s} characters, aim for at most 72: $s"
  fi
}

if [ -n "$TITLE" ]; then check "PR title" "$TITLE"; fi
if [ -n "$RANGE" ]; then
  while IFS=$'\t' read -r sha subject; do
    check "$sha" "$subject"
  done < <(git log --no-merges --format=$'%h\t%s' "$RANGE")
fi
[ "$bad" = 0 ] && echo "commit subjects ok"
exit "$bad"
