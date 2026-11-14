//! `net.minecraft.world.level.levelgen.PatrolSpawner`: every 12000 to 13200 ticks, by day and one time in five, a pillager patrol
//! (a banner-carrying leader plus a group sized by the local difficulty) appears 24..47 blocks from a random player.

use linkme::distributed_slice;
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_data::EntityType;
use mc_data::enums::EntitySpawnReason;
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::attribute::attributes::CAN_PILLAGER_PATROL_SPAWN;
use mc_game_core::world::entity::factory::{self, EntitySpawnRequest};
use mc_game_core::world::entity::monster::patrolling_monster;
use mc_game_core::world::entity::movement::MovementLevel;
use mc_game_core::world::level::BlockGetter;
use mc_game_core::world::level::server_level_accessor::ServerLevelAccessor;
use mc_game_core::world::level::spawner::custom_spawner::{
    CUSTOM_SPAWNERS, CustomSpawner, CustomSpawnerRegistration, delay_option, order,
};
use mc_game_core::world::level::spawner::natural_spawner;
use mc_world::heightmap::HeightmapType;

/// `PatrolSpawner`.
#[derive(Default)]
pub struct PatrolSpawner {
    next_tick: i32,
}

impl CustomSpawner for PatrolSpawner {
    fn tick(&mut self, level: &mut ServerLevel, spawn_enemies: bool) -> i32 {
        if !spawn_enemies || !level.spawn_patrols() {
            return 0;
        }
        // `RandomSource random = level.getRandom()`: the level's random is a shared handle
        let mut random = level.tick_state.random.clone();
        self.next_tick -= 1;
        if self.next_tick > 0 {
            return 0;
        }
        self.next_tick += 12000 + random.next_int_bounded(1200);
        if !level.is_bright_outside() || random.next_int_bounded(5) != 0 {
            return 0;
        }
        let player_count = level.players().len();
        if player_count < 1 {
            return 0;
        }
        let player = level.players()[random.next_int_bounded(player_count as i32) as usize];
        let Some((is_spectator, player_pos)) = level
            .get_entity(player)
            .map(|p| (p.is_spectator(), p.block_position()))
        else {
            return 0;
        };
        if is_spectator || level.is_close_to_village(player_pos, 2) {
            return 0;
        }
        let x = (24 + random.next_int_bounded(24)) * if random.next_boolean() { -1 } else { 1 };
        let z = (24 + random.next_int_bounded(24)) * if random.next_boolean() { -1 } else { 1 };
        let mut spawn_pos = BlockPos::new(player_pos.x() + x, player_pos.y(), player_pos.z() + z);
        if !level.has_chunks_at(
            spawn_pos.x() - 10,
            spawn_pos.z() - 10,
            spawn_pos.x() + 10,
            spawn_pos.z() + 10,
        ) || !level.attribute_bool(CAN_PILLAGER_PATROL_SPAWN, spawn_pos)
        {
            return 0;
        }
        let group_size = f64::from(
            ServerLevelAccessor::get_current_difficulty_at(&*level, spawn_pos)
                .get_effective_difficulty(),
        )
        .ceil() as i32
            + 1;
        let mut count = 0;
        for i in 0..group_size {
            spawn_pos = BlockPos::new(
                spawn_pos.x(),
                level
                    .get_heightmap_pos(HeightmapType::MotionBlockingNoLeaves, spawn_pos)
                    .y(),
                spawn_pos.z(),
            );
            if i == 0 {
                if !spawn_patrol_member(level, spawn_pos, &mut random, true) {
                    break;
                }
                count += 1;
            } else if spawn_patrol_member(level, spawn_pos, &mut random, false) {
                count += 1;
            }
            spawn_pos = BlockPos::new(
                spawn_pos.x() + random.next_int_bounded(5) - random.next_int_bounded(5),
                spawn_pos.y(),
                spawn_pos.z() + random.next_int_bounded(5) - random.next_int_bounded(5),
            );
        }
        count
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

/// `spawnPatrolMember(level, pos, random, isLeader)`.
fn spawn_patrol_member(
    level: &mut ServerLevel,
    pos: BlockPos,
    random: &mut dyn RandomSource,
    is_leader: bool,
) -> bool {
    let state = level.get_block_state(pos);
    if !natural_spawner::is_valid_empty_spawn_block(
        &*level,
        pos,
        state,
        level.get_fluid_state(pos),
        EntityType::PILLAGER,
    ) || !patrolling_monster::check_patrolling_monster_spawn_rules(
        EntityType::PILLAGER,
        &*level,
        EntitySpawnReason::Patrol,
        pos,
        random,
    ) {
        return false;
    }
    let Some(mut mob) = factory::create(
        EntityType::PILLAGER,
        level,
        EntitySpawnRequest {
            reason: EntitySpawnReason::Patrol,
            ignore_checks: false,
        },
    ) else {
        return false;
    };
    if is_leader {
        patrolling_monster::set_patrol_leader(mob.as_mut(), true);
        patrolling_monster::find_patrol_target(mob.as_mut());
    }
    mob.set_pos(f64::from(pos.x()), f64::from(pos.y()), f64::from(pos.z()));
    let difficulty = ServerLevelAccessor::get_current_difficulty_at(&*level, pos);
    mob.finalize_spawn(level, difficulty, EntitySpawnReason::Patrol, None);
    level.add_fresh_entity_with_passengers(vec![mob]);
    true
}

#[distributed_slice(CUSTOM_SPAWNERS)]
static PATROL_SPAWNER: CustomSpawnerRegistration = CustomSpawnerRegistration {
    order: order::PATROL,
    create: |_| Some(Box::new(PatrolSpawner::default())),
};
