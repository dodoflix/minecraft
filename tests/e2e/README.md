# End-to-end harness: real 26.3 client + real vanilla server

Everything lives in `scripts/`; all state goes under `run/` (gitignored). Needs: JDK 25 or newer, `python3`, `xdotool`,
`gamescope` (headless compositor), network access on first run (downloads via Python urllib). No root, and the
desktop's `DISPLAY`/`WAYLAND_DISPLAY` are never used: the client renders on the GPU inside
`gamescope --backend headless` with its own private Xwayland.

| Script | Purpose |
|---|---|
| `scripts/vanilla-server.sh` | Run `ref/server.jar` as an oracle: `start NAME [--port --seed --level-type --view-distance --gamemode --reset] [key=value…]`, `cmd NAME "say hi"`, `wait NAME REGEX [s]`, `status`, `log`, `stop`. Scratch dir `run/servers/NAME`, offline mode, EULA accepted. `start` blocks until `Done (`. |
| `scripts/real-client.sh` | Run the real client: `prepare` (downloads jar/libs/assets into `run/client-cache`, sha1-verified, ~1 min first time, sound files skipped), `start NAME USER HOST:PORT`, `status`, `screenshot NAME [out.png]`, `key NAME KEYSYM…`, `wait NAME REGEX`, `log`, `stop`. |
| `scripts/e2e-join-check.sh` | One-shot: client joins `HOST PORT`, stays N seconds, screenshots, stops; prints a JSON verdict, exit 0 iff OK. Point it at our server to check that a real client can join. |
| `scripts/e2e/E2eResolver.java`, `scripts/e2e/net-watch.py` | The isolation of the client (below): a resolver that refuses every name but loopback and the game server's own, and a socket watcher that records connections to non-loopback addresses. |
| `scripts/e2e/E2e{Main,Probe,Snapshot,Control}.java` | Tiny wrapper around the stock `Main` (compiled automatically into `run/client-cache/e2e-shim`). `E2eProbe` prints `[E2E] {json}` state lines (screen, in-level, disconnect-screen text, game time, chunks, packet totals) and counts every packet by protocol phase and type; `E2eSnapshot` dumps the full client state as JSON on request; `E2eControl` executes JSON-line commands (chat, command, screens, clicks, keys, connect, screenshot, ...) on the render thread. Formats: `tests/e2e/client/PROTOCOL.md`. |
| `scripts/client-ctl.sh` | `client-ctl.sh NAME JSON` sends one control command to a running client and prints its reply; `client-ctl.sh NAME snapshot [OUT.json]` dumps the state snapshot. |
| `scripts/mc-client-status.py` | Parses `run/clients/NAME/client.out` (+ crash reports) into the JSON used above. |

## Usage

```sh
# oracle server + check
scripts/vanilla-server.sh start demo --port 25571 --seed 777 --reset
scripts/e2e-join-check.sh 127.0.0.1 25571 --seconds 20 --username Steve --out run/e2e/demo
scripts/vanilla-server.sh stop demo

# against our server: just point it at host/port
scripts/e2e-join-check.sh 127.0.0.1 25566 --seconds 30
```

`e2e-join-check.sh HOST PORT [--seconds N=15] [--join-timeout N=90] [--username U] [--out DIR] [--render-distance N=4] [--keep-client]`

Verdict JSON (also `DIR/verdict.json`; artifacts `DIR/{screenshot.png,client.log,crash-report.txt}`):

```json
{ "ok": true, "joined": true, "stayed_connected": true, "connected_seconds": 20,
  "client_game_time_ticks": 573, "chunks_loaded": 117, "disconnect_reason": null,
  "decode_errors": [], "errors": [], "egress": [], "crashed": false, "crash_report": null,
  "screenshot": ".../screenshot.png", "log": ".../client.log", "elapsed_seconds": 29, "host": "...", "port": 25571 }
```

* `joined` — the world was visible: the client had a level **and** a player and no screen was open (the level exists
  while the loading screen is still up, which does not count). Latched even if a disconnect follows.
* `disconnect_reason` — text of the client's disconnect screen (works for login/config/play kicks and connect failures,
  e.g. `You are not white-listed on this server!`, `finishConnect(..) failed with error(-111): Connection refused`).
* `decode_errors` — log lines matching packet-decode patterns **plus** the disconnect text when it is a protocol error
  (`Internal Exception: io.netty.handler.codec.DecoderException: Failed to decode packet 'clientbound/minecraft:login_finished'`).
* `client_game_time_ticks` / `chunks_loaded` — last in-world sample of the client level's game time (set by the server's
  time packets, advanced by client ticks) and the number of chunks received: evidence that play-phase traffic flowed.
* `errors` — other WARN/ERROR client log lines, minus the exact lines of the known offline noise; any entry fails the run.
* `egress` — every name the client tried to resolve and every non-loopback socket it opened (see below); any entry
  fails the run.
* The screenshot is taken (F2 sent via xdotool to the private Xwayland, client writes `game/screenshots/*.png`)
  only while still connected, so it shows the world, not the disconnect screen. If the join fails there is none;
  use `real-client.sh start/screenshot` manually to capture the error screen.

## Client environment (set up by `real-client.sh start`)

854x480 window, render distance 4, simulation 5, all sound categories 0 and OpenAL on its `null` backend
(`ALSOFT_DRIVERS=null`), `pauseOnLostFocus:false`, onboarding/multiplayer warnings skipped, 30 fps cap, Mojang
services pointed at a closed local port (see below),
`nice -n 10`, `-XX:ActiveProcessorCount=4`. Joins via `--quickPlayMultiplayer HOST:PORT`, username offline (`--accessToken <username>`,
UUID = vanilla offline UUID `OfflinePlayer:<name>`). Extra `options.txt` lines: `--option key:value`.
The options are a fixed profile (no clouds, minimal particles, no entity shadows, GUI scale 2, gamma 0.5, `en_us`, no view
bobbing or screen effects) so frames repeat; `--keep-options` keeps an existing `options.txt`. `--server NAME=HOST:PORT`
(repeatable) writes the server list and `HOST:PORT` `none` starts at the title screen; `--display xvfb` runs on Xvfb with
software GL instead of gamescope. The client log4j configuration (`scripts/client-log4j2.xml`) adds `game/latest.log`, which
also holds DEBUG lines of the network packages (the only place a failed packet decode shows at log level).
Game dir `run/clients/NAME/game`; log `run/clients/NAME/client.out` (JVM stdout+stderr, includes gamescope chatter); control FIFO `run/clients/NAME/ctl`.
Startup to in-world takes ~10–20 s; first run also downloads ~220 MB.

## Isolation from Mojang's services

The client talks to no host outside the machine, and a run that tries fails. authlib honours only
`-Dminecraft.api.discovery.host` and `-Dminecraft.api.env` (`prod` or `staging`); every session, profile, key and
certificate URL comes from the discovery document, and the other `-Dminecraft.api.*.host` flags are ignored.
`real-client.sh start` therefore:

- points the discovery URL at `http://127.0.0.1:9/minecraft/client`, a closed port, so the services are offline
  (`--mojang URL` serves a local mock instead);
- sets `-Drealms.environment=local` (Realms at `localhost:8080`) and takes patchy off the classpath, whose block list
  fetch of `https://sessionserver.mojang.com/blockedservers` is hard-coded;
- installs `E2eResolver`, which refuses every name but loopback, the machine's own and the game server's;
- starts `net-watch.py`, which polls `ss` and records sockets to non-loopback addresses (literal IP addresses are not
  resolver lookups; the game server's own address is allowed).

A refused name is printed as an `[E2E-NET]` line and listed in the status `egress`, like every recorded socket; any
entry fails the verdict. The watcher polls every 200 ms, so a connection that opens and closes within one poll is not
seen. A game server on port 25565 is refused: for that port the client asks DNS for a SRV record, outside the resolver.
Needs `ss` (iproute2).

## Debugging a failing run

`--keep-client` leaves the client running (`scripts/real-client.sh status|screenshot|key|stop e2e-<pid>`, name is
printed on start). Server side: `scripts/vanilla-server.sh` logs to `run/servers/NAME/logs/latest.log`.

## World-generation differential (vanilla vs ours)

`scripts/worldgen-diff.sh SEED overworld|nether|end|all CENTER_X CENTER_Z RADIUS_CHUNKS [--out DIR] [--sides vanilla,ours] [--compare-args "--ignore blocks"]`
generates the same area with the real vanilla server and with our server (forceload + wait for FULL + `save-all flush` + stop),
then runs `worldcmp` (`tools/worldcmp`: block states, block entities, biomes, heightmaps, structure starts/references,
post-processing, ticks, entities per chunk; light/UUID/timestamps ignored; exit 1 on any diff, `--ignore <category>` for known
residuals, `--report x.json` for a machine-readable summary). Output: `DIR/{A,B}/world`, `DIR/report.{txt,json}` (default
`run/wgdiff/<seed>-<dim>-<cx>_<cz>`). `--sides vanilla,vanilla` is the determinism sanity run. Preset cases:
`scripts/worldgen-diff.sh --cases scripts/worldgen-diff-cases.txt [--only REGEX]`; new structure locations for a seed:
`scripts/worldgen-locate.sh SEED...` (real `/locate`). Details/limits: header of `scripts/worldgen-diff.sh`.
Standalone: `cargo run -p worldcmp -- WORLD_A WORLD_B --dim overworld --chunks -4,-4,4,4`.
Exact modes for worlds that went through the same upgrade or save path: `--exact` compares every chunk of `region/`,
`entities/` and `poi/` as stored (equal tags, equal key order, equal serialised bytes, no status skip); `--tree` compares
the directories file by file (file set, gzip NBT decoded exact, region files chunk by chunk, JSON structurally, other
files by sha256). Neither normalises anything except `--ignore-field GLOB[#FIELD.PATH]` (repeatable; `session.lock`,
`logs/`, `level.dat#Data.LastPlayed`, ...; syntax in `tools/worldcmp/src/fields.rs`).

## Mob scenarios (vanilla vs ours, bot over the wire)

`scripts/mob-e2e.sh tests/e2e/mobs/NAME.scn` runs one `mc-testbot` scenario against the real vanilla server and then against
`mc-server`, and diffs what the bot observed (packets, entity metadata, positions, offers, chat). Outputs go to
`run/mob-e2e/NAME/` (`vanilla.{jsonl,obs}`, `ours.{jsonl,obs}`, `diff.txt`, `obs.diff`); `--sides vanilla` only records the
oracle, `--ignore PACKET` drops noisy packets from the diff. Build first: `cargo build -p mc-testbot -p mc-server`.
The scenario language is documented in `crates/mc-testbot/src/scenario.rs`; start a new scenario from
`tests/e2e/mobs/template.scn` (all state changes through `cmd`, observations through `observe`). Exact server-side mob state
(paths, goals, RNG-driven choices) is covered by the in-process entity oracle (`scripts/entity-oracle.sh`), assertions that
must run in CI belong in `crates/mc-server/tests/oracle/entity_dual.rs`.
Scenarios: `summon_mobs` (initial metadata, variants), `darkness_spawning` (natural spawning at night), `combat`, `breeding`,
`villager_trading`, `raid_start`.
