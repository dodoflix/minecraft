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
  implemented so far in the `mc-game-*` crates (see the gaps below).
- **In progress:** world generation. Noise, density functions, biome sources and
  state-only chunk generation exist; behaviour is checked against vanilla output
  where fixtures exist.
- **Known gaps** (vanilla behaviour that is skipped; each skip is also documented at the item that skips it):
  - Advancements: no advancement data is loaded. The `advancements` selector option never matches, `/advancement`
    grants and revokes nothing, advancement rewards (recipes, functions) are not given, and several criteria
    (opening a loot container, killing near a sculk catalyst, summoning a wither, avoiding a vibration) are not fired.
  - Loot contexts: command number providers (`/data`, `/execute store` computing a provider) and the `predicate`
    selector option cannot be evaluated; explorer maps get no target decoration.
  - Crafting: no recipe is matched, so every crafting result slot is empty (including the 2x2 grid); the recipe book
    cannot place a recipe; campfires do not cook food; signs cannot be dyed, inked or waxed; no statistics are
    counted for tuning a note block or planting a flower pot.
  - Mobs and raids: raids and the Bad Omen and Raid Omen effects do nothing; piglins do not get angry when a player
    opens a chest or breaks gold; dolphins cannot find treasure; zombie villagers cannot be cured; there is no lead
    knot, no leashing of boats and no spear-use goal; lightning traps spawn no skeleton horses; there are no bees
    (hives release occupants but no bee behaviour exists), villagers or wandering traders with their trading goals;
    evokers and illusioners cast no spells.
  - Items and projectiles: splash and lingering potions have no projectile entity; chest boats and chest minecarts
    open no menu; scaffolding placement has no stability distance.
  - Redstone and blocks: the detector rail does not feed a comparator; shelves have no powered side-chain connection.
  - Server: structure types that are not ported do not generate, world data is not upgraded through the file fixer,
    server links and text filtering are not sent, the bonus chest is not generated, and a spectator cannot use a
    portal teleport or leave a bed on teleporting to a target; no data fix of `DataFixers.addFixers` is ported, so
    old worlds are not upgraded, and there is no per-version fixer stepping.
- **Layout:** gameplay is split over `mc-game-core` and the `mc-game-*` area crates, re-exported by the `mc-game` facade;
  see [ARCHITECTURE.md](ARCHITECTURE.md).

## Prerequisites

- Linux (other platforms are not supported)
- Rust, installed with [rustup](https://rustup.rs); `rust-toolchain.toml` selects the
  toolchain and components
- JDK 25 or newer, including `javac`
- `curl`, `unzip`, `jq`, `sha1sum`
- a C toolchain (compiler and linker)
- a few hundred MB of disk for `ref/` (more with `--decompile`) and several GB for `target/` once you build the tests

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

By running the script you accept Mojang's EULA and terms yourself. Never
redistribute `ref/` or a build that embeds it.

## Build

```sh
cargo build -p mc-server --release
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
