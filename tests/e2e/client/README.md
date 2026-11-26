# Real-client scenarios

`scripts/client-e2e.sh SCENARIO.cscn` runs one scenario with the stock 26.3 client against the vanilla server and against
`mc-server`, one after the other, and compares what the client saw. The client and its state reporting are
`scripts/real-client.sh`; the line formats are in `PROTOCOL.md`.

```sh
MC_ACCEPT_EULA=1 scripts/client-e2e.sh tests/e2e/client/trivial.cscn                 # vanilla, then ours, compared
MC_ACCEPT_EULA=1 scripts/client-e2e.sh tests/e2e/client/trivial.cscn --sides vanilla # records and gates one side
MC_ACCEPT_EULA=1 scripts/client-e2e.sh tests/e2e/client/trivial.cscn --sides vanilla,vanilla:seed=99   # must differ
```

Needs `ref/server.jar`, the client (`scripts/real-client.sh prepare`), and
`cargo build --locked -p mc-server -p mc-testkit --bin mock-mojang`. Output goes to `run/client-e2e/NAME`:
one directory per side (`client.log`, `latest.log`, `server.log`, `mojang.jsonl`, `snapshots/`, `shots/`, `chat/`,
`reports/`, `side.json`), `report.json` and `diff.txt`.

## What a run does

Per side: starts `mock-mojang` (the client and the server both get its discovery URL, so nothing reaches Mojang and
online-mode logins and profile keys work), prepares the server directory (the same `server.properties`, files and data
packs for every side), starts the server, fixes the clock, weather and spawn offset with gamerules, runs the scenario steps with the client, stops everything, keeps the logs.
Then it compares the first two sides (the first is the baseline) and gates their logs.

## Scenario grammar

The grammar is documented in `scripts/client_e2e/scenario.py` (`server`, `file`, `datapack`, `serve`, `tolerate`, `console`,
`client join|title|rejoin|disconnect`, `wait_screen`, `wait_state`, `wait_log`, `wait_server`, `ctl`, `snapshot`, `shot`,
`chat_log`, `expect disconnect`). Waits poll the client's state lines and logs; there is no fixed sleep.

`serve /PATH SRC` answers `GET /PATH` on `127.0.0.1` with the file SRC (a directory is served as a zip with fixed
timestamps, so every side sees the same sha1; any other path answers 404). Three placeholders are filled in `server`
values, `console` text and `ctl` JSON: `{port}` (the side's server port), `{http}` (the base URL of the `serve` entries) and
`{sha1:/PATH}` (the sha1 of a served entry), for example
`server resource-pack={http}/pack.zip resource-pack-sha1={sha1:/pack.zip}`.

`client title` starts the client on the title screen with the side's server in its server list (open it with
`ctl {"cmd": "setScreen", "screen": "multiplayer"}`). A step that starts with `@NAME ` addresses a second client JVM of
that name: `@b client join --user Alex`, `@b wait_screen null`. Its logs are `client-NAME.log` and `latest-NAME.log`, its
snapshots, shots and chat logs are labelled `NAME.LABEL`; without a prefix a step addresses the first client.

## What is compared

| Key | Meaning |
|---|---|
| `error:SIDE:STEP` | a step failed on that side (timeout, client gone, control command refused); STEP is `line N (op)` (a failure before the steps: the message up to its first `:`), the message is the detail |
| `snapshot:LABEL:missing` | only one side saved the snapshot |
| `screens`, `disconnect` | the sequence of screen classes, the disconnect screen texts; `screens@NAME`, `disconnect@NAME` for a second client |
| `snapshot:LABEL:PATH` | a value of the client's state snapshot, after renumbering entity ids by creation order, blanking the UUIDs of non-player entities and ignoring game time; packet counters compare by presence of the packet type; `tolerate GLOB N` lets numbers differ by N and pins a counter (N = 0 is exact) |
| `chat:LABEL` | the `[CHAT]` lines |
| `client-log:SIDE:LINE` | a WARN/ERROR/exception/`Failed to` line that is not in `client-log-noise.txt` (exact lines) |
| `client-log:missing:LINE`, `client-log:extra:LINE` | a log line one side printed and the other did not (run noise such as buffer resizing is normalised away) |
| `client-log@NAME:...` | the same three keys for the log of a second client |
| `server-log:SIDE:LINE` | an error, exception or `Detected unsafe terrain read` line in a server log |
| `server-log:missing:LINE`, `server-log:extra:LINE` | session lines (joined, left, UUID, logged in, kicked) only one server printed |
| `client-report:SIDE:FILE` | the client wrote a crash report or a debug report on that side |
| `shot:LABEL` | screenshots beyond the thresholds, see `shots.md` |

`known-differences.txt` lists accepted differences as `SCENARIO KEY`; the run exits 0 iff every difference is listed and
reports listed ones that no longer occur.

## Tests of the runner

```sh
for t in gate shotdiff runner; do python3 tests/e2e/client/test_$t.py; done
```

They run on recorded client logs (`fixtures/`), generated frames and synthetic side directories and need no client.
