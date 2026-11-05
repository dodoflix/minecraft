# Architecture & conventions

Goal: a Rust Minecraft Java Edition **26.3** server (protocol `777`, world version `5023`, data pack
format `121`) whose *observable behavior* matches the vanilla server 1:1 — wire protocol, world
generation, world files (Anvil), game mechanics, commands, data packs. Code is idiomatic Rust, not a
transliteration.

## Reference material (`ref/`, gitignored)

| Path | What |
|---|---|
| `ref/src/net/minecraft/**` | Decompiled vanilla 26.3 server (Mojang names, unobfuscated). **The spec.** |
| `ref/src/data/minecraft/**` | Vanilla built-in data pack (recipes, loot, tags, worldgen JSON, …) |
| `ref/generated/reports/` | Vanilla data-generator reports: `blocks.json`, `registries.json`, `packets.json`, `commands.json`, `datapack.json`, … |
| `ref/generated/data/` | Data-generator output (same content as the built-in data pack) |
| `ref/bundle/META-INF/{versions,libraries}` | Unbundled vanilla server jar + its libraries (brigadier, DFU, authlib, …) |
| `ref/server.jar` | Original vanilla server bundler jar (use it to run the real vanilla server as an oracle) |
| `ref/vineflower.jar` | Decompiler (e.g. to decompile brigadier / authlib from `ref/bundle`) |

### Vanilla oracle

`scripts/vanilla-java.sh Foo.java` runs a single-file Java program with the vanilla classes on the
classpath (`scripts/vanilla-cp.sh` prints it). Call
`net.minecraft.SharedConstants.tryDetectVersion(); net.minecraft.server.Bootstrap.bootStrap();` first
if you touch registries. **Use this to produce golden values for tests** (random sequences, noise
samples, shapes, codecs, …) instead of guessing. Golden outputs are committed as test fixtures under
the crate's `tests/fixtures/`; keep the generating `.java` next to them.

The real vanilla server: `java -jar ref/server.jar nogui` in a scratch dir (accept EULA, set
`online-mode=false`). Used for differential tests (worldgen, protocol).

## Workspace layout

```
crates/
  mc-core             math (Mth), positions, Direction/Axis, AABB, random sources, BitStorage, Identifier, UUID utils
  mc-nbt              NBT tags, binary IO (named + network/unnamed root), SNBT, compression helpers
  mc-text             Chat Component model + JSON and NBT codecs
  mc-brigadier        Port of com.mojang.brigadier
  mc-dfu              Port of the DFU Codec/DataResult/DynamicOps framework (NBT, JSON, CRC32C-hash ops) for vanilla data codecs
  mc-data             Generated static data (blocks/states/items/entities/registries/packet ids) + vanilla data pack access
  mc-protocol         Wire codec primitives, framing, compression, encryption, packet structs (all states);
                      module `components`: all data component types + DataComponentMap/Patch + ItemStack value type,
                      with NBT/JSON/stream codecs (lives here because play packets need to decode item stacks)
  mc-protocol-macros  derive(Encode, Decode) proc macros
  mc-worldgen         pure levelgen: noise synth, density functions, NoiseChunk/aquifer/ore veins, biome sources + climate,
                      surface rules, carvers (everything that needs only block *states*, not block *behavior*)
  mc-world            chunk storage: PalettedContainer, sections, ChunkAccess/ProtoChunk/LevelChunk data, heightmaps,
                      chunk NBT serialization, Anvil region files, level.dat / playerdata IO, chunk packet data
  mc-light            light engine (sky + block) over mc-world chunks
  mc-game             all gameplay: MinecraftServer, PlayerList, ServerLevel, chunk map/tickets, network listeners
                      (login/config/play), block behaviors, block entities, fluids, entities + AI, items, menus,
                      recipes, loot, advancements, commands, features/structures/placement, scoreboard, raids, …
  mc-server           thin binary: CLI args, server.properties, console, JSON-RPC management, starts mc-game
  mc-testbot          headless protocol client (scriptable bot) used by integration/E2E tests
tools/                non-Rust tooling (e.g. Java data extractor)
scripts/              helper scripts (vanilla oracle, real client harness, setup-ref)
```

Dependency direction (lower never imports higher):
`mc-core, mc-nbt, mc-brigadier` → `mc-text, mc-data` → `mc-protocol, mc-world` → `mc-light, mc-worldgen`
→ `mc-game` → `mc-server`; `mc-testbot` → `mc-protocol`.

## mc-game design (contract for all gameplay work)

Vanilla's gameplay code is one tightly coupled object graph, so it lives in one crate and mirrors vanilla structure.

- **Threads.** One game thread runs `MinecraftServer` exactly like vanilla's main thread (20 TPS loop, task queue
  drained between ticks, same per-tick order: `MinecraftServer::tick_server` → `tick_children` → each
  `ServerLevel::tick` in vanilla order). Network I/O is tokio; each connection's reader task decodes packets and
  forwards them to the game thread through a channel; packets are *handled* on the game thread wherever vanilla
  handles them there (play/configuration). Chunk generation/loading/saving run on worker threads and hand results
  back to the game thread, mirroring ChunkMap's status pipeline.
- **Blocks.** `trait BlockBehaviour` (vanilla `BlockBehaviour` + `Block` overridable methods, snake_case, default
  impls = vanilla base behavior). One static implementor per block (unit/param structs: `StairBlock`, `SlabBlock`,
  …), looked up by `Block` id. Static per-state data (shapes, flags) comes from `mc_data`.
- **Entities.** Data inheritance by composition, behavior by trait: `EntityData` ⊂ `LivingEntityData` ⊂ `MobData`
  ⊂ … (each concrete type struct embeds its parent's data), `trait Entity` with accessors (`base()`, `living()`,
  `mob()` returning `Option`) and overridable vanilla methods whose default impls are the vanilla super behavior;
  overrides call the parent behavior explicitly (free functions like `living_entity::base_tick`). Entities are stored
  as `Box<dyn Entity>` in the level's entity storage keyed by entity id; while an entity ticks it is taken out of the
  slot and put back afterwards (lookups during its own tick exclude it, as vanilla queries do).
- **Block entities** likewise `Box<dyn BlockEntity>` in the chunk, same take-out pattern while ticking.
- **Items.** `trait ItemBehaviour` static per item (vanilla `Item` subclasses); `ItemStack` value type comes from
  `mc-components`; behavior functions take `&mut ServerLevel`, player, hand, …
- **Level access traits** mirror vanilla interfaces (`BlockGetter`, `LevelReader`, `LevelAccessor`, `WorldGenLevel`)
  so block/feature code runs against both `ServerLevel` and `WorldGenRegion`.
- **Registration without central lists:** blocks/items/entities/commands/argument types/menus/etc. register through
  `linkme` distributed slices from the module that implements them, so parallel work never edits one shared table.
- **Data-driven content** (recipes, loot tables, advancements, worldgen JSON, tags, damage types, enchantments,
  dimension types, …) is loaded from the vanilla data pack + world data packs through one `ReloadableServerResources`
  / registry-loading path, with vanilla codec semantics (unknown/invalid entries rejected or logged like vanilla).

## Parallel workflow (from wave 2 on)

Every agent works in its own git worktree on its own branch:
`git worktree add /home/dodo/Projects/mc-wt/<name> -b <name> main && ln -s ./ref /home/dodo/Projects/mc-wt/<name>/ref`
(the `ref/` symlink is required: builds read `ref/extracted`, `ref/generated`, …), builds only there, then integrates:
`git rebase main` (resolve conflicts, keep both sides' intent), build + test the crates it touched, and fast-forward
main under a lock: `flock /tmp/mc-merge.lock git -C /home/dodo/Projects/minecraft merge --ff-only <name>` (retry
rebase if main moved). Never force-push/reset main; never discard others' work.

**Backup:** remote `origin` = private GitHub repo `dodoflix/minecraft`. After merging to main run
`flock /tmp/mc-merge.lock git -C /home/dodo/Projects/minecraft push -q origin main`; after WIP/checkpoint commits
push your branch (`git push -q origin <name>`; branch force-push only after your own rebase, never main).
**Disk:** after your branch is merged and you're done, delete your worktree's build cache
(`rm -rf /home/dodo/Projects/mc-wt/<name>/target`); `/home` filled up once with stale per-worktree targets.
**Machine memory (hard rule):** systemd-oomd killed the whole session twice (RAM+swap >90%) when many mc-game builds ran
at once. Every cargo build/test/check, by every agent, goes through ONE machine-wide lock and low priority:
`flock /tmp/mc-build.lock nice -n 10 env CARGO_BUILD_JOBS=6 cargo …`. At most 3 helper agents per scope; at most one
vanilla JVM server + one real client running at a time per agent.

**Commits use Conventional Commits**: `<type>(<scope>): <summary>` with type in
`feat|fix|refactor|perf|test|docs|build|chore`, scope = crate without the `mc-` prefix or area
(e.g. `feat(protocol): add play packets`, `fix(worldgen): aquifer lava level`, `test(world): region differential`).
Imperative, lowercase summary, no trailing period; body explains why when non-obvious.

## Conventions (all agents)

1. **Naming mirrors vanilla.** Types keep vanilla class names (`BlockPos`, `LegacyRandomSource`,
   `NormalNoise`), methods/fields are vanilla names in snake_case (`next_int`, `get_x`→`x()` is fine
   for trivial getters). Module paths loosely mirror vanilla packages. When in doubt about an API
   another crate should expose, it is the vanilla API in Rust form.
2. **Behavior is the spec, Java is the reference.** Read the vanilla class before writing the Rust
   equivalent. Rewrite idiomatically; keep every observable behavior, constant, edge case, ordering,
   and RNG call sequence (RNG call order is observable through world generation and mob behavior).
3. **Java numeric semantics.** Java `int`/`long` arithmetic wraps: use `wrapping_*` wherever overflow
   is possible. `>>>` is a cast to unsigned then shift. Keep `float` vs `double` exactly as vanilla
   (`f32` vs `f64`); do not "improve" precision. Rust `as` from float to int matches Java
   (saturating, NaN→0). Java `Math.floor`/`Mth.floor` semantics matter (`Mth.floor` is int).
   Java strings are UTF-16: lengths/limits measured by vanilla in UTF-16 code units must be too.
4. **Tests.** Unit tests per crate. Prefer golden values generated by the vanilla oracle. No
   tautological tests.
5. **Isolation while parallel agents work:** only edit your own crate(s); add dependencies in your
   crate's `Cargo.toml` (not the root). Build/test only your crate:
   `CARGO_TARGET_DIR=target/<your-crate> cargo test -p <crate>`. Never run workspace-wide
   `cargo build`/`fmt`/`clippy`, never touch other crates' files. If you need something from another
   crate that doesn't exist yet, message its owner or write against the vanilla-shaped API described
   here.
6. Edition 2024, stable toolchain. `unsafe` only with a justified comment. No `unwrap()` on
   network/disk input — malformed input must produce the same outcome as vanilla (usually a
   disconnect with vanilla's message).
7. Keep `STATUS.md` updated with what your slice covers and what is known missing.
8. **Pace (user constraint: usage limits).** At most 1–2 top-level scopes run at a time; helper sub-agents
   inside a scope are fine. No polling loops. Read only the vanilla ranges you need; run targeted tests while
   iterating and the full suite once. Paused scopes leave a committed `RESUME-<Name>.md` on their branch.
9. **Java gotchas that leak into observable output:** `Math.round` is `floor(x + 0.5)` (Rust `round` differs at
   .5); `Double/Float.toString` → `mc_core::math::java::{java_double_to_string, java_float_to_string}` (never `{}`);
   `String.hashCode` → `mc_core::math::java`; `Mth.sin/cos` use the lookup table; Java `HashMap`/`HashSet`
   iteration order can be observable (vanilla code that iterates them must be reproduced, `LinkedHashMap` = insertion order).
