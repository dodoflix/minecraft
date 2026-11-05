//! `net.minecraft.world.level.block.PowderSnowBlock` (the entity-facing half: `entityInside`, `fallOn`).
use super::prelude::*;
use crate::server::level::ServerLevel;
use crate::world::entity::Entity;
use crate::world::entity::inside_block_effects::{InsideBlockEffectApplier, InsideBlockEffectType};
use mc_core::phys::Vec3;
use mc_core::random::RandomSource;
use mc_protocol::packets::play::{ParticleOptions, RandomizationType};

pub struct PowderSnowBlock;

/// `Mth.randomBetween(random, min, max)`.
fn random_between(random: &mut dyn RandomSource, min: f32, max: f32) -> f32 {
    random.next_float() * (max - min) + min
}

impl BlockBehaviour for PowderSnowBlock {
    /// `getCollisionShape`: a falling entity (fall distance > 2.5) lands on a 0.9 high slab of the block, entities that can
    /// walk on powder snow (leather boots, ...) and falling blocks stand on the full block, everything else sinks in.
    fn get_collision_shape(&self, _state: BlockState, _level: &dyn BlockGetter, pos: BlockPos, context: &crate::world::level::collision::CollisionContext) -> mc_core::phys::shapes::VoxelShape {
        use mc_core::phys::shapes::Shapes;
        if !context.is_placement() && context.entity().is_some() {
            if context.entity_fall_distance() > 2.5 {
                return Shapes::box_(0.0, 0.0, 0.0, 1.0, f64::from(0.9f32), 1.0);
            }
            if context.entity_is_falling_block() || (context.entity_can_walk_on_powder_snow() && context.is_above(&Shapes::block(), pos, false) && !context.is_descending()) {
                return Shapes::block();
            }
        }
        Shapes::empty()
    }
    fn get_entity_inside_collision_shape(&self, state: BlockState, level: &dyn BlockGetter, pos: BlockPos, entity: &dyn Entity) -> mc_core::phys::shapes::VoxelShape {
        let shape = self.get_collision_shape(state, level, pos, &crate::world::entity::movement::collision_context(entity));
        if shape.is_empty() { mc_core::phys::shapes::Shapes::block() } else { shape }
    }
    fn entity_inside(&self, state: BlockState, level: &mut ServerLevel, pos: BlockPos, entity: &mut dyn Entity, effect_applier: &mut dyn InsideBlockEffectApplier, _is_precise: bool) {
        if entity.living().is_none() || entity.get_in_block_state(level).is_block(state.block()) {
            entity.make_stuck_in_block(state, Vec3::new(f64::from(0.9f32), 1.5, f64::from(0.9f32)));
            let known_movement = entity.get_known_movement(level);
            let is_moving = known_movement.x != 0.0 || known_movement.z != 0.0;
            let random = LevelAccessor::random(level);
            if is_moving && random.next_boolean() {
                let x_speed = random_between(random, -1.0, 1.0) * 0.083333336;
                let z_speed = random_between(random, -1.0, 1.0) * 0.083333336;
                let p = entity.base().position();
                level.send_particles_full(
                    ParticleOptions::Simple(mc_data::ParticleType::SNOWFLAKE),
                    false,
                    false,
                    p.x,
                    f64::from(pos.y() + 1),
                    p.z,
                    1,
                    0.0,
                    0.0,
                    0.0,
                    f64::from(x_speed),
                    f64::from(0.05f32),
                    f64::from(z_speed),
                    RandomizationType::Alternative,
                );
            }
        }
        effect_applier.run_before(
            InsideBlockEffectType::Extinguish,
            Box::new(move |e, level| {
                if e.is_on_fire() && (level.mob_griefing() || e.is_player()) && e.may_interact(level, pos) {
                    level.destroy_block(pos, false, None, 512);
                }
            }),
        );
        effect_applier.apply(InsideBlockEffectType::Freeze);
        effect_applier.apply(InsideBlockEffectType::Extinguish);
    }

    fn is_bucket_pickup(&self) -> bool {
        true
    }
    /// `BucketPickup.pickupBlock`.
    fn pickup_block(&self, _user: Option<&dyn Entity>, level: &mut dyn LevelAccessor, pos: BlockPos, state: BlockState) -> crate::world::item::ItemStack {
        level.set_block_flags(pos, air(), 11);
        level.level_event(None, 2001, pos, state.id() as i32);
        crate::world::item::ItemStack::new(mc_data::Item::POWDER_SNOW_BUCKET)
    }
    fn get_pickup_sound(&self) -> Option<mc_data::SoundEvent> {
        Some(mc_data::SoundEvent::ITEM_BUCKET_FILL_POWDER_SNOW)
    }

    fn fall_on(&self, level: &mut ServerLevel, _state: BlockState, _pos: BlockPos, entity: &mut dyn Entity, fall_distance: f64) {
        if !(fall_distance < 4.0) && entity.living().is_some() {
            // `LivingEntity.getFallSounds()`: players have their own, mobs override per type (generic default here)
            let (small, big) = if entity.is_player() {
                (mc_data::SoundEvent::ENTITY_PLAYER_SMALL_FALL, mc_data::SoundEvent::ENTITY_PLAYER_BIG_FALL)
            } else {
                (mc_data::SoundEvent::ENTITY_GENERIC_SMALL_FALL, mc_data::SoundEvent::ENTITY_GENERIC_BIG_FALL)
            };
            entity.play_sound(level, if fall_distance < 7.0 { small } else { big }, 1.0, 1.0);
        }
    }
}

register_block_class!("PowderSnowBlock", |_| Box::new(PowderSnowBlock));
