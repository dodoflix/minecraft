//! `net.minecraft.world.entity.ai.village.VillageSiege`: on one night in ten (rolled at the `roll_village_siege` time marker of the default
//! clock) twenty zombies appear around a village, one every third tick, 32 blocks from the player who stands in it.

use crate::registry::keys;
use crate::server::level::ServerLevel;
use crate::world::clock::{ClockTimeMarker, ClockTimeMarkers, time_markers_from_timeline_json};
use crate::world::entity::factory::{self, EntitySpawnRequest};
use crate::world::entity::spawn_predicates::check_monster_spawn_rules;
use crate::world::level::LevelReader;
use crate::world::level::server_level_accessor::ServerLevelAccessor;
use crate::world::level::spawner::custom_spawner::{
    CUSTOM_SPAWNERS, CustomSpawner, CustomSpawnerRegistration, order,
};
use linkme::distributed_slice;
use mc_core::ident::Identifier;
use mc_core::math::mth;
use mc_core::phys::Vec3;
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_data::EntityType;
use mc_data::enums::EntitySpawnReason;
use mc_world::heightmap::HeightmapType;

/// `VillageSiege.State` (`SIEGE_CAN_ACTIVATE` is never assigned).
#[derive(Clone, Copy, PartialEq, Eq)]
enum State {
    SiegeTonight,
    SiegeDone,
}

/// `VillageSiege`.
pub struct VillageSiege {
    /// The `roll_village_siege` marker of the dimension's default clock, looked up on the first tick (`Some(None)` when the dimension has no default
    /// clock or no timeline defines the marker).
    roll_marker: Option<Option<(Identifier, ClockTimeMarker)>>,
    has_setup_siege: bool,
    siege_state: State,
    zombies_to_spawn: i32,
    next_spawn_time: i32,
    spawn_x: i32,
    spawn_y: i32,
    spawn_z: i32,
}

impl VillageSiege {
    fn new() -> VillageSiege {
        VillageSiege {
            roll_marker: None,
            has_setup_siege: false,
            siege_state: State::SiegeDone,
            zombies_to_spawn: 0,
            next_spawn_time: 0,
            spawn_x: 0,
            spawn_y: 0,
            spawn_z: 0,
        }
    }

    /// `level.clockManager().isAtTimeMarker(defaultClock, ROLL_VILLAGE_SIEGE)`.
    fn at_roll_time(&mut self, level: &ServerLevel) -> bool {
        self.roll_marker
            .get_or_insert_with(|| roll_marker(level))
            .as_ref()
            .is_some_and(|(clock, marker)| {
                marker.occurs_at(level.tick_state.attributes.clock_total_ticks(clock))
            })
    }

    /// `tryToSetupSiege(level)`.
    fn try_to_setup_siege(&mut self, level: &mut ServerLevel) -> bool {
        let mut random = level.tick_state.random.clone();
        let players: Vec<_> = level.players().to_vec();
        for player in players {
            let Some((spectator, center)) = level
                .get_entity(player)
                .map(|p| (p.is_spectator(), p.block_position()))
            else {
                continue;
            };
            if spectator || !level.is_village(center) || without_zombie_sieges(level, center) {
                continue;
            }
            for _ in 0..10 {
                let angle = random.next_float() * (std::f64::consts::PI * 2.0) as f32;
                self.spawn_x = center.x() + mth::floor_f32(mth::cos(f64::from(angle)) * 32.0);
                self.spawn_y = center.y();
                self.spawn_z = center.z() + mth::floor_f32(mth::sin(f64::from(angle)) * 32.0);
                let around = BlockPos::new(self.spawn_x, self.spawn_y, self.spawn_z);
                if find_random_spawn_pos(level, around, &mut random).is_some() {
                    self.next_spawn_time = 0;
                    self.zombies_to_spawn = 20;
                    break;
                }
            }
            return true;
        }
        false
    }

    /// `trySpawn(level)`.
    fn try_spawn(&self, level: &mut ServerLevel) {
        let mut random = level.tick_state.random.clone();
        let around = BlockPos::new(self.spawn_x, self.spawn_y, self.spawn_z);
        let Some(pos) = find_random_spawn_pos(level, around, &mut random) else {
            return;
        };
        let Some(mut zombie) = factory::create(
            EntityType::ZOMBIE,
            level,
            EntitySpawnRequest {
                reason: EntitySpawnReason::Event,
                ignore_checks: true,
            },
        ) else {
            return;
        };
        let difficulty =
            ServerLevelAccessor::get_current_difficulty_at(&*level, zombie.block_position());
        zombie.finalize_spawn(level, difficulty, EntitySpawnReason::Event, None);
        zombie.snap_to(pos.x, pos.y, pos.z, random.next_float() * 360.0, 0.0);
        level.add_fresh_entity_with_passengers(vec![zombie]);
    }
}

impl CustomSpawner for VillageSiege {
    fn tick(&mut self, level: &mut ServerLevel, spawn_enemies: bool) -> i32 {
        if level.is_bright_outside() || !spawn_enemies {
            self.siege_state = State::SiegeDone;
            self.has_setup_siege = false;
            return 0;
        }
        if self.at_roll_time(level) {
            self.siege_state = if level.tick_state.random.next_int_bounded(10) == 0 {
                State::SiegeTonight
            } else {
                State::SiegeDone
            };
        }
        if self.siege_state == State::SiegeDone {
            return 0;
        }
        if !self.has_setup_siege {
            if !self.try_to_setup_siege(level) {
                return 0;
            }
            self.has_setup_siege = true;
        }
        if self.next_spawn_time > 0 {
            self.next_spawn_time -= 1;
        } else {
            self.next_spawn_time = 2;
            if self.zombies_to_spawn > 0 {
                self.try_spawn(level);
                self.zombies_to_spawn -= 1;
            } else {
                self.siege_state = State::SiegeDone;
            }
        }
        0
    }
}

/// `level.getBiome(center).is(BiomeTags.WITHOUT_ZOMBIE_SIEGES)`.
fn without_zombie_sieges(level: &ServerLevel, pos: BlockPos) -> bool {
    let tag = Identifier::parse("minecraft:without_zombie_sieges").expect("valid tag");
    level
        .get_biome_key(pos)
        .is_some_and(|biome| level.registries.is_in_tag(keys::BIOME, &biome, &tag))
}

/// `findRandomSpawnPos(level, pos)`.
fn find_random_spawn_pos(
    level: &ServerLevel,
    pos: BlockPos,
    random: &mut dyn RandomSource,
) -> Option<Vec3> {
    for _ in 0..10 {
        let x = pos.x() + random.next_int_bounded(16) - 8;
        let z = pos.z() + random.next_int_bounded(16) - 8;
        let y = level.get_height_at(HeightmapType::WorldSurface, x, z);
        let offset = BlockPos::new(x, y, z);
        if level.is_village(offset)
            && check_monster_spawn_rules(
                EntityType::ZOMBIE,
                level,
                EntitySpawnReason::Event,
                offset,
                random,
            )
        {
            return Some(Vec3::new(
                f64::from(x) + 0.5,
                f64::from(y),
                f64::from(z) + 0.5,
            ));
        }
    }
    None
}

/// The `roll_village_siege` marker and the default clock it must belong to (`ClockTimeMarkers.ROLL_VILLAGE_SIEGE` of `defaultClock`).
fn roll_marker(level: &ServerLevel) -> Option<(Identifier, ClockTimeMarker)> {
    let clock = level
        .spawning
        .dimension_type
        .as_ref()?
        .default_clock
        .clone()?;
    let id = Identifier::parse(ClockTimeMarkers::ROLL_VILLAGE_SIEGE).expect("valid id");
    level
        .registries
        .registry(keys::TIMELINE)?
        .entries()
        .iter()
        .filter_map(|entry| time_markers_from_timeline_json(&entry.json).ok())
        .flatten()
        .find(|(marker_id, marker)| *marker_id == id && marker.clock == clock)
        .map(|(_, marker)| (clock, marker))
}

#[distributed_slice(CUSTOM_SPAWNERS)]
static VILLAGE_SIEGE: CustomSpawnerRegistration = CustomSpawnerRegistration {
    order: order::VILLAGE_SIEGE,
    create: |_| Some(Box::new(VillageSiege::new())),
};
