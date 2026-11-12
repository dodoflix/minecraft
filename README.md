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

Skipped vanilla behaviour is recorded where it is skipped, in a `Not implemented:` comment in the code
(`git grep 'Not implemented:'`). That is the authoritative list. This section only gives an overview by category; each
bullet links to the `parity` issue that tracks the category, and known differences from vanilla are tracked in the other
[`parity` issues](https://github.com/dodoflix/minecraft/labels/parity).

- **Entity types without behaviour:** wither, ender dragon, illagers, minecarts, display entities, projectiles such as ender pearls and potions, and several animals. They can be summoned and loaded as inert placeholder entities that keep their NBT, but they have no AI, physics, interaction or drops. ([#13](https://github.com/dodoflix/minecraft/issues/13))
- **Mob AI and spawning:** skeletons never shoot, zombies do not use spears or cure villagers, block spawners skip the mob hooks, and leads, dolphin treasure and piglin anger are missing. ([#14](https://github.com/dodoflix/minecraft/issues/14))
- **Items:** several items do nothing when used (ender pearl, egg, fishing rod, minecarts, brush, carrot on a stick boost, and more). ([#15](https://github.com/dodoflix/minecraft/issues/15))
- **Dispensers:** armor stands, boats, water bottles, shulker boxes, TNT, the arrow, snowball, experience bottle, firework and wind charge projectiles and the component defaults (equipment, spawn eggs, sulfur cube swallowing) exist; every other item is dropped. ([#16](https://github.com/dodoflix/minecraft/issues/16))
- **Workstations and menus:** the crafting table, furnaces, stonecutter, loom, smithing table, cartography table and the crafter open no menu, and the recipe book cannot place recipes. ([#17](https://github.com/dodoflix/minecraft/issues/17))
- **Blocks:** the block behaviours are ported; what is left needs entities, items or menus that do not exist yet: copper golem statues cannot turn back into golems, and a carved pumpkin on a copper block, like a wither skull pattern, summons only an inert placeholder entity; beehives release inert placeholder bees; fences cannot hold a lead; a detector rail gives no comparator signal for container minecarts; sign `run_command` clicks do nothing; and dropping an item is not counted as a statistic. ([#13](https://github.com/dodoflix/minecraft/issues/13), [#14](https://github.com/dodoflix/minecraft/issues/14), [#15](https://github.com/dodoflix/minecraft/issues/15), [#22](https://github.com/dodoflix/minecraft/issues/22))
- **Commands, advancements and loot:** advancement data is loaded and tracked, but `/advancement` and the `advancements` selector do not reach it; loot contexts cannot be built by the commands, and some criteria are never fired. ([#22](https://github.com/dodoflix/minecraft/issues/22))
- **Raids and custom spawners:** raids spawn no raiders, omen effects start no raid, and only the phantom spawner runs. ([#19](https://github.com/dodoflix/minecraft/issues/19))
- **World upgrade and data fixers:** no data fix is ported, so old worlds are not upgraded. ([#20](https://github.com/dodoflix/minecraft/issues/20))
- **Server features:** server links, a moderation text filter, the bonus chest and some spectator behaviour are missing. ([#21](https://github.com/dodoflix/minecraft/issues/21))

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
failure that the file does not list; the nightly CI run uses it. A cold debug test build is slow and
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
