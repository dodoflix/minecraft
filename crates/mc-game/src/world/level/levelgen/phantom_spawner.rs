//! `net.minecraft.world.level.levelgen.PhantomSpawner`: spawns phantoms above players that did not rest for a long time.

use crate::server::level::ServerLevel;
use crate::stats::stats::Stats;
use crate::world::entity::ServerPlayer;
use crate::world::entity::factory::EntitySpawnRequest;
use crate::world::level::BlockGetter;
use crate::world::level::server_level_accessor::ServerLevelAccessor;
use crate::world::level::spawner::custom_spawner::{CUSTOM_SPAWNERS, CustomSpawner, CustomSpawnerRegistration, order};
use crate::world::level::spawner::natural_spawner;
use crate::world::level::LevelReader;
use linkme::distributed_slice;
use mc_core::math::mth;
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_data::EntityType;
use mc_data::enums::EntitySpawnReason;

/// `PhantomSpawner`.
#[derive(Default)]
pub struct PhantomSpawner {
    next_tick: i32,
}

impl CustomSpawner for PhantomSpawner {
    fn tick(&mut self, level: &mut ServerLevel, spawn_enemies: bool) -> i32 {
        let has_sky_light = level.chunk_source.chunk_map.has_sky_light;
        if !spawn_enemies || !level.spawn_phantoms() {
            return 0;
        }
        self.next_tick -= 1;
        if self.next_tick > 0 {
            return 0;
        }
        // `RandomSource random = level.getRandom()`: the level's random is a shared handle
        let mut random = level.tick_state.random.clone();
        self.next_tick += (60 + random.next_int_bounded(60)) * 20;
        if level.sky_darken < 5 && has_sky_light {
            return 0;
        }
        let mut count = 0;
        let players: Vec<_> = level.players().to_vec();
        for player_id in players {
            let Some((is_spectator, player_pos, time_since_rest)) = level.get_entity(player_id).map(|p| {
                let time = p.as_any().downcast_ref::<ServerPlayer>().map_or(0, |sp| sp.stats.lock().get_value(&Stats::TIME_SINCE_REST));
                (p.is_spectator(), p.block_position(), time)
            }) else {
                continue;
            };
            if is_spectator {
                continue;
            }
            if has_sky_light && !(player_pos.y() >= level.get_sea_level() && level.can_see_sky(player_pos)) {
                continue;
            }
            let difficulty = ServerLevelAccessor::get_current_difficulty_at(&*level, player_pos);
            if !difficulty.is_harder_than(random.next_float() * 3.0) {
                continue;
            }
            let value = mth::clamp_i32(time_since_rest, 1, i32::MAX);
            if random.next_int_bounded(value) < 72000 {
                continue;
            }
            let spawn_pos = player_pos.above_n(20 + random.next_int_bounded(15)).east_n(-10 + random.next_int_bounded(21)).south_n(-10 + random.next_int_bounded(21));
            let block_state = level.get_block_state(spawn_pos);
            let fluid_state = level.get_fluid_state(spawn_pos);
            if !natural_spawner::is_valid_empty_spawn_block(&*level, spawn_pos, block_state, fluid_state, EntityType::PHANTOM) {
                continue;
            }
            let mut group_data = None;
            let group_size = 1 + random.next_int_bounded(difficulty.get_difficulty().get_id() + 1);
            for _ in 0..group_size {
                let Some(mut phantom) = crate::world::entity::factory::create(EntityType::PHANTOM, level, EntitySpawnRequest { reason: EntitySpawnReason::Natural, ignore_checks: false }) else { continue };
                phantom.snap_to(f64::from(spawn_pos.x()) + 0.5, f64::from(spawn_pos.y()), f64::from(spawn_pos.z()) + 0.5, 0.0, 0.0);
                group_data = phantom.finalize_spawn(level, difficulty, EntitySpawnReason::Natural, group_data);
                level.add_fresh_entity_with_passengers(vec![phantom]);
                count += 1;
            }
        }
        count
    }
}

#[distributed_slice(CUSTOM_SPAWNERS)]
static PHANTOM_SPAWNER: CustomSpawnerRegistration = CustomSpawnerRegistration { order: order::PHANTOM, create: |_| Some(Box::new(PhantomSpawner::default())) };
