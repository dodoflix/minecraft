#!/usr/bin/env bash
# Dual-server command corpus runner: runs the same command corpus on the REAL vanilla server (scripts/vanilla-server.sh)
# and on OUR server (mc-server), normalizes both outputs, diffs them per command, prints IDENTICAL or the diff.
# Exit status: 0 identical, 1 differences, 2 harness error.
#
#   scripts/command-diff.sh CORPUS... [--seed N] [--level-type flat|normal] [--gamemode M] [--keep] [--tree] [--tree-strict]
#                                     [--out DIR] [--ours-bin PATH] [--no-build]
#   scripts/command-diff.sh --tree                       only compare the live `commands` packets (see below)
#   e.g.  scripts/command-diff.sh crates/mc-game/tests/fixtures/commands_corpus/operating.txt
#
# Every corpus FILE gets a fresh pair of servers (fresh worlds, so runs are reproducible); both sides run in parallel with
# the same seed. Defaults: seed 12345, level-type minecraft:flat, gamemode creative, difficulty peaceful, no structures,
# view/simulation distance 4, spawn-npcs/animals/monsters=false, chat/command spam thresholds disabled. Two bots join each
# server through mc-testbot: `Bot` (opped, level 4, `op Bot` runs before it joins) and `Bot0` (not opped, level 0).
# `--keep` leaves the server directories (run/servers/cmddiff-*, run/cmd-diff/<id>/*/ours) for inspection; normalized
# transcripts of both sides always land in run/cmd-diff/<id>/<corpus>.{vanilla,ours}.txt.
# Needs java (ref/server.jar) and builds mc-server + mc-command-diff with scripts/cargo-locked.sh (unless --no-build /
# --ours-bin). Vanilla JVMs are always stopped on exit (Ctrl-C included).
#
# CORPUS FORMAT (line based; blank lines and `#` comment lines ignored)
#   <command>          console command (a leading `/` is stripped); output = the server log lines it produced plus every
#                      chat/system message the bots received because of it
#   @bot <command>     the command as player Bot (op, permission level 4)
#   @bot0 <command>    the command as player Bot0 (not an op, level 0)
#   !wait <ms>         sleep (real time), then resync
#   !ticks <n>         wait n game ticks: polls `time query gametime` (and runs `tick step n` first when the corpus did
#                      `tick freeze`); `tick freeze` / `tick step` are the way to make tick-dependent output exact
#   !ignore <regex>    drop every normalized output line matching the regex (log, bot chat, packets) for the rest of the file
#   !packets <regex>   also record the packets the bots RECEIVE whose name (e.g. set_title_text, level_particles, sound) matches
#                      (`!packets ^$` switches it off); recorded with the packet's Debug form
#   !join <Bot|Bot0>   (re)connect that bot (e.g. after `kick Bot`); output = join log + messages
#   !leave <Bot|Bot0>  disconnect that bot
# Segmentation: each command is followed by a marker (`say @@END n` from the console; `me @@END n` sent by the
# SAME bot for @bot/@bot0 commands (level 0, so it works for Bot0; same packet type, hence processed right after the command)). The command's output is
# everything before the marker in the server log, and in each bot's chat stream, which is why the order is robust and
# independent of tick timing. A bot kicked by its own command is detected and the marker is sent from the console.
# Bot output lines look like `Bot| [system_chat] <plain text>` followed by `Bot|   json {...}` (the full component incl.
# styles/events); server log lines `log| <text>`.
#
# NORMALIZATION (only what vanilla does non-deterministically; nothing else is hidden)
#   - `[HH:MM:SS] [thread/LEVEL]: ` log prefix: removed (a non-INFO level is kept as `WARN: `)
#   - UUIDs (dashed form, and the `[I; a,b,c,d]` form after UUID/Owner/Thrower/Target/LoveCause/AngryAt/LastHurtBy/Leash/
#     Trusted* keys) -> `<UUID>`, EXCEPT the fixed offline UUIDs of Bot and Bot0 (identical on both servers)
#   - absolute paths of the server directory and of the repository -> `<DIR>`
#   - `/127.0.0.1:<client port>` (connection addresses) -> `/127.0.0.1:<PORT>`
#   - trailing whitespace; the marker lines themselves
#   Entity numeric ids are NOT normalized (they should be equal: same join order, same seed); a corpus that spawns entities
#   whose ids legitimately depend on timing uses `!ignore`.
#
# --tree / --tree-strict: also join both servers as Bot (op) and Bot0, take the first `commands` play packet each bot
#   received (raw packet bytes from the testbot packet log), decode it and compare the two trees per top-level command
#   (name, flags, parser, suggestions, children order, redirect path; `gamerule` as a set). Differing node PATHS are listed,
#   plus the vanilla commands we do not register yet. `--tree` fails on a differing command or extra command, not on
#   missing ones; `--tree-strict` also fails on missing commands and requires the packets to be equal after canonical
#   gamerule ordering (the final goal, see tests/command_tree_golden.rs `tree_equals_vanilla_full`).
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
BUILD=1 BIN=""
ARGS=()
while (($#)); do
  case "$1" in
    --no-build) BUILD=0; shift;;
    --ours-bin) BIN="$2"; shift 2;;
    *) ARGS+=("$1"); shift;;
  esac
done
TARGET="${CARGO_TARGET_DIR:-$ROOT/target}"
if ((BUILD)); then
  (cd "$ROOT" && "$ROOT/scripts/cargo-locked.sh" build -q -p mc-server -p mc-testbot) >&2 || { echo "command-diff: build failed" >&2; exit 2; }
fi
[[ -n "$BIN" ]] || BIN="$TARGET/debug/mc-server"
RUNNER="$TARGET/debug/mc-command-diff"
[[ -x "$RUNNER" ]] || { echo "command-diff: $RUNNER missing (build failed or --no-build)" >&2; exit 2; }
[[ -x "$BIN" ]] || { echo "command-diff: mc-server binary $BIN missing" >&2; exit 2; }
[[ -f "$ROOT/ref/server.jar" ]] || { echo "command-diff: ref/server.jar missing (scripts/setup-ref.sh)" >&2; exit 2; }

export MC_CMDDIFF_RUN_ID="$$-$(date +%H%M%S)"
cleanup() {
  # a killed runner never ran its destructors: stop this run's vanilla servers
  for d in "$ROOT"/run/servers/cmddiff-"$MC_CMDDIFF_RUN_ID"-*; do
    [[ -d "$d" ]] && "$ROOT/scripts/vanilla-server.sh" stop "$(basename "$d")" >/dev/null 2>&1 || true
  done
}
trap cleanup EXIT
set +e
"$RUNNER" --ours-bin "$BIN" "${ARGS[@]}"
exit $?
