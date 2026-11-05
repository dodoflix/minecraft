# End-to-end harness: real 26.3 client + real vanilla server

Everything lives in `scripts/`; all state goes under `run/` (gitignored). Needs: `java` (>= 25), `python3`, `xdotool`,
`gamescope` (headless compositor, already installed), network access on first run (downloads via Python urllib). No root, and the
desktop's `DISPLAY`/`WAYLAND_DISPLAY` are never used: the client renders on the GPU inside
`gamescope --backend headless` with its own private Xwayland.

| Script | Purpose |
|---|---|
| `scripts/vanilla-server.sh` | Run `ref/server.jar` as an oracle: `start NAME [--port --seed --level-type --view-distance --gamemode --reset] [key=value…]`, `cmd NAME "say hi"`, `wait NAME REGEX [s]`, `status`, `log`, `stop`. Scratch dir `run/servers/NAME`, offline mode, EULA accepted. `start` blocks until `Done (`. |
| `scripts/real-client.sh` | Run the real client: `prepare` (downloads jar/libs/assets into `run/client-cache`, sha1-verified, ~1 min first time, sound files skipped), `start NAME USER HOST:PORT`, `status`, `screenshot NAME [out.png]`, `key NAME KEYSYM…`, `wait NAME REGEX`, `log`, `stop`. |
| `scripts/e2e-join-check.sh` | One-shot: client joins `HOST PORT`, stays N seconds, screenshots, stops; prints a JSON verdict, exit 0 iff OK. Point it at our server to check that a real client can join. |
| `scripts/e2e/E2eMain.java` | Tiny wrapper around the stock `Main` (compiled automatically into `run/client-cache/e2e-shim`). A daemon thread prints `[E2E] {json}` state lines (current screen, in-level, disconnect-screen text, client game time, chunks received). Needed because the stock client logs *nothing* for a login/config-phase kick, for "joined", or for the tick count. Read-only. |
| `scripts/mc-client-status.py` | Parses `run/clients/NAME/client.out` (+ crash reports) into the JSON used above. |

## Usage

```sh
# oracle server + check
scripts/vanilla-server.sh start demo --port 25571 --seed 777 --reset
scripts/e2e-join-check.sh 127.0.0.1 25571 --seconds 20 --username Steve --out run/e2e/demo
scripts/vanilla-server.sh stop demo

# against our server: just point it at host/port
scripts/e2e-join-check.sh 127.0.0.1 25565 --seconds 30
```

`e2e-join-check.sh HOST PORT [--seconds N=15] [--join-timeout N=90] [--username U] [--out DIR] [--render-distance N=4] [--keep-client]`

Verdict JSON (also `DIR/verdict.json`; artifacts `DIR/{screenshot.png,client.log,crash-report.txt}`):

```json
{ "ok": true, "joined": true, "stayed_connected": true, "connected_seconds": 20,
  "client_game_time_ticks": 573, "chunks_loaded": 117, "disconnect_reason": null,
  "decode_errors": [], "errors": [], "crashed": false, "crash_report": null,
  "screenshot": ".../screenshot.png", "log": ".../client.log", "elapsed_seconds": 29, "host": "...", "port": 25571 }
```

* `joined` — the client had a level **and** a player (play state reached). Latched even if a disconnect follows.
* `disconnect_reason` — text of the client's disconnect screen (works for login/config/play kicks and connect failures,
  e.g. `You are not white-listed on this server!`, `finishConnect(..) failed with error(-111): Connection refused`).
* `decode_errors` — log lines matching packet-decode patterns **plus** the disconnect text when it is a protocol error
  (`Internal Exception: io.netty.handler.codec.DecoderException: Failed to decode packet 'clientbound/minecraft:login_finished'`).
* `client_game_time_ticks` / `chunks_loaded` — last in-world sample of the client level's game time (set by the server's
  time packets, advanced by client ticks) and the number of chunks received: evidence that play-phase traffic flowed.
* `errors` — other WARN/ERROR client log lines, minus known offline noise (auth/Realms/narrator/profile lookups).
* The screenshot is taken (F2 sent via xdotool to the private Xwayland, client writes `game/screenshots/*.png`)
  only while still connected, so it shows the world, not the disconnect screen. If the join fails there is none;
  use `real-client.sh start/screenshot` manually to capture the error screen.

## Client environment (set up by `real-client.sh start`)

854x480 window, render distance 4, simulation 5, all sound categories 0 and OpenAL on its `null` backend
(`ALSOFT_DRIVERS=null`), `pauseOnLostFocus:false`, onboarding/multiplayer warnings skipped, 30 fps cap, Mojang
auth/session hosts pointed at a closed local port (no network stalls; Realms/profile errors are expected noise),
`nice -n 10`, `-XX:ActiveProcessorCount=4`. Joins via `--quickPlayMultiplayer HOST:PORT`, username offline (`--accessToken 0`,
UUID = vanilla offline UUID `OfflinePlayer:<name>`). Extra `options.txt` lines: `--option key:value`.
Game dir `run/clients/NAME/game`; log `run/clients/NAME/client.out` (JVM stdout+stderr, includes gamescope chatter).
Startup to in-world takes ~10–20 s; first run also downloads ~220 MB.

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
