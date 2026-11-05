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
| T2 JVM oracle | `ref/`, JDK 25, `MC_ACCEPT_EULA=1`, and a spawned vanilla JVM and/or our `mc-server` | no, all `#[ignore]`d | `tests/oracle/` target of a crate |
| T3 real client | GPU, `gamescope`, `xdotool`, client download | no, manual | [`tests/e2e/README.md`](../tests/e2e/README.md) |

T0 and T1 spawn no processes. Building the workspace already needs `ref/` and JDK 25, because the build script of
`mc-data` reads the extractor output under `ref/`. `scripts/setup-ref.sh` downloads the vanilla
server from Mojang; you accept Mojang's EULA yourself (section 3).

**Contract.** `cargo test --workspace` runs T0 and T1 only, and it must be green on a clean checkout after
`scripts/setup-ref.sh`. `cargo nextest` may be used as a faster runner, but the documented contract is
`cargo test --workspace`. Enforced by CI once the lanes in section 9 are wired.

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
an `oracle` target has the `oracle:` class. Ignores outside it are not all in one of these classes yet (some still
start with `known deviation:` or `known limit:`).

## 2. A missing prerequisite fails, it does not skip

A test whose prerequisite is absent panics with the command that fixes it, for example
`ref/ is missing: run scripts/setup-ref.sh`. A green run must mean the check ran.

`eprintln!("skipping ...")` followed by an early return is forbidden in `crates/*/tests`. Enforcement will be a
syntactic check in `scripts/check.sh`; until it lands, review enforces it.

The tree meets this rule. Two cases needed work:

- Tests that need our server binary use `env!("CARGO_BIN_EXE_mc-server")`, which Cargo only defines for the
  integration tests of package `mc-server`. They live in `crates/mc-server/tests/oracle`, which builds the binary
  itself. There is no `MC_SERVER_BIN` variable for tests.
- Tests that read a vanilla-generated world get it from `mc_testkit::worldref()`. The first call starts the vanilla
  server once (seed 12345, spawn area, clean stop) and leaves the world in `run/servers/worldref`; later calls and
  other test processes reuse it. Delete that directory to regenerate it. Because generating it spawns a JVM, these
  tests are T2.

## 3. EULA

T2 and T3 start the real vanilla server, which requires accepting the
[Minecraft EULA](https://www.minecraft.net/en-us/eula). Running them is your acceptance, not the project's.

Rule: no helper or script writes `eula=true` unless `MC_ACCEPT_EULA=1` is set; without it they print the EULA URL
and exit non-zero. CI sets the variable explicitly for the oracle lane. Enforced by `scripts/check.sh` (planned:
`eula=true` may appear only inside the guarded helper) and review. In Rust the guarded helper is `mc-testkit`
(below); `scripts/vanilla-server.sh` applies the same rule.

### The server helper

Every test that needs a live server starts it through `mc_testkit::Server::vanilla()` (the vanilla JVM) or
`Server::ours(bin)` (our `mc-server`). There is no second launcher. The crate has no workspace dependencies, so any
crate can use it as a dev-dependency.

- The returned guard owns the child. Dropping it, also while the test panics, kills the child's whole process group,
  and the kernel kills the child when the test process dies (`kill -9` included).
- The port is chosen by the OS and passed to the server; if another process takes it first, the start is retried with
  a new port. No fixed ports.
- Readiness is the `Done (` log line plus an answered status ping, with a timeout. No fixed sleeps after start.
- Each server runs in its own temporary directory (or one the test passes with `.dir(..)`), never in `run/servers/<name>`.
- A missing `java`, `ref/server.jar` or `MC_ACCEPT_EULA=1` makes `start()` return an error that names the fix; tests
  `expect` it and fail.

## 4. Layout

- A test lives in the lowest crate where it passes with identical results. Integration goldens live in `mc-game`,
  the crate that wires everything together. Review.
- Few test targets per crate: `tests/golden/main.rs` for T0/T1, `tests/oracle/main.rs` for T2, topics as modules
  (`mod args;`, `mod chunk_world;`, ...). A module keeps the name of the vanilla area it tests, so a test is
  `<module>::<test>` in the output. Only `mc-server` has nothing but an oracle target; crates without T2 tests have
  only `golden`. Current targets: `golden` in `mc-brigadier`, `mc-core`, `mc-data`, `mc-datafix`, `mc-dfu`,
  `mc-game`, `mc-game-core`, `mc-light`, `mc-nbt`, `mc-protocol`, `mc-text`, `mc-world`, `mc-worldgen`; `oracle` in
  `mc-game`, `mc-game-core`, `mc-light`, `mc-protocol`, `mc-server`, `mc-world`. Run one module with
  `cargo test -p mc-game --test golden args::`.
- Shared helpers go in `mod common;` (or `args_common`, `net_common`, ...) at the target root, with
  `#![allow(dead_code, reason = "...")]` stating why (not every module uses every helper). A helper two targets of one
  crate both need is split so each keeps only what it uses; the few exceptions load one helper file with `#[path]`.
- No `#[path]` attributes and no reaching into another crate's `tests/` directory. Review. Not met yet: the golden
  target of `mc-game-core` loads the helper modules of `mc-game`'s golden target (`common`, `args_common`,
  `net_common`, `decoration_common`) and reads fixtures from `mc-game`, because these tests pass on the core crate alone
  but share their helpers and recordings with the tests that need the area crates; `mc-game`'s oracle target and
  `mc-world`'s oracle target load three helper modules of their golden target; and some modules read another crate's
  fixtures through `../<crate>/tests/fixtures`.
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

The headers are the source of truth. A fixture index is derived from them, not maintained by hand; the check that
derives it and fails on a generator without headers or a missing `Output:` is planned for `scripts/check.sh`. Some
older generators do not have headers yet.

**Regeneration contract.**

- A generator rewrites only its own `Output:`, atomically (write a temp file, move it into place only on exit 0).
- Output is deterministic: no clock, no unseeded randomness, `Locale.ROOT` for formatting.
- The vanilla version is whatever `ref/` pins, currently Minecraft 26.3 (protocol 777, world version 5023,
  data pack format 121).
- Fixtures that are recordings of vanilla are rewritten only through `MC_UPDATE_FIXTURES=1`, and only inside the
  oracle target. This is the single variable for it. The ad-hoc `ENTITY_DUAL_UPDATE`, `PORTAL_DUAL_UPDATE` and
  `WC_DUAL_UPDATE` variables still exist and are being replaced. Enforced by a planned grep in `scripts/check.sh`.

**Format and size.** Fixtures are plain text: no `.gz` for regenerated files, no Git LFS. No tracked file may exceed
8 MiB (`scripts/check.sh`). Files over 1 MiB need a sentence in the PR explaining why.

**Provenance.** Fixtures may contain Mojang-derived output (names, ids, text). They are public but not relicensed;
the root `NOTICE` lists them. Hand-written fixtures and generator sources are ours, under the project's MIT
licence. `scripts/check.sh` also rejects tracked Mojang sources (`package net.minecraft.`), jars, class files,
images, audio and archives.

### Regenerating a fixture

```sh
scripts/setup-ref.sh                      # once; needs JDK 25
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

## 7. Untrusted input

Input from the network and from data packs is hostile. Existing corpora are replayed as ordinary tests:

- `crates/mc-protocol/tests/fixtures/components/corpus/` and `components/text/corpus.txt` (component codecs)
- `crates/mc-protocol/tests/fixtures/wire_limits.golden` (length and depth limits on the wire)
- `crates/mc-nbt/tests/fixtures/snbt_golden.tsv` (SNBT parsing, including malformed input)
- `crates/mc-game/tests/fixtures/commands/*corpus*.txt` and `commands_corpus/` (command parsing and execution errors)

Planned and not written yet: one deterministic test per listener phase (handshake, status, login, configuration,
play) that sends a malformed client packet and asserts that the client is disconnected and the server keeps
running. Review until they exist.

## 8. Known failures

`tests/known-failures.txt` is a temporary baseline of tests that fail on `main` today. One line per test:

```
<package> <test binary path> <test name>      e.g. mc-game tests/golden/main.rs attack_golden::damage_matches_vanilla
```

Rules: a change may only delete lines, never add them; a new failure is fixed, not listed. Flaky tests are not
listed there; fix or quarantine them with an `#[ignore = "..."]` and an issue. The file must be empty, and removed
from the tree, before 1.0. Review until the check exists; the file's presence is a release blocker.

## 9. CI

The deterministic lane is: `cargo fmt --all --check`, `cargo clippy --workspace --all-targets --all-features
--locked -- -D warnings`, `cargo deny check licenses bans sources`, `scripts/check.sh`, and
`cargo test --workspace` (T0/T1). Run `scripts/check.sh` locally before pushing; `scripts/check.sh --fast` skips
clippy.

The oracle tier (`cargo test --workspace --test oracle -- --ignored`, with `MC_ACCEPT_EULA=1`) runs on pushes to
`main` and nightly. It must pass before a release is tagged. It is not part of the required pull request checks.

Current state: `.github/workflows/ci.yml` only runs the scheduled and on-demand full build and test of the
workspace, `cargo test --workspace` followed by the oracle tier with `MC_ACCEPT_EULA=1`. The other lanes above are
wired in after the tree passes `scripts/check.sh` and clippy with `-D warnings`; until then `scripts/check.sh` is
expected to fail on parts of the tree.
