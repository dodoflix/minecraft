# Contributing

This is a Rust port of the vanilla Minecraft Java Edition 26.3 server (protocol 777, world version 5023, data pack format 121). Linux only. Behaviour is checked against the real vanilla server, so most changes need a vanilla reference. See [ARCHITECTURE.md](ARCHITECTURE.md) for the crate layout and [docs/standards.md](docs/standards.md) for code standards.

## Setup

- Install [rustup](https://rustup.rs). It picks the toolchain pinned in `rust-toolchain.toml` automatically.
- Install JDK 25. The build script of `mc-data` reads the extractor output under `ref/`.
- Run `scripts/setup-ref.sh`. It downloads the vanilla server from Mojang into `ref/`. You accept Mojang's EULA yourself; nothing from `ref/` is ever committed.
- Install the tools `scripts/check.sh` needs: rustfmt, clippy, cargo-deny, shellcheck, ruff.
- Run `git config blame.ignoreRevsFile .git-blame-ignore-revs`. The file lists the one-off rustfmt commit (and any later bulk reformat), so `git blame` skips it and keeps pointing at the real change. GitHub's blame view reads it automatically.
- Compiling `mc-game-core` is memory hungry. On a small machine set `CARGO_BUILD_JOBS` (for example `CARGO_BUILD_JOBS=2`).

## Workflow

Until the first 1.0 release:

- Maintainers integrate branches through a local gate: formatting, lints and tests must pass, then the branch is fast-forwarded onto `main`.
- Contributors open pull requests against `main`. Maintainers integrate them the same way, so keep your branch rebased on `main`.

From 1.0 on, `main` is protected: pull requests only, squash-merge, the PR title becomes the commit message, and the `ci-ok` check must pass.

Branch names are `<type>/<short-kebab>`, for example `fix/aquifer-lava-level`. `<type>` is a commit type from the list below.

### Commits

Commits follow [Conventional Commits](https://www.conventionalcommits.org): `type(scope)!: subject`.

- Types: `feat`, `fix`, `perf`, `refactor`, `docs`, `test`, `build`, `ci`, `chore`, `revert`, `style`.
- Scope is the crate name without the `mc-` prefix (`protocol`, `game-core`, `game-blocks`, `worldgen`, `nbt`, `testbot`, `server`, ...) or one of `ci`, `deps`, `scripts`, `tools`, `docs`.
- The subject is imperative, lowercase, at most 72 characters, no trailing period.
- No `wip` commits. Squash them locally before you push.
- The body says why when it is not obvious from the diff.
- Mark breaking changes with `!` after the scope and a `BREAKING CHANGE:` footer.

Example: `fix(worldgen): use vanilla lava level in aquifer picker`.

CI runs `scripts/check-commits.sh` on a pull request's title and every commit: known type, scope from the list above if present, no trailing period, at most 100 characters (over 72 only warns). Run it yourself with `scripts/check-commits.sh origin/main..HEAD`.

## Checks

Run `scripts/check.sh` before you push. `--fast` skips clippy. A missing tool is a failure, not a skip, and the script exits 2 with a message.

CI (`.github/workflows/ci.yml`) runs on every pull request and every push to `main`. Its jobs, and the one command each runs:

| Job | Runs | Expected time |
|---|---|---|
| `fmt` | `cargo fmt --all --check` | under 1 min |
| `clippy` | `cargo clippy --workspace --all-targets --all-features --locked --keep-going -- -D warnings` | 4 to 15 min (cold cache) |
| `deny` | `cargo deny check licenses bans sources` | 1 min |
| `hygiene` | `scripts/check.sh --fast` (shellcheck, ruff, source and provenance checks) | 1 min |
| `commits` | `scripts/check-commits.sh` on the PR title and commits | 1 min |
| `test` | `scripts/test-known-failures.py`: `cargo test` per package (T0/T1) | 35 to 45 min |
| `ref` | builds or restores the cached `ref/` that `clippy` and `test` need | seconds cached, 10 to 15 min cold |
| `ci-ok` | passes only when all of the above passed; the one required check | seconds |

`scripts/test-known-failures.py` fails only on a test that `tests/known-failures.txt` does not list; run it locally the same way (`-p <crate>` limits it). A change that makes a listed test pass deletes its line.

`.github/workflows/nightly.yml` runs once a day and on demand: `cargo deny check advisories` and the oracle tier (`scripts/test-known-failures.py --oracle`, which starts the vanilla server and sets `MC_ACCEPT_EULA=1`). Neither blocks a pull request; the oracle tier must be green before a release is tagged. To run CI on a branch without a pull request: `gh workflow run ci.yml --ref <branch>`.

Also run:

- `cargo test -p <crate>` for each crate you touched.
- `cargo test --workspace` before opening a PR.

Do not run workspace-wide `cargo fmt` or clippy rewrites in a feature change. Maintainers make a one-off formatting commit. After it has landed, format the files you touch.

## Tests

Test tiers, layout and how to run them are in [docs/testing.md](docs/testing.md). Put a test at the lowest tier that can prove the behaviour. Expected values for vanilla behaviour come from the oracle, not from your reading of the code.

The oracle and E2E tiers start the real vanilla server. Running them means you accept [Mojang's EULA](https://aka.ms/MinecraftEULA). Opt in explicitly with `MC_ACCEPT_EULA=1`; the helpers print the EULA URL and refuse to run without it.

### Fixtures

Fixtures that record vanilla behaviour are produced by the generator next to them. The generator's header comments (`Usage:`, `Output:`) say how to run it. To regenerate:

1. Run `scripts/setup-ref.sh` if `ref/` is not set up.
2. Run the generator.
3. Review the diff; unexpected changes mean a bug in either side.
4. Commit the generator and its output together.

Never edit generated fixtures by hand. Details are in [docs/testing.md](docs/testing.md).

## Content rules

- No Mojang code or assets in the repository: no decompiled sources, no jars, no copied data files. Read decompiled vanilla source from your local `ref/` only.
- Comments name the vanilla symbol (`Class.method`). They do not paste its statements.
- A new data directory under `crates/*/data` needs a row in `NOTICE`.
- By contributing you certify that you wrote the change or have the right to submit it, and that it contains no Mojang code or assets. There is no DCO or CLA check.

`scripts/check.sh` rejects tracked Mojang-style sources, jars, classes, images, audio, archives and oversized files.

## Review checklist

Reviewers check:

- Behaviour matches vanilla, and the vanilla class and method are cited.
- Tests are at the right tier and golden values come from the oracle, not guesses.
- No silencers: `fn _x`, `let _ = ident;`, `#[allow(dead_code)]` on production items, or blanket allows without `reason`.
- Every file is in the module tree.
- No stub or no-op seam without a caller. A skipped vanilla behaviour is documented in the code and listed in the README status.
- Hook installers and traits have both a caller and an implementor.
- One definition per Java-compat primitive (`String.hashCode`, `floor_div`, ...), in `mc-core`.
- Comments explain why. They never narrate history or paste Java statements.
- No `unwrap`, `expect`, indexing or `panic!` reachable from network, NBT or region input.
- Names follow [docs/standards.md](docs/standards.md).
- Docs touched are still true.
- New dependencies are justified and `cargo deny` passes.

Enforcement is review, except where an item above is covered by `scripts/check.sh`.

## Security

Report vulnerabilities privately through GitHub security advisories. Do not open a public issue.

## License

The project is MIT licensed. Contributions are under the same license.
