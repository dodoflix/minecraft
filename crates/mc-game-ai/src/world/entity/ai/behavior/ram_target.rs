//! `RamTarget` (a `Goat` behavior; the horn dropping of `Goat.dropHorn()` is the `drop_horn` callback).

use mc_core::math::mth;
use mc_core::phys::Vec3;
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_data::{Attribute, MobEffect, SoundEvent};
use mc_game_core::entry_condition;
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::entity::ai::behavior::behavior_utils;
use mc_game_core::world::entity::ai::behavior::{Behavior, BehaviorBase};
use mc_game_core::world::entity::ai::memory::WalkTarget;
use mc_game_core::world::entity::ai::memory::memory_module_type::{
    RAM_COOLDOWN_TICKS, RAM_TARGET, WALK_TARGET,
};
use mc_game_core::world::entity::ai::targeting::TargetingConditions;
use mc_game_core::world::entity::entity::Entity;
use mc_game_core::world::entity::mob::ext;
use mc_game_core::world::item::enchantment::actors::Actors;
use mc_game_core::world::item::enchantment_helper;
use mc_game_core::world::level::BlockGetter;
use mc_game_core::world::level::block::state::BlockStateExt;
use mc_protocol::packets::play::SoundSource;

/// `TIME_OUT_DURATION`.
pub const TIME_OUT_DURATION: i32 = 200;
/// `RAM_SPEED_FORCE_FACTOR`.
pub const RAM_SPEED_FORCE_FACTOR: f32 = 1.65;

type TimeBetweenRamsGetter = Box<dyn Fn(&dyn Entity) -> (i32, i32)>;
type KnockbackForceGetter = Box<dyn Fn(&dyn Entity) -> f64>;
type SoundGetter = Box<dyn Fn(&dyn Entity) -> SoundEvent>;
type DropHorn = Box<dyn FnMut(&mut ServerLevel, &mut dyn Entity) -> bool>;

pub struct RamTarget {
    get_time_between_rams: TimeBetweenRamsGetter,
    ram_targeting: TargetingConditions,
    speed: f32,
    get_knockback_force: KnockbackForceGetter,
    ram_direction: Vec3,
    get_impact_sound: SoundGetter,
    get_horn_break_sound: SoundGetter,
    drop_horn: DropHorn,
}

impl RamTarget {
    /// `new RamTarget(getTimeBetweenRams, ramTargeting, speed, getKnockbackForce, getImpactSound, getHornBreakSound)`; the
    /// `UniformInt` of `getTimeBetweenRams` is `(min, max)` inclusive; `drop_horn` = `Goat.dropHorn()`.
    pub fn new(
        get_time_between_rams: impl Fn(&dyn Entity) -> (i32, i32) + 'static,
        ram_targeting: TargetingConditions,
        speed: f32,
        get_knockback_force: impl Fn(&dyn Entity) -> f64 + 'static,
        get_impact_sound: impl Fn(&dyn Entity) -> SoundEvent + 'static,
        get_horn_break_sound: impl Fn(&dyn Entity) -> SoundEvent + 'static,
        drop_horn: impl FnMut(&mut ServerLevel, &mut dyn Entity) -> bool + 'static,
    ) -> BehaviorBase<RamTarget> {
        BehaviorBase::with_timeout(
            entry_condition![RAM_COOLDOWN_TICKS => ValueAbsent, RAM_TARGET => ValuePresent],
            TIME_OUT_DURATION,
            RamTarget {
                get_time_between_rams: Box::new(get_time_between_rams),
                ram_targeting,
                speed,
                get_knockback_force: Box::new(get_knockback_force),
                ram_direction: Vec3::ZERO,
                get_impact_sound: Box::new(get_impact_sound),
                get_horn_break_sound: Box::new(get_horn_break_sound),
                drop_horn: Box::new(drop_horn),
            },
        )
    }

    fn has_rammed_horn_breaking_block(level: &ServerLevel, body: &dyn Entity) -> bool {
        let horizontal_movement_normalized = body
            .base()
            .delta_movement()
            .multiply_xyz(1.0, 0.0, 1.0)
            .normalize();
        let p = body.position().add(horizontal_movement_normalized);
        let facing_block_position = BlockPos::containing(p.x, p.y, p.z);
        level
            .get_block_state(facing_block_position)
            .is_tag("snaps_goat_horn")
            || level
                .get_block_state(facing_block_position.above())
                .is_tag("snaps_goat_horn")
    }

    fn finish_ram(&self, level: &mut ServerLevel, body: &mut dyn Entity) {
        level.broadcast_entity_event(body.id(), 59);
        let (min, max) = (self.get_time_between_rams)(&*body);
        let cooldown = level.tick_state.random.next_int_between_inclusive(min, max);
        body.brain_mut().set_memory(&RAM_COOLDOWN_TICKS, cooldown);
        body.brain_mut().erase_memory(&RAM_TARGET);
    }

    fn amplifier_level(body: &dyn Entity, effect: MobEffect) -> i32 {
        body.get_effect(effect).map_or(0, |e| e.amplifier + 1)
    }
}

impl Behavior for RamTarget {
    fn check_extra_start_conditions(
        &mut self,
        _level: &mut ServerLevel,
        body: &mut dyn Entity,
    ) -> bool {
        body.brain().has_memory_value(RAM_TARGET)
    }

    fn can_still_use(
        &mut self,
        _level: &mut ServerLevel,
        body: &mut dyn Entity,
        _timestamp: i64,
    ) -> bool {
        body.brain().has_memory_value(RAM_TARGET)
    }

    fn start(&mut self, _level: &mut ServerLevel, body: &mut dyn Entity, _timestamp: i64) {
        let cur_pos = body.block_position();
        let ram_target_pos = *body
            .brain()
            .get_memory(&RAM_TARGET)
            .expect("ram target present");
        self.ram_direction = Vec3::new(
            f64::from(cur_pos.x()) - ram_target_pos.x,
            0.0,
            f64::from(cur_pos.z()) - ram_target_pos.z,
        )
        .normalize();
        body.brain_mut().set_memory(
            &WALK_TARGET,
            WalkTarget::from_vec3(ram_target_pos, self.speed, 0),
        );
    }

    fn tick(&mut self, level: &mut ServerLevel, body: &mut dyn Entity, _timestamp: i64) {
        let bb = body.get_bounding_box();
        let nearby_entities =
            ext::get_nearby_entities(level, |_| true, &self.ram_targeting, &*body, &bb);
        if let Some(&ram_target) = nearby_entities.first() {
            let damage_source = level.damage_sources().no_aggro_mob_attack(&*body);
            let damage = body
                .living()
                .expect("living")
                .get_attribute_value(Attribute::ATTACK_DAMAGE) as f32;
            let movement_speed_level = Self::amplifier_level(&*body, MobEffect::SPEED);
            let movement_slowdown_level = Self::amplifier_level(&*body, MobEffect::SLOWNESS);
            let speed_boost_power = 0.25 * (movement_speed_level - movement_slowdown_level) as f32;
            let speed_factor = mth::clamp_f32(body.get_speed() * RAM_SPEED_FORCE_FACTOR, 0.2, 3.0)
                + speed_boost_power;
            let knockback_force = (self.get_knockback_force)(&*body);
            let ram_direction = self.ram_direction;
            let source = level.damage_sources().mob_attack(&*body);
            level.with_entity(ram_target, |level, target| {
                if target.hurt_server(level, &damage_source, damage) {
                    enchantment_helper::do_post_attack_effects(
                        level,
                        &mut Actors::of(body).and(target),
                        ram_target,
                        &damage_source,
                    );
                }
                let blocked_damage =
                    mc_game_core::world::entity::living_entity::blocking::apply_item_blocking(
                        target, level, &source, damage,
                    );
                let blocking_factor = if blocked_damage > 0.0 {
                    0.5_f32
                } else {
                    1.0_f32
                };
                target.knockback(
                    f64::from(blocking_factor * speed_factor) * knockback_force,
                    ram_direction.x,
                    ram_direction.z,
                    &source,
                    damage,
                    false,
                );
            });
            self.finish_ram(level, body);
            behavior_utils::play_sound_from_entity(
                level,
                body.id(),
                (self.get_impact_sound)(&*body),
                SoundSource::Neutral,
                1.0,
                1.0,
            );
        } else if Self::has_rammed_horn_breaking_block(level, &*body) {
            behavior_utils::play_sound_from_entity(
                level,
                body.id(),
                (self.get_impact_sound)(&*body),
                SoundSource::Neutral,
                1.0,
                1.0,
            );
            let dropped = (self.drop_horn)(level, body);
            if dropped {
                behavior_utils::play_sound_from_entity(
                    level,
                    body.id(),
                    (self.get_horn_break_sound)(&*body),
                    SoundSource::Neutral,
                    1.0,
                    1.0,
                );
            }
            self.finish_ram(level, body);
        } else {
            let walk_target = body.brain().get_memory(&WALK_TARGET).cloned();
            let ram_target = body.brain().get_memory(&RAM_TARGET).copied();
            let lost_or_reached_target = match (&walk_target, ram_target) {
                (Some(w), Some(r)) => w.get_target().current_position(level).closer_than(r, 0.25),
                _ => true,
            };
            if lost_or_reached_target {
                self.finish_ram(level, body);
            }
        }
    }
}
