//! Small shared helpers of the `LivingEntity` port (entity-side reads that vanilla spells as one-liners).

use crate::server::level::ServerLevel;
use crate::world::entity::living::LivingEntityData;
use crate::world::entity::movement;
use crate::world::entity::{Entity, EntityId};
use mc_core::phys::Vec3;
use mc_core::pos::{BlockPos, ChunkPos};
use mc_data::{GameEvent, MobEffect};
use mc_protocol::packets::play::ClientboundEntityEventPacket;

/// The fluid tags `LivingEntity` distinguishes (`FluidTags.WATER` / `FluidTags.LAVA`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FluidKind {
    Water,
    Lava,
}

impl FluidKind {
    pub fn index(self) -> usize {
        match self {
            FluidKind::Water => 0,
            FluidKind::Lava => 1,
        }
    }
}

/// `asLivingEntity()` of an entity the caller knows is living.
pub fn l(entity: &dyn Entity) -> &LivingEntityData {
    entity.living().expect("living entity")
}

pub fn lm(entity: &mut dyn Entity) -> &mut LivingEntityData {
    entity.living_mut().expect("living entity")
}

/// `hasEffect(effect)`.
pub fn has_effect(entity: &dyn Entity, effect: MobEffect) -> bool {
    l(entity).active_effects.contains_key(&effect)
}

/// `getEffect(effect).getAmplifier()`.
pub fn effect_amplifier(entity: &dyn Entity, effect: MobEffect) -> Option<i32> {
    l(entity).active_effects.get(&effect).map(|e| e.amplifier)
}

/// `isInWater()`.
pub fn is_in_water(entity: &dyn Entity) -> bool {
    entity.base().was_touching_water
}

/// `isInLava()`.
pub fn is_in_lava(entity: &dyn Entity) -> bool {
    !entity.base().first_tick && entity.base().fluid_height[1] > 0.0
}

/// `isInLiquid()`.
pub fn is_in_liquid(entity: &dyn Entity) -> bool {
    is_in_water(entity) || is_in_lava(entity)
}

/// `isUnderWater()`.
pub fn is_under_water(entity: &dyn Entity) -> bool {
    entity.base().was_eye_in_water && is_in_water(entity)
}

/// `getFluidHeight(tag)`.
pub fn get_fluid_height(entity: &dyn Entity, fluid: FluidKind) -> f64 {
    entity.base().fluid_height[fluid.index()]
}

/// `isEyeInFluid(tag)`.
pub fn is_eye_in_fluid(entity: &dyn Entity, fluid: FluidKind) -> bool {
    entity.base().eye_in_fluid[fluid.index()]
}

/// `isSprinting()`.
pub fn is_sprinting(entity: &dyn Entity) -> bool {
    entity.base().get_shared_flag(crate::world::entity::entity::FLAG_SPRINTING)
}

/// `getLookAngle()`.
pub fn look_angle(entity: &dyn Entity) -> Vec3 {
    crate::world::item::calculate_view_vector(entity.base().x_rot(), entity.base().y_rot())
}

/// `isFree(xa, ya, za)`: the box moved by the offset has no collision.
pub fn is_free(level: &ServerLevel, entity: &dyn Entity, xa: f64, ya: f64, za: f64) -> bool {
    let area = entity.base().bounding_box().move_(xa, ya, za);
    crate::world::level::collision::CollisionGetter::no_collision(level, Some(entity.id()), Some(&movement::collision_context(entity)), &area)
}

/// `Level.isPositionEntityTicking(pos)`.
pub fn is_position_entity_ticking(level: &ServerLevel, pos: BlockPos) -> bool {
    level.entity_manager.can_position_tick(pos)
        && level.chunk_source.chunk_map.distance_manager.in_entity_ticking_range(ChunkPos::pack_block_pos(&pos))
}

/// `Level.broadcastEntityEvent(entity, event)`: to the tracking players and the entity itself.
pub fn broadcast_entity_event(level: &mut ServerLevel, id: EntityId, event_id: i8) {
    let packet = ClientboundEntityEventPacket { entity_id: id, event_id };
    level.entity_tracker.send_to_tracking_players_and_self(id, &crate::server::level::level_events::encode_packet(&packet));
}

/// `Entity.gameEvent(event)`.
pub fn game_event(level: &mut ServerLevel, entity: &dyn Entity, event: GameEvent) {
    level.game_event_from_entity_ref(entity, event, entity.base().position());
}

/// `move(MoverType.SELF-style, delta)`: runs `Entity.move`.
pub fn do_move(entity: &mut dyn Entity, level: &mut ServerLevel, mover: crate::world::entity::MoverType, delta: Vec3) {
    let _ = entity.r#move(level, mover, delta);
}

/// `Entity.moveRelative(speed, input)`.
pub fn move_relative(entity: &mut dyn Entity, speed: f32, input: Vec3) {
    let length = input.length_sqr();
    if length < 1.0E-7 {
        return;
    }
    let movement = (if length > 1.0 { input.normalize() } else { input }).scale(f64::from(speed));
    let y_rot = entity.base().y_rot() * (std::f64::consts::PI / 180.0) as f32;
    let sin = mc_core::math::mth::sin(f64::from(y_rot));
    let cos = mc_core::math::mth::cos(f64::from(y_rot));
    let delta = Vec3::new(
        movement.x * f64::from(cos) - movement.z * f64::from(sin),
        movement.y,
        movement.z * f64::from(cos) + movement.x * f64::from(sin),
    );
    let dm = entity.base().delta_movement();
    entity.base_mut().set_delta_movement(dm.add(delta));
}

/// `Entity.playSound(sound, volume, pitch)` of a living entity (players do not hear their own sounds).
pub fn play_sound(level: &mut ServerLevel, entity: &dyn Entity, sound: mc_data::SoundEvent, volume: f32, pitch: f32) {
    use mc_protocol::packets::play::SoundSource;
    let p = entity.base().position();
    let (except, source) = if entity.is_player() { (Some(entity.id()), SoundSource::Players) } else { (None, SoundSource::Neutral) };
    if !entity.base().is_silent() {
        level.play_sound_at(except, p.x, p.y, p.z, sound, source, volume, pitch);
    }
}

/// `getFallDamageSound(dmg)`.
pub fn fall_damage_sound(entity: &dyn Entity, dmg: i32) -> mc_data::SoundEvent {
    let (small, big) = entity.get_fall_sounds();
    if dmg > 4 { big } else { small }
}

/// `getEquipmentBreakEvent`: `entityEventForEquipmentBreak(slot)`.
pub fn equipment_break_event(slot: crate::world::entity::equipment::EquipmentSlot) -> i8 {
    use crate::world::entity::equipment::EquipmentSlot as S;
    match slot {
        S::MainHand => 47,
        S::OffHand => 48,
        S::Head => 49,
        S::Chest => 50,
        S::Legs => 51,
        S::Feet => 52,
        S::Body => 65,
        S::Saddle => 68,
    }
}

/// `LivingEntity.isInWall()` (= `!isSleeping() && Entity.isInWall()`).
pub fn is_in_wall(entity: &dyn Entity, level: &ServerLevel) -> bool {
    use crate::world::level::block::state::BlockStateExt;
    use mc_core::phys::AABB;
    use mc_core::phys::shapes::{BooleanOp, Shapes};
    if entity.is_sleeping() || entity.base().no_physics {
        return false;
    }
    let check_width = f64::from(entity.base().dimensions.width * 0.8);
    let eye_bb = AABB::of_size(entity.base().get_eye_position(), check_width, 1.0E-6, check_width);
    let eye_shape = Shapes::create(eye_bb);
    for x in mc_core::math::mth::floor_f64(eye_bb.min_x)..=mc_core::math::mth::floor_f64(eye_bb.max_x) {
        for y in mc_core::math::mth::floor_f64(eye_bb.min_y)..=mc_core::math::mth::floor_f64(eye_bb.max_y) {
            for z in mc_core::math::mth::floor_f64(eye_bb.min_z)..=mc_core::math::mth::floor_f64(eye_bb.max_z) {
                let pos = BlockPos::new(x, y, z);
                let state = crate::world::level::BlockGetter::get_block_state(level, pos);
                if !state.is_air()
                    && state.is_suffocating_at(level, pos)
                    && Shapes::join_is_not_empty(&state.get_collision_shape(level, pos).move_(f64::from(x), f64::from(y), f64::from(z)), &eye_shape, BooleanOp::AND)
                {
                    return true;
                }
            }
        }
    }
    false
}

/// `isInWaterOrRain()`.
pub fn is_in_water_or_rain(entity: &dyn Entity, level: &ServerLevel) -> bool {
    if is_in_water(entity) {
        return true;
    }
    let pos = entity.base().block_position();
    level.is_raining_at(pos) || level.is_raining_at(BlockPos::containing(f64::from(pos.x()), entity.base().bounding_box().max_y, f64::from(pos.z())))
}

/// `entity.hurtServer(level, level.damageSources().<kind>(), amount)`.
pub fn hurt(level: &mut ServerLevel, entity: &mut dyn Entity, kind: impl FnOnce(&crate::world::damagesource::DamageSources<'_>) -> crate::world::damagesource::DamageSource, amount: f32) {
    let source = kind(&level.damage_sources());
    entity.hurt_server(level, &source, amount);
}
