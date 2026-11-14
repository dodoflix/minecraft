//! `net.minecraft.world.entity.npc.CatSpawner` (next to the cat it spawns): every 1200 ticks a cat appears 8..31 blocks from a random player, inside a
//! village (fewer than five cats near more than four occupied homes) or in a swamp hut (persistent).

use linkme::distributed_slice;
use mc_core::ident::Identifier;
use mc_core::phys::AABB;
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_data::EntityType;
use mc_data::enums::EntitySpawnReason;
use mc_game_core::advancements::predicates::location_predicate::has_structure_piece_at;
use mc_game_core::registry::keys;
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::entity::ai::village::poi::Occupancy;
use mc_game_core::world::entity::factory::{self, EntitySpawnRequest};
use mc_game_core::world::entity::movement::MovementLevel;
use mc_game_core::world::entity::spawn_placements;
use mc_game_core::world::level::server_level_accessor::ServerLevelAccessor;
use mc_game_core::world::level::spawner::custom_spawner::{
    CUSTOM_SPAWNERS, CustomSpawner, CustomSpawnerRegistration, delay_option, order,
};

use super::cat::Cat;

/// `CatSpawner`.
#[derive(Default)]
pub struct CatSpawner {
    next_tick: i32,
}

impl CustomSpawner for CatSpawner {
    fn tick(&mut self, level: &mut ServerLevel, _spawn_enemies: bool) -> i32 {
        self.next_tick -= 1;
        if self.next_tick > 0 {
            return 0;
        }
        self.next_tick = 1200;
        let Some(player) = level.get_random_player() else {
            return 0;
        };
        let Some(player_pos) = level.get_entity(player).map(|p| p.block_position()) else {
            return 0;
        };
        let mut random = level.tick_state.random.clone();
        let x = (8 + random.next_int_bounded(24)) * if random.next_boolean() { -1 } else { 1 };
        let z = (8 + random.next_int_bounded(24)) * if random.next_boolean() { -1 } else { 1 };
        let spawn_pos = BlockPos::new(player_pos.x() + x, player_pos.y(), player_pos.z() + z);
        if !level.has_chunks_at(
            spawn_pos.x() - 10,
            spawn_pos.z() - 10,
            spawn_pos.x() + 10,
            spawn_pos.z() + 10,
        ) || !spawn_placements::is_spawn_position_ok(EntityType::CAT, &*level, spawn_pos)
        {
            return 0;
        }
        if level.is_close_to_village(spawn_pos, 2) {
            spawn_in_village(level, spawn_pos)
        } else if in_cats_spawn_in_structure(level, spawn_pos) {
            spawn_in_hut(level, spawn_pos)
        } else {
            0
        }
    }

    fn replay_setup(
        &mut self,
        _level: &mut ServerLevel,
        _seed: i64,
        options: &std::collections::BTreeMap<String, String>,
    ) {
        if let Some(delay) = delay_option(options) {
            self.next_tick = delay;
        }
    }
}

/// `structureManager().getStructureWithPieceAt(pos, StructureTags.CATS_SPAWN_IN).isValid()`.
fn in_cats_spawn_in_structure(level: &ServerLevel, pos: BlockPos) -> bool {
    let tag = Identifier::parse("minecraft:cats_spawn_in").expect("valid tag");
    has_structure_piece_at(level, pos, &|id| {
        level.registries.is_in_tag(keys::STRUCTURE, id, &tag)
    })
}

fn count_cats(level: &ServerLevel, pos: BlockPos, x: f64, z: f64) -> usize {
    let bb = AABB::from_block_pos(&pos).inflate(x, 8.0, z);
    level
        .get_entities_of_class(&bb, |e| e.as_any().is::<Cat>(), |_| true)
        .len()
}

/// `spawnInVillage(level, spawnPos)`.
fn spawn_in_village(level: &mut ServerLevel, spawn_pos: BlockPos) -> i32 {
    let homes = level.poi_manager().get_count_in_range(
        &|t| t.registered_name() == "minecraft:home",
        spawn_pos,
        48,
        Occupancy::IsOccupied,
    );
    if homes > 4 && count_cats(level, spawn_pos, 48.0, 48.0) < 5 {
        return spawn_cat(level, spawn_pos, false);
    }
    0
}

/// `spawnInHut(level, spawnPos)`.
fn spawn_in_hut(level: &mut ServerLevel, spawn_pos: BlockPos) -> i32 {
    if count_cats(level, spawn_pos, 16.0, 16.0) == 0 {
        return spawn_cat(level, spawn_pos, true);
    }
    0
}

/// `spawnCat(spawnPos, level, makePersistent)`.
fn spawn_cat(level: &mut ServerLevel, spawn_pos: BlockPos, make_persistent: bool) -> i32 {
    let Some(mut cat) = factory::create(
        EntityType::CAT,
        level,
        EntitySpawnRequest {
            reason: EntitySpawnReason::Natural,
            ignore_checks: false,
        },
    ) else {
        return 0;
    };
    let difficulty = ServerLevelAccessor::get_current_difficulty_at(&*level, spawn_pos);
    cat.finalize_spawn(level, difficulty, EntitySpawnReason::Natural, None);
    if make_persistent {
        cat.set_persistence_required();
    }
    cat.snap_to(
        f64::from(spawn_pos.x()) + 0.5,
        f64::from(spawn_pos.y()),
        f64::from(spawn_pos.z()) + 0.5,
        0.0,
        0.0,
    );
    level.add_fresh_entity_with_passengers(vec![cat]);
    1
}

#[distributed_slice(CUSTOM_SPAWNERS)]
static CAT_SPAWNER: CustomSpawnerRegistration = CustomSpawnerRegistration {
    order: order::CAT,
    create: |_| Some(Box::new(CatSpawner::default())),
};
