# A Rust implementation of the Minecraft Java Edition 26.3 server

[![ci](https://github.com/dodoflix/minecraft/actions/workflows/ci.yml/badge.svg?branch=main)](https://github.com/dodoflix/minecraft/actions/workflows/ci.yml)

**NOT AN OFFICIAL MINECRAFT PRODUCT. NOT APPROVED BY OR ASSOCIATED WITH MOJANG OR MICROSOFT.**

This is a port of the vanilla Minecraft Java Edition 26.3 dedicated server to Rust
(protocol 777, world version 5023, data pack format 121). The goal is that its
observable behaviour matches the vanilla server: wire protocol, world files,
world generation, game mechanics, commands and data packs. It is in early
development and is not a complete or production-ready server. The project does not
have its final name yet.

## Status

- **Working:** the wire protocol codecs, NBT/SNBT, chat components, Anvil region and
  level storage, the light engine, and the dedicated-server start-up flow
  (`server.properties`, `eula.txt`, command-line options).
- **Working:** the vanilla commands and Brigadier, the DataFixerUpper machinery
  ported from Mojang's libraries, plus the entity, block and item behaviour
  implemented so far in the `mc-game-*` crates (see the known gaps below).
- **In progress:** world generation. Noise, density functions, biome sources and
  state-only chunk generation exist; behaviour is checked against vanilla output
  where fixtures exist.

## Layout

Gameplay is split over `mc-game-core` and the `mc-game-*` area crates, re-exported by the
`mc-game` facade; see [ARCHITECTURE.md](ARCHITECTURE.md).

## Known gaps

Vanilla behaviour that a player, a client, a command user or a data pack author can observe and that is skipped.
Each gap is also documented at the code that skips it; smaller internal gaps are only documented there.

### Entities that do not exist

These entity types have no implementation: they cannot be spawned, summoned or loaded.

- Illagers and raiders: pillager, vindicator, evoker, illusioner, ravager and witch. Raids exist (tracking, waves,
  boss bar) but spawn no raiders.
- Bosses: wither, ender dragon and end crystal.
- Animals and creatures: allay, armadillo, bee (hives cannot release bees), camel and camel husk, goat, happy ghast,
  nautilus and zombie nautilus, sniffer, copper golem, and the wandering trader.
- All minecarts: minecart, chest, furnace, hopper, TNT, command block and spawner minecart.
- Poplar boat and poplar chest boat.
- Display entities (block, item, text), marker, interaction and leash knot.
- Projectiles: thrown egg, ender pearl, eye of ender, splash and lingering potion, fishing bobber.

### Mobs and spawning

- Skeletons, strays and bogged never shoot: `RangedBowAttackGoal` exists but is not wired in, and a skeleton holding a
  bow has no attack goal.
- Zombies and zombified piglins do not use spears: `SpearUseGoal` exists but is never constructed.
- Zombified piglins do not reset their anger target: `ResetUniversalAngerTargetGoal` is never added.
- Zombie villagers cannot be cured (the timer starts and never finishes), and zombies do not turn villagers into
  zombie villagers.
- Mobs created by block spawners skip the `Mob` hooks (`finalizeSpawn`, equipment, spawn rules).
- Piglins do not get angry when a player opens a chest or breaks gold.
- Dolphins cannot find treasure.
- Mobs cannot be tied to a lead knot, and boats cannot be leashed.
- Lightning traps spawn no skeleton horse.
- The patrol, cat, village siege and wandering trader spawners do not exist; only the phantom spawner runs.
- The Bad Omen and Raid Omen effects do not start a raid, and the Oozing and Infested effects spawn no slimes or
  silverfish.

### Items

- The brush does nothing.
- These items do nothing when used: ender pearl, eye of ender, egg, fishing rod, wind charge, flint and steel, fire
  charge, minecart items, end crystal and lead on a fence.
- Dyes do not dye sheep or signs, ink and glow ink sacs do not ink signs, and honeycomb does not wax signs.
- Shears do not carve a pumpkin.
- A spear lunge damages nobody.
- Scaffolding placement has no stability distance.

### Blocks and menus

- No block opens a menu for the crafting table, furnaces (furnace, blast furnace, smoker), stonecutter, loom,
  cartography table, smithing table, crafter, enchanting table, anvil or grindstone. The anvil, grindstone and
  enchantment menus exist but nothing opens them; the other menus do not exist.
- Furnaces, campfires and the crafter have no block entity, so campfires cook nothing.
- The 2x2 crafting grid of the player inventory never produces a result, and the recipe book cannot place a recipe.
- Shelves have no block entity, item storage, powered state, side-chain connection or comparator signal.
- The copper golem statue has no block entity, pose change or comparator signal.
- Sign click commands are not run.
- Statistics are not counted for tuning a note block, planting a flower pot, dropping an item or using a map.

### Commands, advancements and loot

- No advancement data is loaded: the `advancements` selector option never matches, `/advancement` grants and revokes
  nothing, and advancement rewards (recipes, functions) are not given.
- Several advancement criteria are never fired: opening a loot container, killing near a sculk catalyst, summoning a
  wither and avoiding a vibration.
- Command number providers (`/data`, `/execute store` computing a provider) and the `predicate` selector option
  cannot be evaluated, because loot contexts cannot be built.
- The `exploration_map` loot function (explorer map trades and chests) returns its map untouched: no structure search
  and no map data.
- `/data` never produces `SelectedItem`, and `/teleport` does not stop a mob's navigation.

### Server

- World data is not upgraded: no data fix of `DataFixers.addFixers` is ported, and there is no per-version fixer
  stepping or file fixer, so old worlds are not upgraded.
- Server links are not sent, and text filtering uses the dummy filter (no moderation endpoint).
- The bonus chest is not generated.
- A spectator cannot use a portal teleport, and a spectator that teleports to a target does not leave its bed.

## Prerequisites

- Linux (other platforms are not supported)
- Rust, installed with [rustup](https://rustup.rs); `rust-toolchain.toml` selects the
  toolchain and components
- JDK 25 or newer, including `javac`
- `curl`, `unzip`, `jq`, `sha1sum`
- a C toolchain (compiler and linker)
- a few hundred MB of disk for `ref/` (more with `--decompile`), about 70 MB more for the `libraries/`, `versions/` and
  `logs/` directories that the setup script leaves in the working directory (see Setup), and several GB for `target/`
  once you build the tests

## Setup

```sh
scripts/setup-ref.sh
```

This downloads the vanilla 26.3 server from Mojang into the gitignored `ref/`,
unpacks it, runs the vanilla data generator and then the Java data extractor.
`mc-data` reads `ref/` in its `build.rs`, so run it before building. Steps whose output already exists are
skipped; the options (`--decompile`, `--libs`, `--force`, `--no-extract`) are
documented at the top of the script.

To read the vanilla source that this project ports, or to run the oracle and
fixture-regeneration scripts, run `scripts/setup-ref.sh --decompile`: it decompiles
the server jar into `ref/src/net/minecraft/`. Without it only the data and the
reports exist under `ref/`.

The vanilla data generator unpacks the server bundle into the current directory, so run the script from the repository
root, where `libraries/`, `versions/` and `logs/` are gitignored; from any other directory they are left there, untracked.

By running the script you accept Mojang's EULA and terms yourself. Never
redistribute `ref/` or a build that embeds it.

## Build

```sh
cargo build -p mc-server --release --locked
```

A cold release build takes a few minutes, and compiling `mc-game-core` needs a lot of
memory; on a small machine set `CARGO_BUILD_JOBS`.

## Run

```sh
target/release/mc-server [options]
```

The server follows the vanilla start-up order and works in the current directory.
It writes logs to `logs/`, reads or creates `server.properties`, and reads
`eula.txt`. On the first run `eula.txt` is created with `eula=false` and the server
exits; change it to `eula=true` once you agree to the
[Minecraft EULA](https://aka.ms/MinecraftEULA), then start again.

Options are the vanilla ones: `--help` prints them, among them `--port`,
`--world`, `--universe`, `--demo`, `--safeMode`, `--forceUpgrade` and
`--initSettings` (create `server.properties` and `eula.txt`, then quit). Like the
vanilla server, startup errors are logged and the process still exits with status 0.

## Tests

```sh
cargo test --workspace
```

This runs the hermetic tests and the tests that read `ref/`. It currently reports the
failures listed in [tests/known-failures.txt](tests/known-failures.txt), so it does not
pass cleanly. `scripts/test-known-failures.py` runs the same tests and fails only on a
failure that the file does not list; CI uses it. A cold debug test build is slow and
takes several GB under `target/`.

The oracle tier (which
runs the real vanilla server or vanilla classes to produce or check golden output)
and the real-client end-to-end tests are opt-in. See [docs/testing.md](docs/testing.md)
for the tiers and how to run each.

## Releases

Releases are source-only. Builds embed data derived from Mojang's server, so this
project publishes no binaries.

## Documentation

- [ARCHITECTURE.md](ARCHITECTURE.md): goals, conventions, layout of `ref/`, crates
- [CONTRIBUTING.md](CONTRIBUTING.md): how to contribute
- [SECURITY.md](SECURITY.md): how to report a vulnerability
- [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md): expected behaviour
- [docs/standards.md](docs/standards.md): code and review standards
- [docs/testing.md](docs/testing.md): test tiers
- [NOTICE](NOTICE): what the licence covers and what it does not
- [LICENSE](LICENSE): MIT
