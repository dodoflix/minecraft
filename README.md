# A Rust implementation of the Minecraft Java Edition 26.3 server

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
- **Working:** the vanilla commands and Brigadier, DataFixerUpper and data fixers
  ported from Mojang's libraries, plus the entity, block and item behaviour
  implemented so far in the `mc-game-*` crates.
- **In progress:** world generation. Noise, density functions, biome sources and
  state-only chunk generation exist; behaviour is checked against vanilla output
  where fixtures exist.
- **Known gap:** the command-host seams in
  `crates/mc-game-core/src/commands/host.rs` evaluate the `advancements` and
  `predicate` selector options against empty registries. Advancement data and loot
  predicates are not loaded yet, so those options never match.
- **Known gap:** crafting never produces a result.
  `crates/mc-game-core/src/world/inventory/crafting.rs` has a recipe-source hook
  (`set_crafting_recipe_source`) that nothing installs yet.
- **Layout:** gameplay is split over `mc-game-core` and the `mc-game-*` area crates, re-exported by the `mc-game` facade;
  see [docs/crate-split.md](docs/crate-split.md).

## Prerequisites

- Linux (other platforms are not supported)
- Rust, installed with [rustup](https://rustup.rs); `rust-toolchain.toml` selects the
  toolchain and components
- JDK 25 including `javac`
- `curl`, `unzip`, `jq`, `sha1sum`
- a C toolchain (compiler and linker)
- roughly 300 MB of disk for `ref/` (more with `--decompile`)

## Setup

```sh
scripts/setup-ref.sh
```

This downloads the vanilla 26.3 server from Mojang into the gitignored `ref/`,
unpacks it, runs the vanilla data generator and then the Java data extractor.
`mc-data` reads `ref/` in its `build.rs`, so run it before building. Steps whose output already exists are
skipped; the options (`--decompile`, `--libs`, `--force`, `--no-extract`) are
documented at the top of the script.

By running the script you accept Mojang's EULA and terms yourself. Never
redistribute `ref/` or a build that embeds it.

## Build

```sh
cargo build -p mc-server --release
```

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

This runs the hermetic tests and the tests that read `ref/`. The oracle tier (which
runs the real vanilla server or vanilla classes to produce or check golden output)
and the real-client end-to-end tests are opt-in. See [docs/testing.md](docs/testing.md)
for the tiers and how to run each.

## Releases

Releases are source-only. Builds embed data derived from Mojang's server, so this
project publishes no binaries.

## Documentation

- [ARCHITECTURE.md](ARCHITECTURE.md): goals, conventions, layout of `ref/`
- [CONTRIBUTING.md](CONTRIBUTING.md): how to contribute
- [docs/standards.md](docs/standards.md): code and review standards
- [docs/testing.md](docs/testing.md): test tiers
- [NOTICE](NOTICE): what the licence covers and what it does not
- [LICENSE](LICENSE): MIT
