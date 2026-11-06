# shellcheck shell=bash
# Shared helpers for the scripts in this directory. Sourced, never run: set ROOT first, then
#   . "$ROOT/scripts/lib.sh"

# die MSG...: prints the script name and MSG to stderr, exits 1.
die() { echo "$(basename "$0"): $*" >&2; exit 1; }

# usage: prints the calling script's header comment (line 2 up to the first line that is not a comment).
usage() { awk 'NR > 1 { if (!/^#/) exit; sub(/^# ?/, ""); print }' "$0"; }

# help_flag "$@": -h/--help as the first argument prints usage and exits 0, before anything is touched.
help_flag() { case "${1:-}" in -h | --help) usage; exit 0 ;; esac; }

# need TOOL...: exits 2 naming every tool missing from PATH.
need() {
  local t missing=()
  for t in "$@"; do command -v "$t" >/dev/null || missing+=("$t"); done
  [ ${#missing[@]} -eq 0 ] || { echo "$(basename "$0"): missing required tools: ${missing[*]}" >&2; exit 2; }
}

# to_file OUT CMD...: replaces OUT with CMD's stdout (gzip -9n when OUT ends in .gz) only when CMD succeeds;
# on failure OUT keeps its content.
to_file() {
  local out=$1 tmp; shift
  mkdir -p "$(dirname "$out")"
  tmp="$(mktemp "$out.XXXXXX")"
  if [[ $out == *.gz ]]; then "$@" | gzip -9n >"$tmp"; else "$@" >"$tmp"; fi || { rm -f "$tmp"; return 1; }
  chmod 644 "$tmp"
  mv "$tmp" "$out"
}

# require_eula: exits unless the user opted in to the Minecraft EULA with MC_ACCEPT_EULA=1.
require_eula() {
  [ "${MC_ACCEPT_EULA:-}" = 1 ] || die "starting a server accepts the Minecraft EULA (https://aka.ms/MinecraftEULA) in your name: set MC_ACCEPT_EULA=1 if you accept it"
}

# write_eula DIR: writes DIR/eula.txt (accepts the EULA) after require_eula; the only place a script writes `eula=true`.
write_eula() {
  require_eula
  echo "eula=true" >"$1/eula.txt"
}

# quiet CMD...: runs CMD with stderr hidden (vanilla bootstrap noise); the captured stderr is printed only when CMD fails.
quiet() {
  local err rc=0; err="$(mktemp)"
  "$@" 2>"$err" || rc=$?
  [ "$rc" -eq 0 ] || cat "$err" >&2
  rm -f "$err"
  return "$rc"
}
