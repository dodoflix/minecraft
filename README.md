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
- **Working:** the vanilla commands and Brigadier, plus the entity, block and item behaviour
  implemented so far in the `mc-game-*` crates (see the known gaps below). The DataFixerUpper
  library core is ported, but no schema or data fix is registered yet.
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

- **Commands, advancements and loot:** each of the 87 commands has scenarios that compare its feedback with vanilla recordings, as do selectors, advancement triggers and rewards, loot tables and statistics; commands and selectors that cross dimensions are replayed with several levels. Not compared: outputs that carry elapsed time, file paths or network lookups (`perf stop`, `debug stop`, profile lookups by id or name in `fetchprofile`), which only the nightly dual-server corpora cover, and seven hand-driven test files that run no commands. Text with selector, score or NBT contents in `set_name`/`set_lore` is resolved only where the roll has a server (commands, not mob deaths or block breaks), a bad inline `/place feature` config reports only the first error of vanilla's DFU wording, and the `/gamerule` literals sit in a different order in the command tree than vanilla's. ([#22](https://github.com/dodoflix/minecraft/issues/22))
- **Floating point:** Java's `Math.sin`/`Math.cos` (HotSpot intrinsics) differ from the port in the last bit for about 0.15% of inputs, which can shift guardian, fox, ravager, TNT and random-position motion by one ulp. ([#32](https://github.com/dodoflix/minecraft/issues/32))
- **World upgrade and data fixers:** no data fix is ported, so old worlds are not upgraded. ([#20](https://github.com/dodoflix/minecraft/issues/20))
- **Server features:** the moderation text filter service (`text-filtering-config`) is not ported; chat and sign text pass through unfiltered, as on a vanilla server without that setting. ([#21](https://github.com/dodoflix/minecraft/issues/21))

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
