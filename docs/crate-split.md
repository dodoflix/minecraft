# Splitting `mc-game` into crates

Why: `mc-game` was ~305k lines in one crate, so every gameplay edit recompiled all of it.
After the split, an edit to one block / mob / command / feature recompiles one 10-25k line crate plus the thin `mc-game` on top.

Rules of the split: no behavior change, no renamed concepts, every historical `mc_game::...` path keeps working,
files keep their relative paths (`crates/mc-game/src/world/entity/animal/sheep/..` becomes
`crates/mc-game-animals/src/world/entity/animal/sheep/..`), so git rename detection carries in-flight branches over.

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
| `mc-game` | thin | the facade: `src/lib.rs` re-exports the modules of all crates under the historical `mc_game::...` paths (the public API) + `tests/` + `examples/` |

The file -> crate map is the file tree itself (every file kept its path under `src/`).

Dependency DAG (arrow = depends on; `mc-server`/`mc-testbot` depend on `mc-game` only):

```
mc-game ──► mc-game-blocks ───────────────┐
        ├─► mc-game-levelgen ─────────────┤
        ├─► mc-game-cmds ─────────────────┼─► mc-game-core ─► mc-core, mc-nbt, mc-protocol, mc-world, ...
        ├─► mc-game-ai ───────────────────┤
        ├─► mc-game-animals ──► mc-game-ai ┤
        └─► mc-game-monsters ─► mc-game-ai ┘
```

`blocks`, `levelgen` and `cmds` are leaves: nothing but `mc-game` depends on them. Editing a block recompiles
`mc-game-blocks` + `mc-game`; a command: `mc-game-cmds` + `mc-game`; a monster: `mc-game-monsters` + `mc-game`; an animal:
`mc-game-animals` + `mc-game`. Dependency rule (lower never imports higher): `core` < `blocks`, `levelgen`, `cmds`, `ai` <
`animals`, `monsters` < `mc-game`; `blocks`, `levelgen`, `cmds`, `animals` and `monsters` never depend on each other.

## How paths resolve

Every area crate declares only the modules whose files it owns and imports everything else from the crate that owns it by
explicit path (`mc_game_core::world::entity::Entity`, `mc_game_ai::world::entity::ai::goal::...`); `crate::` means only the
crate's own modules. A module that has children in more than one crate (`world::entity`, `world::level::block::impls`, ...) exists
once per crate with the same path and holds only that crate's children.

* a module's file always lives in the lowest crate that owns any file below it (`owner(parent) <= owner(child)`), so a `mod x;`
  never has to cross a crate boundary.
* `mc-game/src/lib.rs` is the facade. It declares each module that more than one crate contributes to once and lists exactly
  those crates (`pub use mc_game_core::world::entity::*; pub use mc_game_animals::world::entity::*; ...`); everything else is
  reached through the parent's re-export. Binaries and integration tests import from `mc_game`.
* `#[macro_export]` macros of `core` (`register_block_class!`, `update_shape!`, ...) are imported by path
  (`use mc_game_core::register_block_class;`); a macro used by area crates names its items with `$crate::` (the defining crate).

Consequences of crate boundaries (listed per crate in "Seams" below): `pub(crate)`/`pub(super)` items used by moved files
are `pub`; inherent `impl Foreign {}` blocks are not allowed across crates.

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
  `block_entity_base`, `impl_items_container`) are `#[macro_export]` (their bodies name items with `$crate::`) and imported by
  path; `impls::prelude`, `read_codec`, `dispenser_block_entity`, `sculk_sensor_block_entity` are `pub`.
* `levelgen`: `stateproviders/simple_state_provider.rs` and `blockpredicates/state_testing_predicate.rs` are pinned in core (core calls
  `SimpleStateProvider`; `impl<T: StateTestingPredicate> BlockPredicate for T` is an orphan impl outside core); the factory
  functions of `blockpredicates/mod.rs` and the five `TRUNK/FOLIAGE/ROOT/TREE_DECORATOR/FEATURE_SIZE` slices + `block_tag_fn!` of
  `feature/tree/mod.rs` moved from the core `mod.rs` into the levelgen `mod.rs` files (they name levelgen types).
* `cmds`: visibility only (`packs::throwable`, `MinecraftServer::{saved_types, loot_random_sequences}`, `ScoreHolder::of_entity`).
* `monsters`: `macro_rules! block_entity_base` / `update_shape` made `#[macro_export]` (+ `impls::prelude` and `block_entity::read_codec` `pub`) so
  the creaking heart files import them by path across the crate boundary.
* `ai`: `goal/target/mod.rs` declares `target_goal` as `pub mod` (the moved target goals import it from `core` by path);
  `goal/tests.rs` imports what it uses.
* `animals`: `macro_rules! model_type` made `#[macro_export]`; `animal/mod.rs` does not declare `bee` (its
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

A test moves to `mc-game-core` only if it passes there with the same results as before: most tests build a
`MinecraftServer`/`ServerLevel`, which needs the registrations (blocks, features, commands, entity factories, ...) of the area
crates, and those only exist in the linked-everything `mc-game`. Of 231 test binaries, 55 run on `mc-game-core` alone and moved
(`crates/mc-game-core/tests/`, `brain_golden` included); the rest stay in `crates/mc-game/tests/`. Shared helpers
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
