//! `net.minecraft.world.level.block.EyeblossomBlock` (extends `FlowerBlock`): opens and closes with the
//! `gameplay/eyeblossom_open` attribute.
use mc_game_core::register_block;
use mc_game_core::world::level::block::impls::prelude::*;
use mc_game_core::world::level::block::impls::vegetation_block::vegetation_behaviour_methods;
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::entity::effect::MobEffectInstance;
use mc_game_core::world::level::blockscan;
use mc_game_core::world::level::gameevent::GameEventContext;
use mc_core::phys::Vec3;
use mc_data::{GameEvent, MobEffect, SoundEvent};
use mc_protocol::components::SuspiciousStewEffects;
use mc_protocol::packets::play::ParticleOptions;
use mc_protocol::packets::play::level::SoundSource;

const EYEBLOSSOM_XZ_RANGE: i32 = 3;
const EYEBLOSSOM_Y_RANGE: i32 = 2;

/// `EyeblossomBlock.Type`.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) struct Type {
    open: bool,
}

impl Type {
    const OPEN: Type = Type { open: true };
    const CLOSED: Type = Type { open: false };

    pub(super) fn block(self) -> Block {
        if self.open { Block::OPEN_EYEBLOSSOM } else { Block::CLOSED_EYEBLOSSOM }
    }
    pub(super) fn transform(self) -> Type {
        Type { open: !self.open }
    }
    /// `effect` and `effectDuration` (seconds).
    fn effect(self) -> (MobEffect, f32) {
        if self.open { (MobEffect::BLINDNESS, 11.0) } else { (MobEffect::NAUSEA, 7.0) }
    }
    pub(super) fn long_switch_sound(self) -> SoundEvent {
        if self.open { SoundEvent::BLOCK_EYEBLOSSOM_OPEN_LONG } else { SoundEvent::BLOCK_EYEBLOSSOM_CLOSE_LONG }
    }
    fn short_switch_sound(self) -> SoundEvent {
        if self.open { SoundEvent::BLOCK_EYEBLOSSOM_OPEN } else { SoundEvent::BLOCK_EYEBLOSSOM_CLOSE }
    }
    fn particle_color(self) -> i32 {
        if self.open { 16545810 } else { 6250335 }
    }
    /// `spawnTransformParticle(level, pos, random)`.
    pub(super) fn spawn_transform_particle(self, level: &mut ServerLevel, pos: BlockPos) {
        let start = Vec3::at_center_of(&pos);
        let random = level.random();
        let lifetime = 0.5 + random.next_double();
        let velocity = Vec3::new(random.next_double() - 0.5, random.next_double() + 1.0, random.next_double() - 0.5);
        let target = start.add(velocity.scale(lifetime));
        let particle = ParticleOptions::Trail { target, color: self.particle_color(), duration: (20.0 * lifetime) as i32 };
        level.send_particles(particle, start.x, start.y, start.z, 1, 0.0, 0.0, 0.0, 0.0);
    }
}

/// `Type.fromBoolean(open).transform()`.
pub(super) fn transformed_type(open: bool) -> Type {
    Type { open }.transform()
}

pub struct EyeblossomBlock {
    ty: Type,
}

impl EyeblossomBlock {
    /// `tryChangingState(state, level, pos, random)`.
    fn try_changing_state(&self, state: BlockState, level: &mut ServerLevel, pos: BlockPos) -> bool {
        let should_be_open = level.attribute_tri_state(mc_game_core::world::attribute::attributes::EYEBLOSSOM_OPEN, pos).to_boolean(self.ty.open);
        if should_be_open == self.ty.open {
            return false;
        }
        let new_type = self.ty.transform();
        level.set_block_and_update(pos, new_type.block().default_state());
        level.game_event_at_block(GameEvent::BLOCK_CHANGE, pos, &GameEventContext::of_state(Some(state)));
        new_type.spawn_transform_particle(level, pos);
        let min_pos = pos.offset(-EYEBLOSSOM_XZ_RANGE, -EYEBLOSSOM_Y_RANGE, -EYEBLOSSOM_XZ_RANGE);
        let max_pos = pos.offset(EYEBLOSSOM_XZ_RANGE, EYEBLOSSOM_Y_RANGE, EYEBLOSSOM_XZ_RANGE);
        let mut nearby = Vec::new();
        blockscan::find_blocks(&*level, min_pos, max_pos, &|nearby_state| nearby_state == state, &mut |nearby_pos, _| {
            nearby.push(nearby_pos);
            false
        });
        for nearby_pos in nearby {
            let distance = pos.dist_sqr(&nearby_pos).sqrt();
            let delay = level.random().next_int_between_inclusive((distance * 5.0) as i32, (distance * 10.0) as i32);
            level.schedule_tick_block(nearby_pos, state.block(), delay);
        }
        true
    }
}

impl mc_game_core::world::level::block::impls::vegetation_block::MayPlaceOn for EyeblossomBlock {}

impl BlockBehaviour for EyeblossomBlock {
    vegetation_behaviour_methods!();
    fn random_tick(&self, state: BlockState, level: &mut ServerLevel, pos: BlockPos) {
        if self.try_changing_state(state, level, pos) {
            level.play_sound(None, pos, self.ty.transform().long_switch_sound(), SoundSource::Blocks, 1.0, 1.0);
        }
    }
    fn tick(&self, state: BlockState, level: &mut ServerLevel, pos: BlockPos) {
        if self.try_changing_state(state, level, pos) {
            level.play_sound(None, pos, self.ty.transform().short_switch_sound(), SoundSource::Blocks, 1.0, 1.0);
        }
    }
    fn entity_inside(
        &self,
        state: BlockState,
        level: &mut ServerLevel,
        _pos: BlockPos,
        entity: &mut dyn mc_game_core::world::entity::Entity,
        _effect_applier: &mut dyn mc_game_core::world::entity::inside_block_effects::InsideBlockEffectApplier,
        _is_precise: bool,
    ) {
        if level.level_data.get_difficulty() != mc_world::level_storage::Difficulty::Peaceful
            && entity.get_type() == mc_data::EntityType::BEE
            && super::flower_block::attracts_bees(state)
            && !entity.has_effect(MobEffect::POISON)
        {
            entity.add_effect(level, MobEffectInstance::new(MobEffect::POISON, 25, 0, false, true, true, None), None);
        }
    }
    fn get_suspicious_effects(&self) -> Option<SuspiciousStewEffects> {
        let (effect, seconds) = self.ty.effect();
        Some(super::flower_block::make_effect_list(effect, seconds))
    }
    /// `getBeeInteractionEffect`: `new MobEffectInstance(MobEffects.POISON, 25)`.
    fn get_bee_interaction_effect(&self) -> Option<MobEffectInstance> {
        Some(MobEffectInstance::new(MobEffect::POISON, 25, 0, false, true, true, None))
    }
}

register_block!("minecraft:open_eyeblossom", |_| Box::new(EyeblossomBlock { ty: Type::OPEN }));
register_block!("minecraft:closed_eyeblossom", |_| Box::new(EyeblossomBlock { ty: Type::CLOSED }));
