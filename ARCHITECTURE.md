# Architecture & conventions

Goal: a Rust Minecraft Java Edition **26.3** server (protocol `777`, world version `5023`, data pack
format `121`) whose *observable behavior* matches the vanilla server 1:1 — wire protocol, world
generation, world files (Anvil), game mechanics, commands, data packs. Code is idiomatic Rust, not a
transliteration.

## Reference material (`ref/`, gitignored)

| Path | What |
|---|---|
| `ref/src/net/minecraft/**` | Decompiled vanilla 26.3 server (Mojang names, unobfuscated). **The spec.** Exists only after `scripts/setup-ref.sh --decompile`. |
| `ref/src/data/minecraft/**`, `ref/src/assets/**` | Vanilla built-in data pack (recipes, loot, tags, worldgen JSON, …) and assets (language files) |
| `ref/generated/reports/` | Vanilla data-generator reports: `blocks.json`, `registries.json`, `packets.json`, `commands.json`, `datapack.json`, … |
| `ref/generated/data/` | Data-generator output (same content as the built-in data pack) |
| `ref/bundle/META-INF/{versions,libraries}` | Unbundled vanilla server jar + its libraries (brigadier, DFU, authlib, …) |
| `ref/server.jar` | Original vanilla server bundler jar (use it to run the real vanilla server as an oracle) |
| `ref/vineflower.jar` | Decompiler used by `--decompile` (and `--libs`, which decompiles brigadier / DataFixerUpper / authlib into `ref/libs/`) |
| `ref/extracted/` | Code-defined data that the data generator does not report, written by `tools/extractor` |

Run `scripts/setup-ref.sh --decompile` to read the vanilla source. The oracle and fixture-regeneration scripts also
need it.

### Vanilla oracle

`scripts/vanilla-java.sh Foo.java` runs a single-file Java program with the vanilla classes on the
classpath (`scripts/vanilla-cp.sh` prints it). Call
`net.minecraft.SharedConstants.tryDetectVersion(); net.minecraft.server.Bootstrap.bootStrap();` first
if you touch registries. **Use this to produce golden values for tests** (random sequences, noise
samples, shapes, codecs, …) instead of guessing. Golden outputs are committed as test fixtures under
the crate's `tests/fixtures/`; keep the generating `.java` next to them.

The real vanilla server: `java -jar ref/server.jar nogui` in a scratch dir (you accept the EULA yourself, set
`online-mode=false`). Used for differential tests (worldgen, protocol).

## Workspace layout

```
crates/
  mc-core             math (Mth), positions, Direction/Axis, AABB, random sources, BitStorage, Identifier, UUID utils
  mc-nbt              NBT tags, binary IO (named + network/unnamed root), SNBT, compression helpers
  mc-text             Chat Component model + JSON and NBT codecs
  mc-brigadier        Port of com.mojang.brigadier
  mc-dfu              Port of the DFU Codec/DataResult/DynamicOps framework (NBT, JSON, CRC32C-hash ops) for vanilla data codecs
  mc-datafix          Port of the DataFixerUpper type system, `DataFixTypes` and the global `DataFixer` on top of mc-dfu; no schema or fix is ported yet
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
  mc-game-core        the gameplay spine: MinecraftServer, PlayerList, ServerLevel, chunk map/tickets, network listeners
                      (login/config/play), Entity/LivingEntity/Mob/Player + AI framework, BlockBehaviour + state, item base,
                      enchantments, recipes, menus, loot, advancements, command infra + arguments, scoreboard, raids, ...
  mc-game-blocks      block behavior implementations + block entities           (leaf over core)
  mc-game-levelgen    features, structures, placement, template processors, /place (leaf over core)
  mc-game-cmds        the vanilla commands, registered via linkme               (leaf over core)
  mc-game-ai          concrete goals/behaviors/sensors                          (leaf over core)
  mc-game-animals     animals + ambient mobs                                    (over mc-game-ai)
  mc-game-monsters    monsters, the zombie horse, creaking heart                (over mc-game-ai)
  mc-game             thin facade: re-exports the crates above so `mc_game::...` is the public API; integration tests, examples
  mc-server           thin binary: parses the command line, sets up logging, `server.properties` and `eula.txt`, then starts mc-game
  mc-testbot          headless protocol client (scriptable bot) used by integration/E2E tests
  mc-testkit          test support: guarded launcher for the vanilla server and our own (no workspace dependencies)
tools/                tools/extractor (Java data extractor), tools/worldcmp (chunk-by-chunk comparison of two world directories)
scripts/              helper scripts (setup-ref, vanilla oracle, real client harness, check.sh)
```

The area crates keep the file paths of vanilla-shaped modules (`world/entity/animal/sheep/...` lives under
`crates/mc-game-animals/src/` and `crates/mc-game-core/src/` holds the rest of `world/entity/`), so a file's crate
is found by its path.

Dependency direction (lower never imports higher):
`mc-core, mc-nbt, mc-brigadier` → `mc-text, mc-data, mc-dfu` → `mc-datafix, mc-protocol, mc-world` → `mc-light, mc-worldgen`
→ `mc-game-core` → `mc-game-{blocks,levelgen,cmds,ai}` → `mc-game-{animals,monsters}` (over `ai`) → `mc-game` → `mc-server`;
`mc-testbot` → `mc-protocol`.

```
mc-game ──► mc-game-blocks ───────────────┐
        ├─► mc-game-levelgen ─────────────┤
        ├─► mc-game-cmds ─────────────────┼─► mc-game-core ─► mc-core, mc-nbt, mc-protocol, mc-world, ...
        ├─► mc-game-ai ───────────────────┤
        ├─► mc-game-animals ──► mc-game-ai ┤
        └─► mc-game-monsters ─► mc-game-ai ┘
```

Rules:

- `mc-game-blocks`, `-levelgen`, `-cmds`, `-animals` and `-monsters` never depend on each other; `animals` and `monsters`
  depend on `ai` and on `core`. No area crate depends on `mc-game`. `blocks`, `levelgen` and `cmds` are leaves: only
  `mc-game` depends on them. Cargo rejects a cycle; the sibling rule is checked in review.
- **Where new gameplay code goes:** into the area crate that owns it (a block behaviour or block entity: `blocks`; a
  feature, structure or placement: `levelgen`; a command: `cmds`; a goal, behaviour or sensor that nothing in `core`
  names: `ai`; an animal or ambient mob: `animals`; a monster, villager, raid part or creaking heart: `monsters`).
  Code that `mc-game-core` itself calls, or that the `Entity` trait or the block base names, must live in `core`: it is
  the spine and cannot depend on the crates above it.
- **How paths resolve.** An area crate declares only the modules whose files it owns and imports everything else from
  the crate that owns it by explicit path (`mc_game_core::world::entity::Entity`); `crate::` means the crate's own modules.
  A module with children in several crates (`world::entity`, `world::level::block::impls`, ...) exists once per crate
  with the same path and holds only that crate's children. A module's file lives in the lowest crate that owns any file
  below it, so a `mod x;` never crosses a crate boundary.
- **The `mc-game` facade** (`crates/mc-game/src/lib.rs`) declares each module that more than one crate contributes to
  once and re-exports exactly those crates (`pub use mc_game_core::world::entity::*; pub use mc_game_animals::world::entity::*;`);
  everything else is reached through the parent's re-export. Binaries and integration tests import from `mc_game`.
- **Macros.** `#[macro_export]` macros of `core` (`register_block_class!`, `update_shape!`, ...) are imported by path
  (`use mc_game_core::register_block_class;`); a macro used by area crates names its items with `$crate::`.
- **Visibility.** Items that area crates use from `core` are `pub`, not `pub(crate)`. An inherent `impl Foreign {}` block is
  not allowed across crates; use an extension trait.
- **Registration and linking.** `#[distributed_slice]` statics in a crate nothing references are dropped by the linker.
  `mc-game` re-exports modules of every area crate so that they are linked;
  `crates/mc-game/tests/golden/registration_counts.rs` asserts the length of every slice and fails if a crate stops being linked.
- **Integration tests.** Most tests build a `MinecraftServer` or `ServerLevel`, which needs the registrations of the area
  crates, and those exist only in the linked-everything `mc-game`; those tests live in `crates/mc-game/tests`. A test lives
  in `mc-game-core` only if it passes there on its own (see [docs/testing.md](docs/testing.md)).
- **Compile time.** An edit to one block, mob, command or feature recompiles one area crate plus the thin `mc-game`; an edit in
  `mc-game-core` recompiles the spine and everything above it.

## mc-game design (contract for gameplay code)

Vanilla's gameplay code is one tightly coupled object graph. It mirrors vanilla structure and is split over `mc-game-core` + the area
crates (see the rules above: each area crate imports what it uses from the owning crate by explicit path, and `mc-game`
re-exports all of them under the vanilla-shaped `mc_game::world::...` paths). Everything below applies to the crates together.

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
- **Block entities** likewise `Box<dyn BlockEntity>` in the chunk, same take-out pattern while ticking. Vanilla runs a
  block entity's level effects while it is still in the chunk (a comparator beside a container reads it, `setBlock` on
  its own position finds it), so effects that must see the entity (`setChanged`, own-block updates) are queued in
  `world::block_entity::deferred` and run when the call that took it out ends.
- **Items.** `trait ItemBehaviour` static per item (vanilla `Item` subclasses); `ItemStack` value type comes from
  `mc-protocol` (`components`); behavior functions take `&mut ServerLevel`, player, hand, …
- **Level access traits** mirror vanilla interfaces (`BlockGetter`, `LevelReader`, `LevelAccessor`, `WorldGenLevel`)
  so block/feature code runs against both `ServerLevel` and `WorldGenRegion`.
- **Registration without central lists:** blocks/items/entities/commands/argument types/menus/etc. register through
  `linkme` distributed slices from the module that implements them, so contributors never edit one shared table.
- **Data-driven content** (recipes, loot tables, advancements, worldgen JSON, tags, damage types, enchantments,
  dimension types, …) is loaded from the vanilla data pack + world data packs through one `ReloadableServerResources`
  / registry-loading path, with vanilla codec semantics (unknown/invalid entries rejected or logged like vanilla).

## Contributing

Workflow, commit format, local checks and the review checklist are in [CONTRIBUTING.md](CONTRIBUTING.md); code
standards are in [docs/standards.md](docs/standards.md) and test tiers in [docs/testing.md](docs/testing.md). Builds read
`ref/extracted`, `ref/generated`, ... so `ref/` must be set up in every checkout (`scripts/setup-ref.sh`).

## Conventions

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
5. **Shared spine files** (ServerLevel, the Entity trait, BlockBehaviour, ServerPlayer) are touched by many changes:
   keep edits to them small and prefer putting new code in its own module.
6. Edition 2024, stable toolchain. `unsafe` only with a `// SAFETY:` comment (docs/standards.md). No `unwrap()` on
   network/disk input — malformed input must produce the same outcome as vanilla (usually a
   disconnect with vanilla's message).
7. Read only the vanilla ranges you need; run targeted tests while iterating and the full suite once before pushing.
8. **Java gotchas that leak into observable output:** `Math.round` is `floor(x + 0.5)` (Rust `round` differs at
   .5); `Double/Float.toString` → `mc_core::math::java::{java_double_to_string, java_float_to_string}` (never `{}`);
   `String.hashCode` → `mc_core::math::java`; `Mth.sin/cos` use the lookup table; Java `HashMap`/`HashSet`
   iteration order can be observable (vanilla code that iterates them must be reproduced, `LinkedHashMap` = insertion order).
