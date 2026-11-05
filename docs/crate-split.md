# Splitting `mc-game` into crates

Why: `mc-game` was ~305k lines in one crate, so every gameplay edit recompiled all of it (this capped parallel agents at ~2).
After the split, an edit to one block / mob / command / feature recompiles one 10-25k line crate plus the thin `mc-game` on top.

Rules of the split: no behavior change, no renamed concepts, every historical `mc_game::...` path keeps working,
files keep their relative paths (`crates/mc-game/src/world/entity/animal/sheep/..` becomes
`crates/mc-game-animals/src/world/entity/animal/sheep/..`), so git rename detection carries the ~180 feature branches over.

## Crates

| crate | lines | contents |
|---|---|---|
| `mc-game-core` | 210k | the spine: `MinecraftServer`, `ServerLevel`, chunk map, network/players, `Entity`/`LivingEntity`/`Mob`/`Player` + AI framework (brain, memory, navigation, controls), `BlockBehaviour` + block state/helpers, item base + enchantments + crafting + menus, loot, advancements, command infra + arguments, level traits, storage, scoreboard, raids, and everything the lower-level files above reference |
| `mc-game-blocks` | 19.6k | block behavior implementations (`world/level/block/impls`) and block entities (`world/block_entity`) |
| `mc-game-levelgen` | 26.9k | features, structures, placement, pieces, template processors (`world/level/levelgen/{feature,structure,..}`), `/place` |
| `mc-game-cmds` | 12.2k | the vanilla commands (`server/commands`), registered through the `COMMANDS` linkme slice |
| `mc-game-ai` | 5.5k | the goals/behaviors/sensors that nothing in core names (`world/entity/ai/{goal,behavior,sensing,village}`; most concrete goals are pinned in core, see Pins) |
| `mc-game-animals` | 17.9k | `world/entity/animal`, `world/entity/ambient` (+ the goals/behaviors only they use) |
| `mc-game-monsters` | 14.1k | `world/entity/{monster,npc,raid parts}` (+ warden/villager AI, creaking heart block) |
| `mc-game` | thin | `src/lib.rs` merges the module trees of all crates (the public API) + `tests/` + `examples/` |

The file -> crate map is the file tree itself (every file kept its path under `src/`).

Dependency DAG (arrow = depends on; `mc-server`/`mc-testbot` depend on `mc-game` only):

```
mc-game ──► mc-game-blocks ───────────────┐
        ├─► mc-game-levelgen ─────────────┤
        ├─► mc-game-cmds ─────────────────┼─► mc-game-core ─► mc-core, mc-nbt, mc-protocol, mc-world, ...
        └─► mc-game-monsters ─► mc-game-animals ─► mc-game-ai ─┘
```

`blocks`, `levelgen` and `cmds` are leaves: nothing but `mc-game` depends on them. Editing a block recompiles
`mc-game-blocks` + `mc-game`; a command: `mc-game-cmds` + `mc-game`; a monster: `mc-game-monsters` + `mc-game`; an animal:
`animals` + `monsters` + `mc-game`. Dependency rule (lower never imports higher): `core` < `blocks`, `levelgen`, `cmds`, `ai` <
`animals` < `monsters` < `mc-game`; sibling crates never depend on each other.

## How the module tree stays intact (the "mirror")

All code uses `crate::world::...` paths. Moved files keep their text; every area crate **mirrors the module tree of its parent
crate**, so the same paths resolve:

* each area crate has a dependency named `parent` (`parent = { package = "mc-game-core", ... }`; `animals`' parent is `ai`,
  `monsters`' parent is `animals`), its `lib.rs` is `pub use parent::*;` plus `pub mod <top-level modules it extends>;`
* every module that has children in the crate but is *declared* in a lower crate gets a shim `mod.rs`:
  `pub use parent::world::entity::*; pub mod animal; ...` (explicit local modules shadow glob imports). Declarations of the moved
  children (`pub mod x;` / `pub use x::..;`) leave the lower crate's `mod.rs` and live in the shim.
* a module's file always lives in the lowest crate that owns any file below it (`owner(parent) <= owner(child)`), so a `mod x;`
  never has to cross a crate boundary.
* `mc-game/src/lib.rs` merges all trees: `pub use mc_game_core::*;` plus explicit modules where two area crates extend the same
  module (`world`, `world::level`, `world::level::block::impls`, ...).

Consequences of crate boundaries that needed edits (listed per crate in "Seams" below): `pub(crate)`/`pub(super)` items used by
moved files become `pub`; inherent `impl Foreign {}` blocks are not allowed across crates.

## How the assignment was derived (and why the cycles resolve)

A static analysis of `use crate::...`/`crate::..` paths (re-exports followed) over all 1998 files showed one giant strongly
connected component at directory granularity, but at *file* granularity only ~100 references point "up" for a given layering.
Files were assigned by directory rules (`blocks` = block impls + block entities, `levelgen` = features/structures, ...), then every
upward reference was resolved by the cheaper of (a) lowering the referenced file into the referencing crate's parent or (b) raising the
referencing file, preferring to keep lines out of `core` (for instance `world/entity/ai/behavior/warden/*` was raised into
`monsters` rather than dragging the warden into `ai`). References from `ai`/`animals`/`monsters`/`levelgen` to a few block
implementations (door, crop, bell, composter, vine, ...) were resolved by pinning those 14 small block files into `core`, which makes
`blocks` a true leaf.

What stays in `core` on purpose (referenced from the spine, so it cannot sit above it): the item base (`ItemStack` extensions,
`BlockItem`, enchantments, crafting/recipes, trading), menus, the AI framework, loot, advancements, command arguments and the
`Zombie`/`CubeMob`/raid data types that the `Entity` trait exposes (`as_zombie`, `raider`, `spellcaster`), parrot (shoulder),
`MinecraftServer` + `DedicatedServer`, gametest ticker.

## Seams

References that could not be resolved by moving files (each is a small hand edit):

* `blocks`: the `Spawner` trait (named by the core `BlockEntity` trait) moved from `spawner_block_entity.rs` into core's
  `block_entity/mod.rs` (the old path re-exports it); `piston_moving_block_entity.rs` had an inherent `impl ServerLevel`
  (E0116): `moving_piston_info` / `final_tick_moving_piston` became the extension trait `MovingPistonLevelExt`, imported by
  `piston_base_block.rs`; textual `macro_rules!` used across files (`vegetation_behaviour*`, `spreading_bonemeal_methods`,
  `diode_behaviour`, `rail_behaviour`, `sign_block_common`, `double_plant_behaviour_methods`, `update_shape`,
  `block_entity_base`, `impl_items_container`) are `#[macro_export]` and `mc-game-blocks/src/lib.rs` has
  `#[macro_use] extern crate parent;`; `impls::prelude`, `read_codec`, `dispenser_block_entity`, `sculk_sensor_block_entity` are `pub`.
* `levelgen`: `stateproviders/simple_state_provider.rs` and `blockpredicates/state_testing_predicate.rs` are pinned in core (core calls
  `SimpleStateProvider`; `impl<T: StateTestingPredicate> BlockPredicate for T` is an orphan impl outside core); the factory
  functions of `blockpredicates/mod.rs` and the five `TRUNK/FOLIAGE/ROOT/TREE_DECORATOR/FEATURE_SIZE` slices + `block_tag_fn!` of
  `feature/tree/mod.rs` moved from the core `mod.rs` into the levelgen shims (they name levelgen types).
* `cmds`: visibility only (`packs::throwable`, `MinecraftServer::{saved_types, loot_random_sequences}`, `ScoreHolder::of_entity`).
* `monsters`: `macro_rules! block_entity_base` / `update_shape` made `#[macro_export]` (+ `impls::prelude` and `block_entity::read_codec` `pub`) so
  the creaking heart files import them by path across the crate boundary.
* `ai`: `goal/target/mod.rs` declares `target_goal` as `pub mod` (the moved target goals import `super::target_goal`; private child
  modules are not visible through the shim's glob re-export); `goal/tests.rs` imports `Entity` itself (it relied on
  `use super::*` seeing the private import of `goal/mod.rs`).
* `animals`: `macro_rules! model_type` made `#[macro_export]`; the generated `animal/mod.rs` shim does not declare `bee` (its
  `pub mod bee;` is commented out in the original: the file is dead code that never compiled).

### Pins (files held back in `core` against `assignment.txt`)

`assignment.txt` followed `crate::` paths only; references by local name (`drowned::..`, `pub use villager::Villager`,
`super::yield_job_site`) from files that stay in `core`, and orphan-rule violations, were missed. These files are therefore `core`
(the lowest crate that compiles); they block a later carve-out of the listed users from `core`.

| files (`world/entity/..`) | pinned because |
|---|---|
| `monster/zombie/{base,drowned,goals,husk,zombie_villager,zombified_piglin}.rs` | `monster/zombie/mod.rs` (core: the `Zombie` type behind `as_zombie`) calls them by local module name |
| `npc/villager/{villager,abstract_villager,villager_type}.rs`, `npc/inventory_carrier.rs` | `npc/villager/mod.rs::villager_ref/villager_mut` (used by ai behaviors and the core `SecondaryPoiSensor`) name `Villager`; `villager.rs` uses `inventory_carrier` |
| `ai/behavior/{villager_goal_packages,poi_competitor_scan,trade_with_villager,villager_make_love,yield_job_site}.rs` | `villager.rs` builds its brain from `villager_goal_packages`, which pulls the other four |
| `animal/chicken/*`, `animal/temperature_variants.rs` | `zombie/base.rs` (chicken jockey) downcasts to `Chicken`; `chicken_variant` uses `temperature_variants` |
| `animal/equine/{abstract_horse,mod}.rs` | `abstract_horse.rs` has blanket `impl<T: AbstractHorse> MountInventory/AbstractHorseAccess for T` (core traits): an orphan-rule violation unless the `AbstractHorse` trait lives in the same crate as those traits; `mod.rs` follows (parent module must not be above its child) |
| `ai/goal/{break_door,breed,door_interact,follow_parent,melee_attack,move_back_to_village,move_through_village,move_to_block,random_look_around,random_stand,ranged_attack,remove_block,run_around_like_crazy,tempt,zombie_attack}.rs`, `ai/goal/target/hurt_by_target_goal.rs` | named by the pinned zombie goals / chicken / abstract horse / villager files above (the goal framework is core, these concrete goals are their dependencies) |
| `ai/behavior/{assign_profession_from_job_site,behavior_utils,celebrate_villagers_survived_raid,do_nothing,gate_behavior,give_gift_to_hero,go_to_closest_village,go_to_potential_job_site,go_to_wanted_item,harvest_farmland,inside_brownian_walk,interact_with,interact_with_door,jump_on_bed,locate_hiding_place,look_and_follow_trading_player_sink,look_at_target_sink,move_to_sky_seeing_spot,move_to_target_sink,play_tag_with_other_kids,react_to_bell,reset_profession,reset_raid_status,ring_bell,set_closest_home_as_walk_target,set_entity_look_target,set_hidden_state,set_look_and_interact,set_raid_status,set_walk_target_away_from,set_walk_target_from_block_memory,set_walk_target_from_look_target,show_trades_to_player,shuffling_list,sleep_in_bed,socialize_at_bell,stroll_around_poi,stroll_to_poi,stroll_to_poi_list,swim,trigger_gate,use_bonemeal,validate_nearby_poi,village_bound_random_stroll,villager_calm_down,villager_panic_trigger,wake_up,work_at_composter,work_at_poi}.rs` | the pinned `villager_goal_packages.rs` (and `villager.rs`) name them |
| `npc/villager/villager_data.rs` | `villager.rs` / `villager_type.rs` import `super::villager_data` |

## linkme

`#[distributed_slice]` statics in a crate nothing references are dropped by the linker. `mc-game` re-exports modules of every area
crate (so rustc links them); `crates/mc-game/tests/registration_counts.rs` prints and asserts the length of every slice
(values captured before the split) and fails if a crate stops being linked.

## Integration tests

A test moves to `mc-game-core` / `mc-game-ai` only if it passes there with the same results as before: most tests build a
`MinecraftServer`/`ServerLevel`, which needs the registrations (blocks, features, commands, entity factories, ...) of the area
crates, and those only exist in the linked-everything `mc-game`. Of 231 test binaries, 54 run on `mc-game-core` alone and moved
(`crates/mc-game-core/tests/`), `brain_golden` runs on `mc-game-ai`; the rest stay in `crates/mc-game/tests/`. Shared helpers
(`common`, `args_common`, ...) and `fixtures/`, `oracle/` stay in `crates/mc-game/tests/` (most users and every fixture-generating script
live there); moved tests include the helpers with `#[path = "../../mc-game/tests/<dir>/mod.rs"]` and read fixtures through
`../mc-game/tests/..`. Each moved test starts with `extern crate mc_game_core as mc_game;`, so its text is otherwise unchanged.
Fixtures used only by `src/` unit tests (`include_str!`) live in `crates/mc-game-core/tests/fixtures`.

## Further carve-outs of `core` (not done)

`core` is 210k lines (69%): an edit there recompiles all of it. Measured on the file-level reference graph (files of `core` outside the
candidate that name it; "refs" = distinct referencing files):

| candidate | lines | refs | what blocks it | seam it would need |
|---|---|---|---|---|
| items + menus + recipes (`world/item`, `world/inventory`, `world/container*`) | 23.5k | 186 (blocks 37, projectile 19, player 15, living_entity 13, mob 9, loot 9) | `ItemStack` extensions/`UseOnContext`/`InteractionResult` are used by player, living, mob, block base and `server/level/server_player_game_mode`; recipes/menus are named by `ServerPlayer` | keep the item *base* (`stack_ext`, `context`, `interaction`, `behaviour`) in core and move only recipes/trading/enchantment-effect evaluators behind the existing `ITEM_SYSTEMS`/`PLAYER_SYSTEMS` hook slices (`server/network/game_packets/hooks.rs`) |
| loot (`world/level/storage/loot`) | 7.6k | 54 | `LootParams` is built by blocks/entities/commands/advancements; `LootContext` holds `&ServerLevel` | trait `LootTableSource` in core implemented by loot; drop callers call it through `ServerLevel` |
| advancements (+ triggers, predicates) | 7.2k | 41 (player 10, living_entity 3) | `ServerPlayer`/living call `trigger_*` directly; predicates need `Entity`/`ServerLevel` | `AdvancementHooks` trait object on the level/player; triggers already register through the `TRIGGER_TYPES` slice |
| scoreboard (`world/scores`) | 2.4k | 13 | `ServerPlayer`, `MinecraftServer` state glue, command arguments | trait for the few score reads/writes used by entities |
| raids (`world/entity/raid`) | 2.3k | 7 | `Entity::{raider, apply_raid_buffs}` accessors name `RaiderData`/`Raid`; `level_tick::tick_raids`; one advancement predicate | `dyn Any` accessors on `Entity` + a `LEVEL_TICK` hook slice; unpins `patrolling_monster`/illagers |
| projectile + decoration + vehicle | 8.9k | 21 (block 7) | `Projectile` is named by block base and enchantment code; arrow/bow items | move the `Projectile`/`Hanging` traits to core, rest above |
| pathfinder | 2.6k | 15 | `PathNavigation` in `Mob` | navigation behind a trait object |
| network + players + chat | 13.3k | 49 | `ServerPlayer.connection`, `MinecraftServer` fields | none cheap: it is the spine |
| pinned mob code (zombie, villager, chicken, equine, ~60 concrete goals/behaviors) | ~15k | see Pins | `Entity` trait accessors (`as_zombie`, `villager_ref`, ...) and core brain/POI sensors name concrete types; orphan blanket impls | `dyn Any` accessors instead of concrete types in the `Entity` trait; blanket impls re-homed with their traits |

## Removal

This layout is a bridge that keeps the ~180 feature branches rebasable (files keep their paths and text). The end state is plain
Rust: explicit dependencies and imports, no mirror. Mechanical steps, once the branches are migrated:

1. In each area crate delete every shim (`mod.rs`/`lib.rs` carrying the `// bridge:` header), drop the `parent` dependency and depend
   on the real crates (`mc-game-core`, `mc-game-ai`, ...) by name; the crate roots declare only the crate's own modules.
2. In moved files rewrite `crate::<module owned by another crate>::..` to `mc_game_core::..` (or the owning crate); `crate::` then
   means only the crate's own modules. Where a file referred to a lower-crate module through a shim (`use super::*`), import
   explicitly. Drop `#[macro_use] extern crate parent;` and import the exported macros by path.
3. Replace `mc-game/src/lib.rs` (the generated merge) by explicit re-exports of the public API, or delete it and make `mc-server`,
   `mc-testbot` and the tests depend on the crates they use; `registration_counts` keeps guarding the linkme links (each crate must
   still be referenced from the binary).
4. Narrow the visibility again: every item widened for the split is in the commits titled `... (shims, visibility, ...)`; make
   an item `pub(crate)`/private again when all its users are in its own crate. Move the pinned files (table above) out of `core`
   by giving the core call sites a trait/registry seam, then carve the owning area crate for real.
5. Move helper dirs/fixtures next to the tests that use them and drop the `#[path]`/`../mc-game/tests` indirections.

## Landing order

1. baseline (pass/fail set, timings) on untouched main;
2. `mc-game` -> `mc-game-core` rename (pure move), thin `mc-game`, empty area crates and the merged `mc-game/src/lib.rs`;
3. per area crate: `git mv` commit, then a shim/visibility/seam commit, `check --workspace --tests`, merge (`cmds`, `levelgen`,
   `blocks`, then the chain `monsters`, `animals`, `ai`: a crate can only be carved once everything above it is out of `core`);
4. tests relocation; scripts/CI; ARCHITECTURE.md + STATUS.md.
