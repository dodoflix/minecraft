# Standards

Rules for code, scripts and docs in this repository. Each rule says how it is enforced: `scripts/check.sh` (run it
before every push; `scripts/check.sh --fast` skips clippy), a named clippy lint, CI, or review. "Review" means a
maintainer checks it in the diff; nothing fails automatically.

`scripts/check.sh` does not pass on the current tree. The list at the end says what is still open. Rules that are
already met are enforced as written; for the others the lint level starts at `warn` and flips to `deny` in the change
that brings its count to zero.

Related documents: [README](../README.md), [CONTRIBUTING](../CONTRIBUTING.md), [ARCHITECTURE](../ARCHITECTURE.md),
[docs/testing.md](testing.md), [docs/crate-split.md](crate-split.md), [tests/e2e/README.md](../tests/e2e/README.md).

## 1. Style and formatting

- Formatting is rustfmt with default settings. There is no `rustfmt.toml`; the style edition follows `edition = "2024"`.
  Enforced: `cargo fmt --all --check` in `scripts/check.sh`.
- Run `cargo fmt --all` before committing. Do not mix formatting changes into a functional change.
- `.editorconfig` covers everything rustfmt does not (final newline, LF, 2-space indent for TOML/YAML/JSON/shell/Markdown).
  Enforced: editor integration, review.
- The check fails today: the one-off formatting commit has not landed yet. After it lands, its revision is listed in
  `.git-blame-ignore-revs`.

## 2. Toolchain

- `rust-toolchain.toml` pins stable `1.97.1` with the `rustfmt` and `clippy` components. `[workspace.package]` sets
  `rust-version = "1.97"`. Local runs and CI both read the file, so they use the same compiler and lints.
- Toolchain bumps are a dedicated `build:` change that also fixes any new lints. Enforced: review.
- Edition is 2024 everywhere (`edition.workspace = true`). Nightly features are not used; nightly tools (for example
  `cargo-fuzz`) are optional and never required for a change.
- Linux only. Building needs JDK 25 and a populated `ref/` (`scripts/setup-ref.sh`); see the README.

## 3. Lints

The lint policy lives in `[workspace.lints]` in the root `Cargo.toml` and in `clippy.toml`. Every crate opts in with

```toml
[lints]
workspace = true
```

Enforced: `scripts/check.sh` runs `cargo clippy --workspace --all-targets --all-features --locked --keep-going -- -D warnings`.

- Lint groups: `clippy::all` plus the individually named restriction lints in `Cargo.toml`. No `pedantic`, no
  `nursery`, no whole `restriction` group. A new lint is added by name, with a reason in the commit message.
- `-D warnings` is applied only to lints whose count is zero. A lint that current code still violates is set to `warn`
  in the table; the change that fixes the last violation flips it to `deny`. Until the tree is clean, CI does not pass
  `-D warnings`.
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
- `#[allow(dead_code)]` is not used on production items; delete the item. In a crate's `tests/common` module there is
  exactly one `#![allow(dead_code, reason = "each test binary uses a subset of the helpers")]`, which goes away when
  the crate's tests are consolidated into fewer targets. Enforced: review.
- Test code may `unwrap`, `expect`, `panic!`, index, print and use `dbg!` (`clippy.toml` `allow-*-in-tests`). Tests that
  compare NaN on purpose use `#[expect(clippy::eq_op, reason = "NaN != NaN")]`.
- Output: library crates log through the `log` crate; `print_stdout` and `print_stderr` are `warn`. The binaries
  (`mc-server`, `mc-testbot`, `worldcmp`) and examples are exempt through a crate-root
  `#![allow(clippy::print_stdout, clippy::print_stderr, reason = "command-line binary")]`.
  Enforced: clippy.
- Documentation lint policy:
  - Every crate has a crate-level `//!` comment of at least three lines saying what the crate is and which vanilla
    packages it ports. Enforced: review (a script check is planned).
  - `#![warn(missing_docs)]` is enabled per crate, and only once that crate has zero missing items. Order: `mc-nbt`,
    `mc-dfu`, `mc-text`, `mc-brigadier`, then others as they reach zero. It is not enabled on `mc-protocol`,
    `mc-data` or the `mc-game-*` crates before 1.0.
  - `unreachable_pub` stays off until the re-export bridge (section 8) is gone.
  - Links in docs must resolve: `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps`. Enforced: CI once the lanes are wired.

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
- A panic while handling one packet is contained with `catch_unwind` in every listener phase (login, configuration,
  status, play) and closes only that connection. For this, `panic = "unwind"` stays set in `[profile.dev]` and
  `[profile.release]`. Enforced: review.
- Each phase gets one deterministic hostile-packet test: send a malformed packet, assert the connection is closed and
  the server still accepts new connections. Enforced: review. These tests are not written yet.
- Fuzzing (`cargo-fuzz`, nightly) is optional and never gates a pull request. The existing fixture corpora are the
  seeds for fuzz targets.

## 5. `unsafe`

- The `unsafe_code` lint is not enabled: every `linkme` registration expands to a `#[link_section]` static that it
  flags, which is far more sites than real `unsafe`. New `unsafe` is checked in review; keep it in the smallest function
  that needs it.
- Every `unsafe` block has a `// SAFETY:` comment stating the invariant. Enforced: `clippy::undocumented_unsafe_blocks`
  (`warn` today; `deny` once the remaining blocks have comments).
- A new lifetime-erasing `transmute` needs explicit sign-off from a maintainer in the review. Enforced: review.

## 6. Error handling

- New error types in library crates use `thiserror`. Where the message is observable (disconnect reasons, command
  errors, log lines players see), `Display` is identical to vanilla's text. Enforced: review.
- No new `Result<_, String>` in public signatures of `mc-nbt`, `mc-protocol`, `mc-world` or `mc-core`. Enforced: review.
- Type names ending in `Exception` are used only for types that have an identically named vanilla class. Enforced: review.
- `std::process::exit` is called only from `main`. Library code returns an error. Enforced: review.
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
  one; the `mc-game-*` area crates never depend on each other or on `mc-game`. Enforced: Cargo (a cycle does not
  build) and review for the sibling rule.
- Every `.rs` file is reachable from a crate root through `mod`, `#[path]`, a `[[bin]]` or a `[[test]]` target.
  Enforced: the orphan-file check in `scripts/check.sh`. Do not comment out a `mod` line; delete the file.
- The `mc-game-*` re-export bridge (a `parent` dependency plus `pub use parent::*`, see
  [docs/crate-split.md](crate-split.md)) is a temporary layout. It is removed before any statement of API stability,
  before any crates.io publication and before `mc-game-core` is split further. Until then new code goes into the area
  crate that owns it. Enforced: review.
- Every external crate version is declared once, in `[workspace.dependencies]`, and used with `workspace = true`.
  Enforced: review. The migration is pending; so far only `serde_json` is declared there.
- No unused dependencies. Enforced: review (`cargo check --workspace --tests` shows what a crate does not import).
- Every crate sets `publish = false` (inherited from `[workspace.package]`) and a `description`.
- `cargo deny check licenses bans sources` must pass (`deny.toml`: permissive-license allow-list, wildcard version
  requirements denied, crates.io as the only registry, no git dependencies). Enforced: `scripts/check.sh`.
- `cargo deny check advisories` runs on a schedule, because its result changes without a code change. One advisory is
  known and unresolved: RUSTSEC-2023-0071 (`rsa`, timing side channel). There is no patched release, and vanilla's
  login handshake requires an RSA-1024 key exchange. No mitigation is claimed. It is listed in `deny.toml` with this
  reason.
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
  write. Enforced: review now; a check that fails on a missing header is planned.

## 10. Comments and docs

- A comment explains why something is the way it is, or names the vanilla symbol it mirrors (`Class.method`). Code
  that is obvious needs no comment.
- Comments do not narrate history ("previously", "now uses", "fixed:"). Git has the history.
- Do not paste Java statements or method bodies into comments. Three or more consecutive comment lines of Java is a
  defect; a single-line quote of a vanilla condition is tolerated, but a prose description is preferred. Name the
  class and method instead. Enforced: review.
- `TODO`, `FIXME` and similar marker comments carry an issue number (`TODO(#123): ...`). Enforced: review.
- Prose does not hard-code sizes, counts or percentages that change (number of crates, tests, lines, disk usage).
  Enforced: review.
- One document per purpose: `README.md` (what it is, status, quick start), `CONTRIBUTING.md` (workflow),
  `ARCHITECTURE.md` (design and conventions), `docs/standards.md` (this file), `docs/testing.md` (test tiers and
  fixtures), `docs/crate-split.md` (crate layout), `tests/e2e/README.md` (end-to-end harness). Do not add a second
  document for a purpose that has one.
- Docs describe what is, not plans. Planned work belongs in issues.
- A public item that skips vanilla behaviour (a stub that ignores its arguments and returns a default) documents what it
  skips, and the gap is listed in the README status section. Enforced: review.
- A hook installer or trait with no caller or no implementor is deleted, or wired in the same change. Enforced: review.
- Doc links resolve: `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps`. Enforced: CI once the lanes are wired.

## 11. Syntactic slop checks

`scripts/check.sh` fails on these patterns. They are syntactic on purpose; everything else in this document is review.

- A function named `fn _name(` outside tests: a function that exists only to use a binding and silence a warning.
- A statement `let _ = <identifier or path>;`: same purpose. Remove the unused binding or parameter instead. (An
  unused trait-method parameter is named `_param` in the signature.) Discarding a call result with `let _ = call();` is
  not matched.
- An `.rs` file not reachable from a crate root (section 8).
- A commented-out `mod`, `use`, `fn` or `let` line. Delete the code.

## 12. Provenance guard

The repository holds our code only. Mojang's sources and assets are never committed. Releases are source-only: builds
embed Mojang data, so no binary artifacts are published.

`scripts/check.sh` fails if the tree contains:

- a tracked file whose first line starts with `package net.minecraft.` (decompiled vanilla source);
- a tracked `*.jar`, `*.class`, `*.png`, `*.ogg` or `*.zip` file;
- a tracked file larger than 8 MiB.

Data derived from vanilla (fixtures, generated reports) is covered by the notice in the repository root. A new data
directory adds a row there. Enforced: review.

## Where the tree does not yet meet this document

`scripts/check.sh` is expected to fail until these are done. The `-D warnings` wiring in CI follows the cleanup.

- The one-off formatting commit is pending, so `cargo fmt --all --check` fails.
- Clippy is not clean: several lints in the workspace table are still `warn`, and `-D warnings` is not applied yet.
- Silencer functions, `let _ = <ident>;` statements and orphan `.rs` files still exist.
- Decode modules in `mc-protocol`, `mc-nbt`, `mc-world` and the command code still contain `unwrap` and indexing; the
  per-module `warn` attributes and the hostile-packet tests are not in place.
- `missing_docs` is not enabled on any crate yet, and some crates lack a three-line crate doc.
- The `mc-game-*` bridge layout is still in place, and external dependency versions are still declared per crate
  instead of in `[workspace.dependencies]`.
