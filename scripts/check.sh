#!/usr/bin/env bash
# The project's checks: one entry point for contributors and CI.
#
#   scripts/check.sh [--fast]
#   scripts/check.sh --list
#
#   --fast   skip clippy (the only check that compiles the workspace and needs ref/)
#   --list   print the index of scripts/: each script and the first line of its header comment
#
# Every check runs; the summary lists the failures. A missing tool is a failure, not a skip: exit status is 0 when
# everything passed, 1 when a check failed, 2 when a required tool is not installed.
# Needs: rustfmt, clippy, cargo-deny, shellcheck, ruff, python3, git.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
# shellcheck source=scripts/lib.sh
. "$ROOT/scripts/lib.sh"
help_flag "$@"
need git python3
cd "$ROOT"

FAST=0
case "${1:-}" in
  "") ;;
  --fast) FAST=1 ;;
  --list)
    for f in scripts/*.sh scripts/*.py; do printf '%-28s %s\n' "${f#scripts/}" "$(sed -n '2s/^# *//p;2s/^"""//p' "$f")"; done
    exit 0 ;;
  *) echo "unknown option: $1" >&2; exit 2 ;;
esac

failed=() missing=()
out="$(mktemp)"
trap 'rm -f "$out"' EXIT

# run NAME CMD...: runs CMD, records NAME as failed on non-zero exit and shows the first lines of its output.
run() {
  local name=$1; shift
  if "$@" >"$out" 2>&1; then
    echo "ok    $name"
  else
    failed+=("$name")
    echo "FAIL  $name"
    sed -n '1,15p' "$out" | sed 's/^/        /'
    local total; total=$(wc -l <"$out")
    [ "$total" -le 15 ] || echo "        ... ($((total - 15)) more lines)"
  fi
}

# need NAME CMD...: runs a tool's version command; records NAME as missing when it fails.
have() {
  local name=$1; shift
  "$@" >/dev/null 2>&1 || { missing+=("$name"); echo "MISS  $name is not installed"; return 1; }
}

# no_match PATTERN: fails when PATTERN matches in tracked Rust sources outside tests directories.
rust_sources() { git ls-files -z 'crates/*.rs' 'tools/*.rs' | grep -zvE '/tests/' ; }
no_match() { ! xargs -0 grep -nE "$1" < <(rust_sources); }

if have rustfmt cargo fmt --version; then run "fmt" cargo fmt --all --check; fi
if [ "$FAST" = 0 ]; then
  if have clippy cargo clippy --version; then
    run "clippy" cargo clippy --workspace --all-targets --all-features --locked --keep-going -- -D warnings
  fi
fi
if have cargo-deny cargo deny --version; then run "deny (licenses bans sources)" cargo deny check licenses bans sources; fi
if have shellcheck shellcheck --version; then
  run "shellcheck" bash -c "git ls-files -z '*.sh' | xargs -0 shellcheck"
fi
if have ruff ruff --version; then
  run "ruff" bash -c "git ls-files -z '*.py' | xargs -0 ruff check --select F,E9"
fi

# Syntactic slop checks (docs/standards.md): code that exists only to silence a lint, files the compiler never sees.
run "no silencer functions (fn _name)" no_match '^\s*(pub(\([a-z]+\))? )?fn _[a-z0-9_]+\s*[(<]'
run "no 'let _ = ident;' statements" no_match '^\s*let _ = &?[A-Za-z_][A-Za-z0-9_:]*;\s*(//.*)?$'
run "no commented-out mod/use/fn/let" no_match '^\s*//\s*(pub )?(mod \w+;|use [A-Za-z_][^;]*;|fn \w+|let (mut )?\w+ =)'
run "every .rs file is in a module tree" python3 scripts/check-modules.py

# Scripts: every generator (fixture/oracle source or golden script) documents how to run it and what it writes, and only
# scripts/lib.sh writes the EULA acceptance (behind MC_ACCEPT_EULA=1).
generator_headers() {
  local f bad=0
  while IFS= read -r -d '' f; do
    case $f in
      *.java) grep -q 'static void main' "$f" || continue ;;
      *.py) grep -qE '__main__|sys\.argv' "$f" || continue ;;
    esac
    for tag in 'Usage:' 'Output:'; do grep -q "$tag" "$f" || { echo "$f: no '$tag' line in the header"; bad=1; }; done
  done < <(git ls-files -z 'crates/*/tests/*.java' 'crates/*/tests/*.py' 'crates/*/tests/*.sh' \
    'scripts/golden-*.sh' 'scripts/*-golden.sh' 'scripts/*-oracle.sh' scripts/worldgen-locate.sh)
  return "$bad"
}
eula_only_in_lib() { ! git grep -n 'eula[=]true' -- '*.sh' ':!scripts/lib.sh'; }
run "generators have Usage:/Output: headers" generator_headers
run "EULA acceptance only in scripts/lib.sh" eula_only_in_lib

# Provenance (NOTICE): no Mojang sources or assets in the tree.
mojang_source() { ! git ls-files -z | xargs -0 grep -lIE '^package net\.minecraft\.'; }
binary_assets() { ! git ls-files | grep -E '\.(jar|class|png|ogg|zip)$'; }
big_files() { ! git ls-files -z | xargs -0 du -b | awk '$1 > 8388608 { print "over 8 MiB: " $2; bad = 1 } END { exit !bad }'; }
run "no net.minecraft sources" mojang_source
run "no jars, classes or assets" binary_assets
run "no tracked file over 8 MiB" big_files

echo
if [ ${#missing[@]} -gt 0 ]; then echo "missing tools: ${missing[*]}"; fi
if [ ${#failed[@]} -gt 0 ]; then echo "failed: ${failed[*]}"; fi
[ ${#missing[@]} -eq 0 ] || exit 2
[ ${#failed[@]} -eq 0 ] || exit 1
echo "all checks passed"
