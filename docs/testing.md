# Testing

This is the one document that describes how tests are organised, what they need to run, and how fixtures are made.
Each rule says how it is enforced. "Review" means a reviewer checks it by hand. Where the tree does not meet a rule
yet, the text says so.

The goal of the test suite is parity: for the same input, this server must produce what vanilla Minecraft 26.3
produces. Most tests therefore replay recordings made from the real vanilla classes and compare byte for byte or
line for line.

## 1. Tiers

Tests are grouped by what they need to run.

| Tier | Needs | Runs in `cargo test --workspace` | Where |
|---|---|---|---|
| T0 hermetic | nothing beyond a successful build | yes | unit tests, `tests/golden/`, committed fixtures |
| T1 reads `ref/` | `ref/` from `scripts/setup-ref.sh` | yes | suites that read vanilla data (registries, reports, data packs) from `ref/` |
| T2 JVM oracle | `ref/`, JDK 25 or newer, `MC_ACCEPT_EULA=1`, and a spawned vanilla JVM and/or our `mc-server` | no, all `#[ignore]`d | `tests/oracle/` target of a crate |
| T3 real client | GPU, `gamescope`, `xdotool`, client download | no, manual | [`tests/e2e/README.md`](../tests/e2e/README.md) |

T0 and T1 spawn no processes. Building the workspace already needs `ref/` and JDK 25 or newer, because the build script of
`mc-data` reads the extractor output under `ref/`. `scripts/setup-ref.sh` downloads the vanilla
server from Mojang; you accept Mojang's EULA yourself (section 3). `scripts/setup-ref.sh --decompile` additionally
decompiles the server jar into `ref/src/net/minecraft/`: you need it to read the vanilla source, and the oracle
programs that compile against vanilla sources (`scripts/oracle-shadow.sh`, `scripts/entity-oracle.sh`) refuse to run without it.

**Contract.** `cargo test --workspace` runs T0 and T1 only. After `scripts/setup-ref.sh` it reports exactly the
failures listed in `tests/known-failures.txt` (section 8) and no others, so it does not pass cleanly while that file has
lines. `scripts/test-known-failures.py` runs the same tests and fails only on a failure that the file does not list; the
nightly CI run uses it (section 9). `cargo nextest` may be used as a faster runner. A cold debug build of the tests takes a long time and
several GB under `target/`.

**T2 ignores.** T2 tests live in a dedicated target, `crates/<crate>/tests/oracle/main.rs`, and every test in it
carries `#[ignore = "<class>: <reason>"]`. There is no Cargo feature for it. Run the oracle tier of the whole
workspace, or of one crate:

```sh
MC_ACCEPT_EULA=1 cargo test --workspace --test oracle -- --ignored
MC_ACCEPT_EULA=1 cargo test -p <crate> --test oracle -- --ignored
```

`--workspace --test oracle` skips the crates that have no `oracle` target. Add `--test-threads=2` on a small machine:
every oracle test starts its own server on its own port and directory, but each vanilla JVM needs about 1 GB.
Never use `--include-ignored` over the workspace: it would also run fixture regeneration and the known-red
`deviation:` tests.

Ignore classes (the only allowed prefixes):

- `oracle: <what it needs>`: needs the vanilla JVM or a running server, for example `oracle: spawns vanilla server`.
- `deviation: <what differs>`: our behaviour differs from vanilla; an issue number is required.
- `permanent: <why>`: vanilla behaviour that cannot be represented in Rust; explain why.

A bare `#[ignore]` is not allowed, and `#[should_panic]` always has `expected = "..."`. Enforced by
`clippy::ignore_without_reason` (via `cargo clippy --all-targets`) and review of the class prefix. Every ignore in
an `oracle` target has the `oracle:` class; outside it only `deviation:` and `permanent:` occur.

## 2. A missing prerequisite fails, it does not skip

A test whose prerequisite is absent panics with the command that fixes it, for example
`ref/ is missing: run scripts/setup-ref.sh`. A green run must mean the check ran.

`eprintln!("skipping ...")` followed by an early return is forbidden in `crates/*/tests`. Enforced: review.

Two cases need care:

- Tests that need our server binary use `env!("CARGO_BIN_EXE_mc-server")`, which Cargo only defines for the
  integration tests of package `mc-server`. They live in `crates/mc-server/tests/oracle`, which builds the binary
  itself. There is no `MC_SERVER_BIN` variable for tests.
- The dual-server command corpora (`crates/mc-game/tests/fixtures/commands_corpus/*.txt`, runner
  `scripts/command-diff.sh`) are committed as vanilla transcripts (`commands_corpus/golden/<corpus>.vanilla.txt`,
  recorded with `--record`). `crates/mc-server/tests/oracle/command_corpora.rs` runs one corpus per test against our
  server only (no JVM) and compares it with its transcript; a command whose output is a known difference carries a
  `!known <reason>` line in the corpus.
- Tests that read a vanilla-generated world get it from `mc_testkit::worldref()`. The first call starts the vanilla
  server once (seed 12345, 7x7 chunks force-loaded in each dimension, 30 s of game time, clean stop) and leaves the
  world in `run/servers/worldref`; later calls and
  other test processes reuse it. Delete that directory to regenerate it. Because generating it spawns a JVM, these
  tests are T2.

## 3. EULA

T2 and T3 start the real vanilla server, which requires accepting the
[Minecraft EULA](https://www.minecraft.net/en-us/eula). Running them is your acceptance, not the project's.

Rule: no helper or script writes `eula=true` unless `MC_ACCEPT_EULA=1` is set; without it they print the EULA URL
and exit non-zero. CI sets the variable explicitly for the oracle lane. In Rust the guarded helper is `mc-testkit` (below); the
scripts write `eula=true` only through `write_eula` in `scripts/lib.sh`. Enforced: `scripts/check.sh` fails on `eula=true`
in a shell script other than `scripts/lib.sh`; review elsewhere.

### The server helper

Every test that needs a live server starts it through `mc_testkit::Server::vanilla()` (the vanilla JVM) or
`Server::ours(bin)` (our `mc-server`). There is no second launcher. The crate has no workspace dependencies, so any
crate can use it as a dev-dependency.

- The returned guard owns the child. Dropping it, also while the test panics, kills the child's whole process group,
  and the kernel kills the child when the test process dies (`kill -9` included).
- The port is chosen by the OS and passed to the server; if another process takes it first, the start is retried with
  a new port. No fixed ports.
- Readiness is the `Done (` log line plus an answered status ping, with a timeout. No fixed sleeps after start.
  With `enable-status=false` the server closes status connections; the helper then treats the close as the ready signal.
- `Builder::arg("--demo")` passes command-line arguments of the jar and of `mc-server` (after `nogui`).
- `Builder::jar(path)` / `Builder::java(path)` run an older vanilla jar (see `scripts/datafix-worlds.sh`) with a chosen `java`
  executable (releases up to 1.16.5 need JDK 8) instead of `ref/server.jar` and the `java` on `PATH`.
- `Builder::env("MINECRAFT_API_DISCOVERY_HOST", url)` sets an environment variable of the server process (here: the Mojang
  Services discovery URL, read like authlib's `minecraft.api.discovery.host`, so a test can serve its own keys).
- `mc_testkit::MockMojang` is a local stand-in for the Mojang web services (session join/`hasJoined`, profile keys,
  player certificates signed by its own key, attributes, block list), served from one discovery document.
  `Builder::mojang(&mock)` points a server at it (`-Dminecraft.api.discovery.host` for the vanilla jar; for `mc-server`
  `MINECRAFT_API_DISCOVERY_HOST`, which redirects its profile keys and lookups, not yet its `hasJoined` request);
  `Builder::java_property(key, value)` sets any other JVM property of the vanilla server. The `mock-mojang` binary
  (`cargo run -p mc-testkit --bin mock-mojang`) prints the discovery URL and serves until killed, for the real client's
  services option in `scripts/real-client.sh`. The bearer token of a request names the profile: a UUID, else a player
  name with its offline UUID, so the client has to run with its username as access token. The oracle test
  `mc-protocol/tests/oracle/mock_mojang.rs` shows an online-mode login against vanilla.
- Each server runs in its own temporary directory (or one the test passes with `.dir(..)`), never in `run/servers/<name>`.
- A missing `java`, `ref/server.jar` or `MC_ACCEPT_EULA=1` makes `start()` return an error that names the fix; tests
  `expect` it and fail.
- At most two tests of one process run servers at the same time, whatever `--test-threads` says: a test takes a
  permit with its first server and keeps it until its last server is dropped (`MC_TESTKIT_TESTS=<n>` changes the
  limit). Without it, a default `cargo test --test oracle` would start one JVM per core.
- Dual-server tests wait for events where one exists instead of for a fixed time: a console command is fenced by a log line plus an
  answered `/list` (`record_setblock.rs`), a walk starts once the chunks around it arrived, a join waits for the spawn
  position. The `*_dual_ours` tests fail on any difference from the vanilla fixtures (`PORTAL_DUAL_STRICT=0` and
  `ENTITY_DUAL_STRICT=0` only report it); debugging dumps go to `target/tmp/oracle-dumps/`, never to a fixed `/tmp` path.

## 4. Layout

- A test lives in the lowest crate where it passes with identical results. Integration goldens live in `mc-game`,
  the crate that wires everything together. Review.
- Few test targets per crate: `tests/golden/main.rs` for T0/T1, `tests/oracle/main.rs` for T2, topics as modules
  (`mod args;`, `mod chunk_world;`, ...). A module keeps the name of the vanilla area it tests, so a test is
  `<module>::<test>` in the output. Only `mc-server` has nothing but an oracle target; crates without T2 tests have
  only `golden`. `ls crates/*/tests` shows which crate has which target. Run one module with
  `cargo test -p mc-game --test golden -- args::`.
- Block registration guards (`mc-game` `golden/block_registration_guards.rs`): a registered block class must override what its
  registered ancestors override, a block must be bound to a behaviour that overrides every server-side hook its vanilla
  class chain overrides (`tests/fixtures/block_overrides.txt`, regenerated with `scripts/vanilla-java.sh -o ... BlockOverridesGolden.java`),
  random ticking, signal and analog output blocks need their methods, every block entity type has an implementation, and
  every `blk_*` oracle scenario has a fixture and a replay. A known gap goes into the `ACCEPTED_*` lists with its reason;
  an entry that is no longer a gap fails the test, so the lists only shrink.
- Block coverage: `block_behaviour_golden` hashes `canSurvive`, `updateShape`, `rotate`, `mirror` and `postProcessPos` of every block
  state against vanilla; `block_placement_golden` compares `getStateForPlacement` of every block; `container_block_entities_golden`
  compares the saved NBT, update tag and item components of every block entity type; and every `blk_*` scenario
  (`crates/mc-game/tests/oracle/scenarios/`) is recorded from the vanilla server and replayed for ticks, neighbour
  and shape updates, fluids, redstone, pistons, drops, growth and the rest of the observable block behaviour. What no test
  covers is behaviour that needs entities, items or menus that are not ported yet (README, Known gaps).
- Menu coverage: `tests/oracle/MenuOracleBase.java` opens a menu through its block for a real `ServerPlayer` of the in-process vanilla
  level and records scripts (`crates/mc-game/tests/oracle/menu_scripts/NAME.txt`: open, set, click, drag, button, rename, close,
  block, tick, recipe, gamerule, seed, map, maybuild, setbeacon, seeded random sessions) with the menu, inventory, xp, drops, packets and level
  broadcasts after every op (`scripts/menu-oracle.sh NAME`, fixtures in `fixtures/menu_oracle/`). `golden/common/menu_oracle.rs`
  replays them on a stand-in player and names the first diverging op with both sides' state. A new menu is one entry in the kind
  table on each side and a script. A menu that an entity provides (villager trading, horse inventories) is opened with `SPAWN` and
  `USE` after an `OPEN` of the kind `merchant` or `mount`, and `TRADE`, `NBT`, `DISCARD`, `PLAYER` and `HEALTH` change the world around
  it; statistics and advancement triggers need a real player and are compared by the packet scenarios (`mnu_trade_session`).
- Command coverage: a block-tier scenario that starts with `command_env` replays real commands through an installed server and compares
  what they report, not only what they change. `cmd COMMAND...` runs as the console, `cmd_quiet` runs without printing, `cmd_json`
  adds the JSON of every message, and `cmd_as PLAYER COMMAND...` runs with that player's position, rotation and permissions
  (`cmd_as -N COMMAND...` is the console at permission level `N`). Each message becomes a `CF msg TEXT` line (`CF fail TEXT` for the red
  failure form, `CF json JSON` for the canonical component JSON) and each call of the result callback a `CF result success|failure N`
  line; the oracle driver prints the same lines, so feedback text, results and permission denials are compared line by line. A
  player that left or was kicked can still be the subject of `cmd_as` and `dump stats:`: the driver keeps the removed player object,
  the replay remembers what it needs. The dumps (`scoreboard`, `bossbars`, `storage:ID`, `select:EXECUTOR/X,Y,Z/SELECTOR`,
  `advancements:`, `stats:`, `logs`) compare the state the commands changed. `legacy_feedback` keeps older fixtures recorded
  before `CF` lines existed comparable: it turns the replay's `CF` lines back into the plain `C` lines they contain. The dual-server
  command corpora above remain the proof of the operator commands that need a network, a file or a second server.
- Scenarios with several levels: `levels overworld,the_nether,the_end` (before the first world command) makes the replay and the
  oracle hold those levels of one server; every level is preloaded with the same `preload R` radius (`preload_settle` too) and
  ticks with its chunks. `dimension` names the level world verbs act on (default `overworld`), `in DIM LINE` runs one line with
  `DIM` as that level, and a line that names an entity or a player runs in the level that entity is in. `end void` and
  `nether void` generate that dimension flat and empty, `end fight` keeps the End's `EnderDragonFight` (removed otherwise).
  `tp_dim REF DIM X Y Z` is `Entity.teleport` with a transition into `DIM` (a non-player becomes a new entity there and the name
  follows it), `cmd` runs `/execute in`, `/tp` and the rest through the real commands, `dump levels` prints one `LEVEL` line per
  level (game time, forced and loaded chunks, players, border) and `dump players` prints `PL NAME dim=DIM` per player. The mob
  verbs `tame`, `love` and `leash` take their second reference from any level (a player may be away in another
  one; a horse needs the player in its own level); `ride` and `mob_attack` need both in one level.
- Persistence scenarios: `save [flush]`, `unload CX CZ`, `unloadall`, `load CX CZ`, `reload CX CZ`, `leave PLAYER` and
  `rejoin PLAYER` (the player logs in again from the saved data and the saved dimension, into that level, through the real login
  path) run in the focused level; the file dumps `region:CX,CZ`, `entityfile:CX,CZ`, `poifile:CX,CZ`, `playerfile:PLAYER`,
  `datafile:ID`, `statsfile:PLAYER` and `advancementsfile:PLAYER` print the NBT the server wrote, with entity UUIDs replaced by
  scenario names. Each verb reports the ticks the server needed (`UNLOAD cx cz ticks=N loaded=B`), so unload timing is compared too.
  Grammar and recorded twin: `tests/oracle/PersistVerbs.java`, replay `golden/common/block_scenario_persist.rs`.
- Save-compare (`crates/mc-server/tests/oracle/world_save_compare.rs`): the same console and bot script runs against the vanilla
  server and `mc-server` (seed 12345, `save-all flush`, stop) and the two world directories are compared file by file: file list,
  `level.dat`, the chunk NBT of every `region`, `entities` and `poi` file, the saved data of every dimension and the player files,
  each by file kind. Clocks, unordered collections, all-zero light arrays and unseeded random draws are normalised; terrain
  generated while the server runs is skipped. A difference that is not fixed is a line of its `KNOWN` table with a reason, and a
  line that no longer matches fails the scene. `SAVE_COMPARE_KEEP=<dir>` keeps both directories, `SAVE_COMPARE_BASELINE=1` runs
  vanilla twice to show what vanilla itself varies.
- Dual-server suites (T2, `crates/mc-server/tests/oracle/`; except the two live ones below, each `*_vanilla` test records the
  committed fixtures in `tests/fixtures/<suite>/` with `MC_UPDATE_FIXTURES=1` and each `*_ours` test compares `mc-server` with
  them): `light_packets_dual` (the light a client rebuilds from the chunk and `light_update` packets; one live test, no fixtures),
  `chunk_delivery_dual` (chunk batches and the time to deliver a number of chunks, compared with a tolerance; one live test, no
  fixtures), `jsonrpc_dual` (the JSON-RPC management server through one websocket
  client), `rcon_dual` (RCON authentication, feedback, packet limits and the files the server writes; an RCON server gets a free
  port per launch, `Server::rcon_port`), `demo_dual` (`--demo` and `--bonusChest`), `kick_dual`, `server_policy_dual` and
  `play_policy_dual`. `services_keys_ours` has no vanilla twin: it proves the profile-key handling against a mock discovery
  service (`MINECRAFT_API_DISCOVERY_HOST`). `RCON_DUAL_ONLY=name`, `JSONRPC_DUAL_ONLY=name`, `DEMO_DUAL_ONLY=name` and
  `KICK_DUAL_ONLY=name` run one scenario.
- World start dual (T2, `crates/mc-server/tests/oracle/world_start.rs`, runner `scripts/datafix-world-dual.sh`): the vanilla server
  and `mc-server` open identical copies of a corpus world (`scripts/datafix-worlds.sh`; worlds that older vanilla servers
  generated) in a normal start and as a world newer than the current data version, save and stop; the server directories are
  compared (chunks outside the loaded area exactly, inside it semantically; the module header lists what is normalised) and
  so are the log records that concern the world load. `legacy_users.rs` does the same for `ops.txt`, `white-list.txt`,
  `banned-players.txt`, `banned-ips.txt` and `players/<name>.dat` in offline mode. `world_upgrade.rs` runs `--forceUpgrade`,
  `--eraseCache` and `--recreateRegionFiles` on the 26.2 corpus worlds in both servers and compares the upgrade log and the
  world directories (chunk NBT exact), plus chunks whose compressed stream is damaged (skipped or aborting the upgrade). `DATAFIX_DUAL_WORLDS` (`supported`, `all` or
  ids), `DATAFIX_DUAL_KINDS` (`flat,normal`) and `WORLD_UPGRADE_KEEP=<dir>` select worlds and keep both directories.
- Data-fix coverage (`crates/mc-datafix/tests/oracle/Coverage.java`, the oracle's `coverage` verb, run by
  `scripts/datafix-corpus.sh coverage`): vanilla's `DataFixers` run over every fixture of `tests/fixtures/samples` with a watch
  around each fix, and `tests/fixtures/coverage.tsv` records per fix how many real and synthetic samples it changed. The
  `fix_coverage` golden fails on a fix no sample exercises unless `coverage_expected_unexercised.txt` lists it with a reason, on a
  listed fix that a sample exercises now (the list only shrinks), and on a sample fixture that is not measured or has no replay
  test. Regenerate `coverage.tsv` whenever the sample fixtures change.
- Advancement triggers: `golden/advancement_loot_guards.rs` lists, per trigger, the scenario that fires it and the criterion that
  scenario's fixture records as completed (`ADV PLAYER ADVANCEMENT CRITERION`); a trigger that no recorded completion proves, or that
  is not listed, fails the test. A trigger with no observation yet is listed with its reason.
- A third target, `tests/it/main.rs`, holds T0/T1 tests that need a process of their own because they change
  process-wide state that every other test of the crate would see (today only `mc-text`: `Language::inject` replaces
  the translations of the whole process). A crate adds it only for that reason. Tests that capture log lines, or that
  switch the working directory, stay in `golden`: the log helper keeps the lines per thread and the tests that
  use the working directory share one lock.
- Shared helpers go in `mod common;` (or `args_common`, `net_common`, ...) at the target root, with
  `#![allow(dead_code, reason = "...")]` stating why (not every module uses every helper). A helper two targets of one
  crate both need is split so each keeps only what it uses; the few exceptions load one helper file with `#[path]`.
- No `#[path]` attributes and no reaching into another crate's `tests/` directory. Review. Not met yet
  (`git grep -n '#\[path' -- 'crates/*/tests/*'` lists the sites): the golden target of `mc-game-core` loads helper modules
  from the golden target of `mc-game` (`command_common`, `common`, `decoration_common`, `net_common`), the oracle target
  of `mc-server` loads `net_common` from there, and
  the oracle targets of `mc-game`, `mc-game-core` and `mc-world` load helper modules of their own golden targets. The `mc-game-core` tests
  pass on the core crate alone but share their helpers and recordings with the tests that need the area crates; some
  modules read another crate's fixtures through `../<crate>/tests/fixtures`.
- Every `.rs` file under `tests/` is either a target or named by a `mod`. A file nobody declares is a silently
  dead test. Enforced by the orphan check in `scripts/check.sh`.
- Names: file or module is the vanilla area; golden replays are named `<behaviour>_matches_vanilla`. Review.
- Every `#[test]` asserts something, directly or through a helper that does. No print-only tests; reports belong
  in `examples/`. Review.

## 5. Fixtures and generators

A fixture is a recording of vanilla behaviour (or a hand-written expectation) that a golden test replays.
Generators stay next to the fixtures they produce: `*.java` (run with `scripts/vanilla-java.sh`, which compiles and
runs against the vanilla classpath in `ref/`), `*.py`, or `*.sh`.

**Headers.** Each generator starts with a comment containing

```
Usage: <how to run it>
Output: <path of the file it writes>
```

The headers are the source of truth: there is no hand-maintained fixture index. Enforced: `scripts/check.sh` fails on a
generator without both headers.

**Regeneration contract.**

- A generator rewrites only its own `Output:`, atomically (write a temp file, move it into place only on exit 0).
- Output is deterministic: no clock, no unseeded randomness, `Locale.ROOT` for formatting.
- The vanilla version is whatever `ref/` pins, currently Minecraft 26.3 (protocol 777, world version 5023,
  data pack format 121).
- Fixtures that are recordings of vanilla are rewritten only through `MC_UPDATE_FIXTURES=1`, and only inside the
  oracle target. This is the single variable for it; it counts only when set to `1`. Enforced: review.

**Format and size.** Fixtures are plain text, except inputs in a binary format that the code under test parses (Anvil
region files, `level.dat` and other NBT files, LZ4 frames, the `.bin` oracle inputs, a PKCS#12 key store); there is no Git
LFS. The one other exception is a generated fixture that is larger than 1 MB uncompressed: it is committed as `<name>.gz` (the full file name plus `.gz`, for example `snbt_golden.tsv.gz`; `gzip -9n`, so there is no
name or timestamp and the output is deterministic) and its `Output:` header names the `.gz` path. Smaller fixtures are not compressed. No tracked file may
exceed 8 MiB (`scripts/check.sh`).

**Provenance.** Fixtures may contain Mojang-derived output (names, ids, text). They are public but not relicensed;
the root `NOTICE` lists them. Hand-written fixtures and generator sources are ours, under the project's MIT
licence. `scripts/check.sh` also rejects tracked Mojang sources (`package net.minecraft.`), jars, class files,
images, audio and archives.

### Regenerating a fixture

```sh
scripts/setup-ref.sh                      # once; add --decompile for generators that compile against vanilla sources
crates/mc-core/tests/fixtures/java_collections/gen.sh   # run the generator named by Usage:
git diff --stat                           # review: only the expected Output changed, and the change makes sense
cargo test -p mc-core                     # the golden test still passes against the new file
git add <generator> <output>              # commit generator and output together
```

A fixture diff that nobody can explain is a bug in the generator or in the test, not something to commit.

## 6. Feature families

Every feature family registered through the `linkme` slots (crafting, commands, advancements and loot hosts, and
so on) needs at least one black-box test through its public entry point; for example, a 2x2 craft driven through
the inventory menu rather than by calling the recipe matcher. Review.

A stub in `src` that skips vanilla behaviour needs a matching `#[ignore = "deviation: ..."]` test, or a stated
absence of an oracle in the stub's doc comment. Review.

An entity that acts (a hit, an explosion, a block effect) is out of its storage slot while its code runs, so the code
it calls into only finds it when the call is wrapped in `ServerLevel::lend`. In debug builds, `MC_LEND_DETECTOR=<file>`
makes every hit whose attacker is out of the level and not lent append one line (`attacker <- victim | call path`) to
that file; run the golden tests with it set to list the call-outs that are still missing a wrap.

## 7. Untrusted input

Input from the network and from data packs is hostile. Existing corpora are replayed as ordinary tests:

- `crates/mc-protocol/tests/fixtures/components/corpus/` and `components/text/corpus.txt` (component codecs)
- `crates/mc-protocol/tests/fixtures/wire_limits.golden.gz` (length and depth limits on the wire)
- `crates/mc-nbt/tests/fixtures/snbt_golden.tsv.gz` (SNBT parsing, including malformed input)
- `crates/mc-game/tests/fixtures/commands/*corpus*.txt` and `commands_corpus/` (command parsing and execution errors)

Each pre-play listener phase (handshake, status, login, configuration) has one deterministic test that sends a
malformed client packet and asserts that the client is disconnected and the server keeps accepting connections, and a
panic-injection test covers the containment of a handler panic (`docs/standards.md` section 4). Review.

## 8. Known failures

`tests/known-failures.txt` is a temporary baseline of tests that fail on `main` today. One line per test:

```
<package> <test binary path> <test name>      e.g. mc-game tests/golden/main.rs attack_golden::game_modes_and_reach
```

The push and pull request CI skips these tests and the nightly `full` job runs them (section 9).

Rules: a change may only delete lines, never add them; a new failure is fixed, not listed. Flaky tests are not
listed there; fix or quarantine them with an `#[ignore = "..."]` and an issue. The file must be empty, and removed
from the tree, before 1.0. Review until the check exists; the file's presence is a release blocker.

## 9. CI

`.github/workflows/ci.yml` runs on pull requests and on pushes to `main`; its `ci-ok` job is the single required
status check and passes only when every job passed:

- `fmt`: `cargo fmt --all --check`.
- `clippy`: `cargo clippy --workspace --all-targets --all-features --locked --keep-going -- -D warnings`.
- `deny`: `cargo deny check licenses bans sources`.
- `doc`: `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --workspace --locked --keep-going` (broken documentation links).
- `hygiene`: `scripts/check.sh --fast` (shellcheck, ruff, syntactic slop and provenance checks).
- `commits`: `scripts/check-commits.sh` on the pull request title and commits (Conventional Commits).
- `test-build`: `cargo nextest archive --workspace --locked`, which builds every test binary once and uploads the
  archive as an artifact.
- `test`: four parallel shards (`--partition hash:N/4`) that download the archive and run
  `cargo nextest run --archive-file ... --workspace-remap . --profile ci` (T0/T1; profile in `.config/nextest.toml`:
  one retry, a hung test is killed after 10 minutes). The tests listed in `tests/known-failures.txt` and in
  `tests/slow-tests.txt` are excluded with the filterset that `scripts/nextest-filter.py` prints, so any failure in
  these jobs fails CI. A test that fails and then passes on the retry is reported as flaky (a warning annotation, not
  a failure); flaky tests are fixed, never listed.
- `doctest`: `cargo test --doc --workspace --locked` (nextest does not run doc-tests).
- `ref`: builds `ref/` from Mojang's download, or restores it from the cache keyed on the scripts that produce it.
  `clippy`, `test-build` and `doctest` need it (`mc-data`'s build script reads the extractor output); the `test`
  shards read it at run time.

`.github/workflows/nightly.yml` runs once a day and on demand, and never blocks a pull request:

- `advisories`: `cargo deny check advisories`. Advisories change without a code change; the accepted one is documented
  in `deny.toml`.
- `full`: the whole T0/T1 suite with `scripts/test-known-failures.py`, which runs `cargo test --no-fail-fast` for
  each workspace package including the slow goldens and the known failures that `ci.yml` skips. It fails only on a
  failure that `tests/known-failures.txt` does not list; a binary that aborts counts as a failure. It prints listed
  tests that now pass: delete their lines (section 8). A new failure is re-run once alone: a pass is reported as flaky
  (a warning, not a failure).
- `worlds`: restores the corpus worlds of the data fixer tests from the cache or generates them (section 10), once for
  the jobs below.
- `oracle`: the oracle tier, `scripts/test-known-failures.py --oracle --shard N/4 -- --test-threads=2`, in four parallel
  shards: every package's `tests/oracle` target is listed, the tests are sorted by package and name, and shard N runs
  every fourth one with `--ignored` and the same known-failures filter, with `MC_ACCEPT_EULA=1` and a `ref/` that was built
  with `--decompile` (the data fixer sample generator reads the vanilla sources). The shards together
  run each oracle test of every crate that has an oracle target once, among them the data fixer live suites (`mc-datafix`: the oracle and sample generator checks, the
  recordings, the tables, the file fixer records) and the world start, world upgrade and legacy user duals
  (`mc-server`). It must pass before a release is tagged.

`.github/workflows/weekly.yml` runs once a week and on demand (`gh workflow run weekly.yml`, input `normal_worlds` adds
the seed 12345 worlds): `scripts/datafix-world-dual.sh --all`, the world dual of section 10 over every version of
`versions.tsv`. It reads the same world cache as the nightly run and, like it, never blocks a pull request.

`tests/slow-tests.txt` lists the tests that take 20 s or more (the slow worldgen and structure goldens, the data fixer
corpus replays that are long) in the same key format; they run in `full` only. Like known failures they are skipped by
the push and pull request run, so a change to worldgen or to a data fix shows up the next night.

Run the full filter locally with `scripts/test-known-failures.py [-p <crate>]...`. To run CI on a branch without a pull
request: `gh workflow run ci.yml --ref <branch>`.

## 10. Data fixer corpus

The data fixers (`mc-datafix`, the file fixer in `mc-world`, the world upgrader) are proved by running the same input
through vanilla's classes and through ours. Old worlds are the inputs; they are too large and too much Mojang output to
commit, so the proof has tiers (section 1) that need different amounts of them:

| Tier | What it runs | Needs | Where |
|---|---|---|---|
| T0 | replay of recorded samples: `samples_step_*` (one fix version at a time), `samples_corpus` (chunks, level data, saved data and the rest of the constants that old servers wrote, and inputs generated from the schemas), `samples_json_files`, the type hashes, the chain listing, the lookup tables; the file fixer on the minimal worlds and on a world interrupted halfway (`crates/mc-world/tests/golden/file_fix.rs`, which also replays the corpus worlds it finds in the cache and reports the others as not compared) | committed fixtures only | `crates/mc-datafix/tests/golden/`, fixtures in `crates/mc-datafix/tests/fixtures/` and `crates/mc-world/tests/fixtures/filefix/` |
| T2 | vanilla's `DataFixers` in a JVM against ours: the oracle self-checks, the sample generator (it harvests its vocabulary from the vanilla sources in `ref/`), the recordings against the live chain, the tables, the file fixer records | `ref/` with the decompiled sources (`scripts/setup-ref.sh --decompile`), JDK 25 | `crates/mc-datafix/tests/oracle/` |
| T2 dual | the vanilla server and `mc-server` open identical copies of a corpus world (normal start, a world newer than the current version, `--forceUpgrade`, `--eraseCache`, `--recreateRegionFiles`) and the legacy user lists, then the server directories and the log lines are compared | the corpus worlds, `ref/`, JDK 25, `MC_ACCEPT_EULA=1` | `crates/mc-server/tests/oracle/{world_start,world_upgrade,legacy_users}.rs`, `scripts/datafix-world-dual.sh` |

T0 runs in every `cargo test` and in push CI. The corpus replays (`samples_corpus::*`) are the heaviest of it (under 0.7 GB per test); the `datafix-corpus` test
group of `.config/nextest.toml` runs at most two of them at a time in the default and the `ci` profile; CI runs them, except the
largest (`tests/slow-tests.txt`), which only the nightly `full` job runs.
Proof is differential: the same input goes to both implementations, because an old server's world differs from run to
run (clocks, entity UUIDs, tick counters; `crates/mc-datafix/tests/fixtures/world_run_dependent.txt` says what).

### Where the worlds live: never in git

Everything an old server wrote or shipped stays in `$MC_DATAFIX_CACHE` (default `${XDG_CACHE_HOME:-$HOME/.cache}/mc-datafix-corpus`),
outside the working tree:

- `jars/<id>.jar`: the server of release `<id>`, downloaded from Mojang and checked against the SHA-1 of the version manifest;
- `worlds/<id>` (flat, scripted) and `worlds/<id>-normal` (seed 12345, with structures): the world as the server left it;
- `json/`, `logs/`: the version metadata and the server logs of each run;
- `corpus/`: the working files of `scripts/datafix-corpus.sh` (candidate samples, requests, records; a few GB).

No jar and no complete world of an old server is committed (only the minimal worlds below); `scripts/check.sh` rejects jars, class files and
archives and any tracked file over 8 MiB, and the root `NOTICE` says that what Mojang's software wrote is not relicensed.
What is committed is derived from them and small: the sample fixtures (`fixtures/samples/corpus_*.tsv.gz`, under 6 MiB together),
`fixtures/versions.tsv` (id, DataVersion, jar SHA-1, Java major, whether the jar runs), `fixtures/world_hashes/<id>.txt` (SHA-256 of
every world file), the file fixer records and the minimal worlds (a few chunks of the region files cut out of the seed 12345 worlds,
`crates/mc-world/tests/fixtures/filefix`) and the scripts that make the worlds
(`crates/mc-datafix/tests/corpus/eras/*.txt`, one console command per line).

A dual test that needs a world and finds none panics with the command that makes it (section 2).

### Regeneration

Run in this order; each step reads the cache and the previous step's output, and a rerun with the same inputs gives
identical files:

```sh
export MC_ACCEPT_EULA=1                  # the generator starts the old servers: running them accepts the EULA in your name
export MC_OLD_JAVA=/path/to/jdk-8        # releases up to 1.16.5 need a JDK 8; none is ever downloaded
scripts/datafix-worlds.sh                # jars and worlds of every listed version (or ids: scripts/datafix-worlds.sh 1.12.2 26.2);
                                         # rewrites versions.tsv and world_hashes/; --list shows what is cached
scripts/datafix-corpus.sh all            # extract, synth, select, record: rewrites fixtures/samples/corpus_*.tsv.gz
scripts/datafix-oracle.sh --regen        # fixes.tsv, schema types, tables, table accessors, file fixer records of the minimal worlds
scripts/datafix-oracle.sh --regen-filefix-corpus   # the file fixer records of the corpus worlds
git diff --stat                          # review (section 5): only expected files, sizes within budget
cargo nextest run -p mc-datafix -E 'test(/^samples_corpus::/)'   # replay green
```

Rules:

- The sample fixtures are a function of the worlds in the cache and the seed. A different set of worlds (new era script, new
  version, a regenerated world) gives a different fixture: a reviewed diff, never an automatic update. A diff after
  recording twice from the same worlds is a bug (an unseeded generator), to be fixed there and not committed.
- Recording needs the real vanilla classes, so only `scripts/datafix-oracle.sh` and the scripts that call it write
  recordings; `MC_UPDATE_FIXTURES=1` works inside the oracle target only (section 5).
- A change to `datafix-worlds.sh`, an era script or `versions.tsv` changes the cache key of the CI worlds (below) and so
  regenerates them; regenerate the fixtures in the same commit.

### Running the live suites

```sh
MC_ACCEPT_EULA=1 scripts/test-known-failures.py --oracle -p mc-datafix -- --test-threads=2           # T2, all
MC_ACCEPT_EULA=1 scripts/test-known-failures.py --oracle --shard 2/4 -- --test-threads=2             # one nightly shard
MC_ACCEPT_EULA=1 scripts/datafix-world-dual.sh [--nightly | --all | ID...] [--normal-worlds]         # the world dual
```

`scripts/datafix-world-dual.sh` takes the worlds from the cache; without arguments it opens every corpus version that
`mc-server` accepts (at or above its start-up floor), `--nightly` the four of 1.12.2, 1.16.5, 1.20.4 and 1.21.11, `--all`
every version of `versions.tsv`. A failure prints the first differing log line and the differing files and chunks.
`--keep DIR` keeps both server directories of every run.

### CI

`nightly.yml` and `weekly.yml` get the worlds from `.github/actions/datafix-worlds`: it restores `~/.cache/mc-datafix-corpus`
(a cache named `datafix-corpus-<hash>`, the hash of `scripts/datafix-worlds.sh`, the era scripts and `versions.tsv`), and on a
miss installs JDK 8, 17, 21 and 25, runs `scripts/datafix-worlds.sh` for every listed version and saves the cache. The cold
run takes hours, the later ones minutes. The nightly `oracle` job runs the T2 suites in four shards (section 9); the weekly
run is the `--all` dual over every version. The jars are downloaded from Mojang on the runner; none is uploaded as an artifact.
