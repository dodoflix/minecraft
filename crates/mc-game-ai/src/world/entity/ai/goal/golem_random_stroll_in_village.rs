//! `GolemRandomStrollInVillageGoal` (a `RandomStrollGoal`).

use mc_core::phys::Vec3;
use mc_core::pos::{BlockPos, SectionPos};
use mc_core::random::RandomSource;
use mc_data::EntityType;
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::entity::Entity;
use mc_game_core::world::entity::ai::goal::{Goal, GoalFlags, RandomStrollGoal};
use mc_game_core::world::entity::ai::util::land_random_pos;
use mc_game_core::world::entity::ai::village::poi::Occupancy;
use mc_game_core::world::level::LevelAccessor;

pub const POI_SECTION_SCAN_RADIUS: i32 = 2;
pub const VILLAGER_SCAN_RADIUS: f64 = 32.0;
pub const RANDOM_POS_XY_DISTANCE: i32 = 10;
pub const RANDOM_POS_Y_DISTANCE: i32 = 7;

pub struct GolemRandomStrollInVillageGoal {
    pub base: RandomStrollGoal,
}

impl GolemRandomStrollInVillageGoal {
    /// `GolemRandomStrollInVillageGoal(mob, speedModifier)`: `super(mob, speedModifier, 240, false)`.
    pub fn new(speed_modifier: f64) -> Self {
        Self {
            base: RandomStrollGoal::with_check(speed_modifier, 240, false),
        }
    }

    /// `getPosition()`.
    pub fn get_position(mob: &mut dyn Entity, level: &mut ServerLevel) -> Option<Vec3> {
        if level.random().next_float() < 0.3 {
            return Self::get_position_towards_anywhere(mob, level);
        }
        let target = if level.random().next_float() < 0.7 {
            let t = Self::get_position_towards_villager_who_wants_golem(mob, level);
            if t.is_none() {
                Self::get_position_towards_poi(mob, level)
            } else {
                t
            }
        } else {
            let t = Self::get_position_towards_poi(mob, level);
            if t.is_none() {
                Self::get_position_towards_villager_who_wants_golem(mob, level)
            } else {
                t
            }
        };
        match target {
            None => Self::get_position_towards_anywhere(mob, level),
            some => some,
        }
    }

    fn get_position_towards_anywhere(
        mob: &mut dyn Entity,
        level: &mut ServerLevel,
    ) -> Option<Vec3> {
        land_random_pos::get_pos(mob, level, RANDOM_POS_XY_DISTANCE, RANDOM_POS_Y_DISTANCE)
    }

    fn get_position_towards_villager_who_wants_golem(
        mob: &mut dyn Entity,
        level: &mut ServerLevel,
    ) -> Option<Vec3> {
        let bb = mob.base().bounding_box().inflate(
            VILLAGER_SCAN_RADIUS,
            VILLAGER_SCAN_RADIUS,
            VILLAGER_SCAN_RADIUS,
        );
        let game_time = level.game_time();
        let villagers = level.get_entities_of_class(
            &bb,
            |e| e.get_type() == EntityType::VILLAGER,
            |e| e.villager_wants_to_spawn_golem(game_time),
        );
        if villagers.is_empty() {
            None
        } else {
            let villager =
                villagers[level.random().next_int_bounded(villagers.len() as i32) as usize];
            let target_pos = level.get_entity(villager)?.position();
            land_random_pos::get_pos_towards(
                mob,
                level,
                RANDOM_POS_XY_DISTANCE,
                RANDOM_POS_Y_DISTANCE,
                target_pos,
            )
        }
    }

    fn get_position_towards_poi(mob: &mut dyn Entity, level: &mut ServerLevel) -> Option<Vec3> {
        let target_section = Self::get_random_village_section(mob, level)?;
        let target_pos = Self::get_random_poi_within_section(level, target_section)?;
        land_random_pos::get_pos_towards(
            mob,
            level,
            RANDOM_POS_XY_DISTANCE,
            RANDOM_POS_Y_DISTANCE,
            Vec3::at_bottom_center_of(&target_pos),
        )
    }

    fn get_random_village_section(mob: &dyn Entity, level: &mut ServerLevel) -> Option<SectionPos> {
        let center = SectionPos::of_block_pos(&mob.block_position());
        let village_sections: Vec<SectionPos> = SectionPos::cube(&center, POI_SECTION_SCAN_RADIUS)
            .filter(|s| level.sections_to_village(*s) == 0)
            .collect();
        if village_sections.is_empty() {
            None
        } else {
            Some(
                village_sections[level
                    .random()
                    .next_int_bounded(village_sections.len() as i32)
                    as usize],
            )
        }
    }

    fn get_random_poi_within_section(
        level: &mut ServerLevel,
        section_pos: SectionPos,
    ) -> Option<BlockPos> {
        let pois: Vec<BlockPos> = level
            .poi_manager()
            .get_in_range(&|_| true, section_pos.center(), 8, Occupancy::IsOccupied)
            .iter()
            .map(|r| r.get_pos())
            .collect();
        if pois.is_empty() {
            None
        } else {
            Some(pois[level.random().next_int_bounded(pois.len() as i32) as usize])
        }
    }
}

impl Goal for GolemRandomStrollInVillageGoal {
    fn can_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        self.base.can_use_with(mob, level, Self::get_position)
    }

    fn can_continue_to_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        self.base.can_continue_to_use_impl(mob, level)
    }

    fn start(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        self.base.start_impl(mob, level);
    }

    fn stop(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        self.base.stop_impl(mob, level);
    }

    fn flags(&self) -> GoalFlags {
        self.base.flags()
    }
}
