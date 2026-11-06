//! `CelebrateVillagersSurvivedRaid`: villagers launch fireworks after a won raid.

use super::move_to_sky_seeing_spot::has_no_blocks_above;
use super::village_util::update_activity_from_schedule;
use super::{Behavior, BehaviorBase};
use crate::server::level::ServerLevel;
use crate::world::entity::Entity;
use crate::world::entity::projectile::firework_rocket::FireworkRocketEntity;
use crate::world::entity::projectile::spawn::spawn_projectile_plain;
use crate::world::item::ItemStack;
use mc_core::random::RandomSource;
use mc_data::{Item, SoundEvent};
use mc_protocol::components::values::decor::{
    FIREWORKS, FireworkExplosion, FireworkShape, Fireworks,
};

pub struct CelebrateVillagersSurvivedRaid {
    /// `currentRaid`'s id (the raid is reached through the level).
    current_raid: Option<i32>,
}

impl CelebrateVillagersSurvivedRaid {
    pub fn new(
        min_duration: i32,
        max_duration: i32,
    ) -> BehaviorBase<CelebrateVillagersSurvivedRaid> {
        BehaviorBase::with_duration(
            Vec::new(),
            min_duration,
            max_duration,
            CelebrateVillagersSurvivedRaid { current_raid: None },
        )
    }

    fn get_firework(color: usize, flight_duration: i32) -> ItemStack {
        let mut rocket = ItemStack::new(Item::FIREWORK_ROCKET);
        rocket.set(
            FIREWORKS,
            Fireworks {
                flight_duration,
                explosions: vec![FireworkExplosion {
                    shape: FireworkShape::Burst,
                    colors: vec![mc_data::misc::DYE_COLORS[color].firework_color],
                    fade_colors: Vec::new(),
                    has_trail: false,
                    has_twinkle: false,
                }],
            },
        );
        rocket
    }
}

impl Behavior for CelebrateVillagersSurvivedRaid {
    fn check_extra_start_conditions(
        &mut self,
        level: &mut ServerLevel,
        body: &mut dyn Entity,
    ) -> bool {
        let test_pos = body.block_position();
        self.current_raid = level.get_raid_at(test_pos).map(|r| r.id());
        self.current_raid
            .is_some_and(|id| level.get_raid(id).is_some_and(|r| r.is_victory()))
            && has_no_blocks_above(level, body, test_pos)
    }

    fn can_still_use(
        &mut self,
        level: &mut ServerLevel,
        _body: &mut dyn Entity,
        _timestamp: i64,
    ) -> bool {
        self.current_raid
            .is_some_and(|id| level.get_raid(id).is_some_and(|r| !r.is_stopped()))
    }

    fn stop(&mut self, level: &mut ServerLevel, body: &mut dyn Entity, _timestamp: i64) {
        self.current_raid = None;
        update_activity_from_schedule(level, body);
    }

    fn tick(&mut self, level: &mut ServerLevel, body: &mut dyn Entity, _timestamp: i64) {
        if body.base_mut().random.next_int_bounded(100) == 0 {
            // `playCelebrateSound()`
            crate::world::entity::living_entity::damage::make_sound(
                body,
                level,
                SoundEvent::ENTITY_VILLAGER_CELEBRATE,
            );
        }
        if body.base_mut().random.next_int_bounded(200) == 0
            && has_no_blocks_above(level, body, body.block_position())
        {
            let color = body
                .base_mut()
                .random
                .next_int_bounded(mc_data::misc::DYE_COLORS.len() as i32)
                as usize;
            let flight_duration = body.base_mut().random.next_int_bounded(3);
            let firework = Self::get_firework(color, flight_duration);
            let (x, eye_y, z) = (body.base().x(), body.base().get_eye_y(), body.base().z());
            let rocket =
                FireworkRocketEntity::with_owner_at(level, Some(&*body), x, eye_y, z, &firework);
            spawn_projectile_plain(level, Box::new(rocket), &firework);
        }
    }
}
