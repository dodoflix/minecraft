# Standards

Rules for code, scripts and docs in this repository. Each rule says how it is enforced: `scripts/check.sh` (run it
before every push; `scripts/check.sh --fast` skips clippy and rustdoc), a named clippy lint, CI, or review. "Review" means a
maintainer checks it in the diff; nothing fails automatically.

`scripts/check.sh` passes on `main`. The list at the end says which rules the tree does not meet yet. For a lint with
open findings the level is `warn`; it flips to `deny` in the change that brings its count to zero.

Related documents: [README](../README.md), [CONTRIBUTING](../CONTRIBUTING.md), [ARCHITECTURE](../ARCHITECTURE.md),
[docs/testing.md](testing.md), [SECURITY](../SECURITY.md), [tests/e2e/README.md](../tests/e2e/README.md).

## 1. Style and formatting

- Formatting is rustfmt with default settings. There is no `rustfmt.toml`; the style edition follows `edition = "2024"`.
  Enforced: `cargo fmt --all --check` in `scripts/check.sh`.
- Run `cargo fmt --all` before committing. Do not mix formatting changes into a functional change.
- `.editorconfig` covers everything rustfmt does not (final newline, LF, 2-space indent for TOML/YAML/JSON/shell/Markdown).
  Enforced: editor integration, review.
- The one-off formatting commit is listed in `.git-blame-ignore-revs`, so `git blame` skips it.

## 2. Toolchain

- `rust-toolchain.toml` pins stable `1.97.1` with the `rustfmt` and `clippy` components. `[workspace.package]` sets
  `rust-version = "1.97"`. Local runs and CI both read the file, so they use the same compiler and lints.
- Toolchain bumps are a dedicated `build:` change that also fixes any new lints. Enforced: review.
- Edition is 2024 everywhere (`edition.workspace = true`). Nightly features are not used; nightly tools (for example
  `cargo-fuzz`) are optional and never required for a change.
- Linux only. Building needs JDK 25 or newer and a populated `ref/` (`scripts/setup-ref.sh`); see the README.

## 3. Lints

The lint policy lives in `[workspace.lints]` in the root `Cargo.toml` and in `clippy.toml`. Every crate opts in with

```toml
[lints]
workspace = true
```

Enforced: `scripts/check.sh` runs `cargo clippy --workspace --all-targets --all-features --locked --keep-going -- -D warnings`.

- Lint groups: `clippy::all` plus the individually named restriction lints in `Cargo.toml`. No `pedantic`, no
  `nursery`, no whole `restriction` group. A new lint is added by name, with a reason in the commit message.
- A lint that current code still violates is set to `warn` in the table; the change that fixes the last violation flips
  it to `deny`. CI runs clippy with `-D warnings`, so any warning fails the `clippy` job.
- `dbg_macro`, `todo` and `unimplemented` are `deny`. Enforced: clippy.
- Three lints are `allow` for the whole workspace because vanilla's shapes trigger them and mirroring vanilla wins:
  `neg_cmp_op_on_partial_ord` (negated float comparisons are observable with NaN), `module_inception` (`Foo::foo`
  module paths mirror vanilla packages) and `should_implement_trait` (vanilla methods named `next`, `add`, ...). Each
  has a comment in `Cargo.toml`; do not add per-site `allow`s for them.
- `too-many-arguments-threshold = 12` in `clippy.toml`. Vanilla methods above it keep their signature and carry
  `#[expect(clippy::too_many_arguments, reason = "mirrors Class.method")]`.
- Every `#[allow]` and `#[expect]` carries `reason = "..."`. Enforced: `clippy::allow_attributes_without_reason`.
  `#[ignore]` carries a reason too. Enforced: `clippy::ignore_without_reason`.
- Production code uses `#[expect(lint, reason = ...)]`, not `#[allow]`, so a suppression that stops being needed
  becomes a warning. Enforced: `unfulfilled_lint_expectations` under clippy.
- `#[allow(dead_code)]` is not used on production items; delete the item. The root of a test helper module (`common`
  and friends) has exactly one `#![allow(dead_code, reason = "each test target uses a subset of the shared helpers")]`:
  a helper that one of the crate's two targets does not use would otherwise warn. Enforced: review.
- Test code may `unwrap`, `expect`, `panic!`, index, print and use `dbg!` (`clippy.toml` `allow-*-in-tests`). Tests that
  compare NaN on purpose use `#[expect(clippy::eq_op, reason = "NaN != NaN")]`.
- Output: library crates log through the `log` crate; `print_stdout` and `print_stderr` are `deny`. The binaries that print
  (`mc-testbot`, `worldcmp`) and examples are exempt through a crate-root
  `#![expect(clippy::print_stdout, clippy::print_stderr, reason = "...")]` that says why. Enforced: clippy.
- Documentation lint policy:
  - Every crate has a crate-level `//!` comment of at least three lines saying what the crate is and which vanilla
    packages it ports. Enforced: review.
  - `#![warn(missing_docs)]` is enabled per crate, and only once that crate has zero missing items. Order: `mc-nbt`,
    `mc-dfu`, `mc-text`, `mc-brigadier`, then others as they reach zero. It is not enabled on `mc-protocol`,
    `mc-data` or the `mc-game-*` crates before 1.0.
  - `unreachable_pub` stays off: the area crates export their modules `pub` for each other and for the `mc-game` facade.
  - Links in docs must resolve: `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps`. Enforced: the CI `doc` job and `scripts/check.sh`.

## 4. Panics and untrusted input

A remote peer, a world file or a command string must not be able to panic the server.

Untrusted input is:

- wire decoders in `mc-protocol`, in every connection state and both directions;
- NBT and SNBT parsing (`mc-nbt`);
- region, chunk and palette readers (`mc-world`);
- JSON text components and other Gson-shaped JSON;
- command-string parsing (`mc-brigadier`, the command code in `mc-game-core`);
- the login and encryption path;
- the JSON-RPC management API.

Rules:

- Decoders return `Result`. A malformed input yields the same outcome as vanilla, usually a disconnect with vanilla's
  message. Enforced: review, plus the differential tests where vanilla's behaviour is known.
- Every length or count read from input is bounded before it is used for allocation or looping:
  `Vec::with_capacity(n.min(MAX))` or the shared `read_count` helpers. Never `with_capacity(n)` on a raw value.
  Enforced: review.
- Decode modules do not `unwrap`, `expect`, `panic!`, index or slice on input-derived values. Each decode module
  carries a module-level `#![warn(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)]`,
  which becomes `deny` in the change that brings that module to zero. Enforced: clippy. Today these modules still
  have findings.
- Server handlers never `assert!` or `panic!` on client data; they disconnect or ignore the packet as vanilla does.
  Enforced: review.
- A panic while handling one packet is contained and closes only that connection: the connection task wraps the
  handshake, status, login and configuration phases in one `catch_unwind` guard, and the play packet dispatch has its
  own. For this, `panic = "unwind"` stays set in `[profile.dev]` and `[profile.release]`. Enforced: review and the
  panic-injection test.
- Each pre-play phase (handshake, status, login, configuration) has one deterministic hostile-packet test: send a
  malformed packet, assert the connection is closed and the server still accepts new connections. Enforced: review.
- Fuzzing (`cargo-fuzz`, nightly) is optional and never gates a pull request. The existing fixture corpora are the
  seeds for fuzz targets.

## 5. `unsafe`

- The `unsafe_code` lint is not enabled: every `linkme` registration expands to a `#[link_section]` static that it
  flags, which is far more sites than real `unsafe`. New `unsafe` is checked in review; keep it in the smallest function
  that needs it.
- Every `unsafe` block has a `// SAFETY:` comment stating the invariant. Enforced: `clippy::undocumented_unsafe_blocks`
  (`deny`).
- A new lifetime-erasing `transmute` needs explicit sign-off from a maintainer in the review. Enforced: review.

## 6. Error handling

- New error types in library crates use `thiserror`. Where the message is observable (disconnect reasons, command
  errors, log lines players see), `Display` is identical to vanilla's text. Enforced: review.
- No new `Result<_, String>` in public signatures of `mc-nbt`, `mc-protocol`, `mc-world` or `mc-core`. Enforced: review.
- Type names ending in `Exception` are used only for types that have an identically named vanilla class. Enforced: review.
- `std::process::exit` is called only from the code of a binary (`main` and its command-line helpers). Library code
  returns an error. The one exception is `crates/mc-game-core/src/server/watchdog.rs`, which mirrors vanilla's
  `ServerWatchdog`, which halts the JVM when the server stops ticking. Enforced: review.
- Large error types are boxed. Enforced: `clippy::result_large_err` (part of `clippy::all`).

## 7. Naming

- Ported code mirrors vanilla: types keep vanilla class names, methods and fields keep vanilla names in snake_case,
  module paths follow vanilla packages. `get_*` getters and `*Exception` types are a documented exemption from the
  Rust API Guidelines because they are vanilla's names. The details are in the conventions section of
  [ARCHITECTURE.md](../ARCHITECTURE.md); they are not repeated here.
- Code that has no vanilla counterpart (tooling, the test bot, the management API glue, test helpers) follows the Rust
  API Guidelines naming.
- Enforced: review.

## 8. Layout, crates and dependencies

- Dependency direction is fixed (see [ARCHITECTURE.md](../ARCHITECTURE.md)): a lower crate never imports a higher
  one; the `mc-game-*` area crates never depend on each other (except `animals`/`monsters` on `ai`) or on `mc-game`. Enforced: Cargo (a cycle does not
  build) and review for the sibling rule.
- Every `.rs` file is reachable from a crate root through `mod`, `#[path]`, a `[[bin]]` or a `[[test]]` target.
  Enforced: the orphan-file check in `scripts/check.sh`. Do not comment out a `mod` line; delete the file.
- Area crates (`mc-game-*`) import from the crate that owns an item by explicit path (`mc_game_core::world::...`); they do
  not mirror each other's module trees or re-export a lower crate with `pub use <crate>::*`. Only the `mc-game` facade
  re-exports, and only the modules several crates contribute to. New code goes into the area crate that owns it. Enforced: review.
- Every external crate version is declared once, in `[workspace.dependencies]`, and used with `workspace = true`;
  a crate adds only the features it needs. Enforced: review.
- No unused dependencies. Enforced: review (`cargo check --workspace --tests` shows what a crate does not import).
- Every crate sets `publish = false` (inherited from `[workspace.package]`) and a `description`.
- `cargo deny check licenses bans sources` must pass (`deny.toml`: permissive-license allow-list, wildcard version
  requirements denied, crates.io as the only registry, no git dependencies, a duplicate crate version denied unless it
  is listed in `bans.skip` with the upstream crate that forces it). Enforced: `scripts/check.sh`.
- `cargo deny check advisories` runs on a schedule, because its result changes without a code change. One advisory is
  known and unresolved: RUSTSEC-2023-0071 (`rsa`, timing side channel). There is no patched release, and vanilla's
  login handshake requires an RSA-1024 key exchange whose private-key decryption an unauthenticated client can reach
  in online mode. No mitigation is claimed. It is listed in `deny.toml` with this reason.
- Scripted cargo calls use `--locked`. Enforced: review.
- Features are additive and checked with `--all-features`. No default feature changes behaviour. Enforced: review.

## 9. Scripts

Shell:

- `#!/usr/bin/env bash`, then `set -euo pipefail`.
- `ROOT="$(cd "$(dirname "$0")/.." && pwd)"` for the repository root; no absolute or user-specific paths.
- `-h` and `--help` print the script's header comment.
- Required tools are checked up front with a `command -v` loop and a clear message, not on first use.
- `rm -rf` only on a path from `mktemp -d` or under `$ROOT/run/`.
- No script writes `eula=true` unless `MC_ACCEPT_EULA=1` is set; the Minecraft EULA is accepted by the user, not by
  the repository.
- Enforced: `shellcheck` on every tracked `.sh` file in `scripts/check.sh`. The rest: review.

Python:

- Standard library only; no `pip install` step.
- Enforced: `ruff check --select F,E9` on every tracked `.py` file in `scripts/check.sh`.

Generators (scripts that write fixtures or goldens):

- Start with a `Usage:` line and an `Output: <path>` line in the header comment, and live beside the fixtures they
  write. Enforced: `scripts/check.sh` fails on a generator without both headers.

## 10. Comments and docs

- A comment explains why something is the way it is, or names the vanilla symbol it mirrors (`Class.method`). Code
  that is obvious needs no comment.
- Comments do not narrate history ("previously", "now uses", "fixed:"). Git has the history.
- Do not paste Java statements or method bodies into comments. Three or more consecutive comment lines of Java is a
  defect; a single-line quote of a vanilla condition is tolerated, but a prose description is preferred. Name the
  class and method instead. Enforced: review.
- `TODO`, `FIXME` and similar marker comments carry an issue number (`TODO(#123): ...`). Enforced: review.
- `shortcut: <limit>` marks a deliberate, known limit: a case where the Rust differs from vanilla (or from the obvious
  implementation) and is not modelled, with what differs and when it would matter or be worth upgrading. It is for
  behaviour that vanilla play does not reach or that has a `Not implemented:` comment; unfinished work uses `TODO(#n)` instead.
  `git grep 'shortcut:'` lists them. Enforced: review.
- Prose does not hard-code exact figures that drift (number of crates, tests, files or lines, measured durations).
  Approximate size hints for a prerequisite (disk, memory) are fine. Enforced: review.
- One document per purpose: `README.md` (what it is, status, quick start), `CONTRIBUTING.md` (workflow),
  `ARCHITECTURE.md` (design, conventions, crate layout), `docs/standards.md` (this file), `docs/testing.md` (test
  tiers and fixtures), `SECURITY.md` (vulnerability reporting), `tests/e2e/README.md` (end-to-end harness). Do not
  add a second document for a purpose that has one.
- Docs describe what is, not plans. Open work is tracked as GitHub issues.
- Skipped vanilla behaviour is recorded where it is skipped: every stub, hook or class that omits vanilla behaviour
  carries a `Not implemented: <what vanilla does and what happens instead>` doc comment at that site. These comments
  are the authoritative record. The README "Known gaps" section only gives an overview by category and links to the
  `parity` issue that tracks each category; it does not list individual behaviours. Enforced: review.
- A hook installer or trait with no caller or no implementor is deleted, or wired in the same change. Enforced: review.
- Doc links resolve: `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps`. Enforced: the CI `doc` job and `scripts/check.sh`.

## 11. Syntactic slop checks

`scripts/check.sh` fails on these patterns. They are syntactic on purpose; everything else in this document is review.

- A function named `fn _name(` outside tests: a function that exists only to use a binding and silence a warning.
- A statement `let _ = <identifier or path>;` or `let _ = (a, b);`: same purpose. Remove the unused binding or parameter instead. (An
  unused trait-method parameter is named `_param` in the signature.) Discarding a call result with `let _ = call();` is
  not matched.
- An `.rs` file not reachable from a crate root (section 8).
- A commented-out `mod`, `use`, `fn` or `let` line. Delete the code.

## 12. Provenance guard

The repository holds our code only. Mojang's sources and assets are never committed; the only Mojang text kept for output
parity is listed in `NOTICE`. Releases are source-only: builds
embed Mojang data, so no binary artifacts are published.

`scripts/check.sh` fails if the tree contains:

- a tracked file whose first line starts with `package net.minecraft.` (decompiled vanilla source);
- a tracked `*.jar`, `*.class`, `*.png`, `*.ogg` or `*.zip` file;
- a tracked file larger than 8 MiB.

Data derived from vanilla (fixtures, generated reports) is covered by the notice in the repository root. A new data
directory adds a row there. Enforced: review.

## Where the tree does not yet meet this document

These are open. CI does not enforce them; everything else in this document that says "Enforced" passes on `main`.

- Decode modules in `mc-protocol`, `mc-nbt`, `mc-world` and the command code still contain `unwrap` and indexing; the
  per-module `warn` attributes are not in place. `mc-brigadier`'s `StringReader` panics on an out-of-range cursor like
  Java's `charAt`; the command dispatcher contains it.
- Some generated fixtures do not reproduce byte for byte:
  - the chat goldens embed freshly generated RSA keys;
  - the component hash-order, registry line-order, POI, recipe-book and recipe (update packet) goldens depend on the JVM run;
  - `trades_golden.txt` differs on JDK 27;
  - `model_golden.tsv` and `binary_golden.tsv.gz` (`ModelOracle.java`, `BinaryOracle.java`) were generated from an older
    `snbt_golden.tsv.gz`; regenerating them against the current corpus adds the missing `Z` records;
  - `tree_golden_items.txt` (`CommandTreeGolden.java`, built with `tools/extractor`) gains lines, and
    `vegetation_features.txt.gz` (`VegetationFeatureOracle.java`) differs by a few bytes; the cause of both is unknown.
- `missing_docs` is not enabled on any crate yet, and some crates lack a three-line crate doc.
- Commit subjects: the format rule in `CONTRIBUTING.md` applies to commits after `d0016913`; older subjects are not rewritten.
