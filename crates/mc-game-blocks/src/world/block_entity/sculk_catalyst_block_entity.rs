//! `SculkCatalystBlockEntity`: saves the cursors of its level spreader (`cursors`), spreads them every tick and turns
//! the experience of dying mobs near it into new cursors (`CatalystListener`, delivery `BY_DISTANCE`, radius 8).

use mc_game_core::block_entity_base;
use mc_game_core::world::level::LevelAccessor as _;
use mc_game_core::world::block_entity::{BLOCK_ENTITY_TYPES, BlockEntity, BlockEntityData, BlockEntityRegistration};
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::entity::Entity;
use mc_game_core::world::entity::living_entity::death::get_experience_reward;
use mc_game_core::world::entity::player::ServerPlayer;
use mc_game_core::world::level::block::sculk_spreader::SculkSpreader;
use mc_game_core::world::level::gameevent::{BlockEntityListener, DeliveryMode, GameEventContext, PositionSource};
use linkme::distributed_slice;
use mc_core::direction::Direction;
use mc_core::phys::Vec3;
use mc_core::pos::BlockPos;
use mc_data::block::BlockState;
use mc_data::{BlockEntityType, GameEvent, SoundEvent};
use mc_data::props;
use mc_nbt::CompoundTag;
use mc_protocol::packets::play::ParticleOptions;
use mc_protocol::packets::play::level::SoundSource;

/// `CatalystListener.PULSE_TICKS`.
pub const PULSE_TICKS: i32 = 8;

pub struct SculkCatalystBlockEntity {
    base: BlockEntityData,
    /// `CatalystListener.sculkSpreader`.
    pub sculk_spreader: SculkSpreader,
    /// `CatalystListener.blockState`: the state the block entity was created with (never updated, `bloom` uses it).
    listener_block_state: BlockState,
}

impl SculkCatalystBlockEntity {
    pub fn new(pos: BlockPos, state: BlockState) -> Self {
        Self {
            base: BlockEntityData::new(BlockEntityType::SCULK_CATALYST, pos, state),
            sculk_spreader: SculkSpreader::create_level_spreader(),
            listener_block_state: state,
        }
    }

    fn with<R>(level: &mut ServerLevel, pos: BlockPos, f: impl FnOnce(&mut SculkCatalystBlockEntity) -> R) -> Option<R> {
        level.get_block_entity_immediate(pos)?.as_any_mut().downcast_mut::<SculkCatalystBlockEntity>().map(f)
    }

    /// `serverTick(level, pos, state, entity)`: `sculkSpreader.updateCursors(level, pos, level.getRandom(), true)`. The
    /// spreader is moved out of the block entity while it runs.
    ///
    /// The spreader draws from a clone of the level's shared random handle (vanilla passes `level.getRandom()` next to
    /// the level), so draws made by blocks the spreader changes interleave exactly.
    fn server_tick(level: &mut ServerLevel, pos: BlockPos) {
        let Some(mut spreader) = Self::with(level, pos, |catalyst| std::mem::replace(&mut catalyst.sculk_spreader, SculkSpreader::create_level_spreader())) else { return };
        let mut random = level.tick_state.random.clone();
        spreader.update_cursors(level, pos, &mut random, true);
        Self::with(level, pos, |catalyst| catalyst.sculk_spreader = spreader);
    }
}

/// `CatalystListener.handleGameEvent`.
fn handle_game_event(level: &mut ServerLevel, pos: BlockPos, event: GameEvent, _context: &GameEventContext, source_position: Vec3, live: Option<&mut dyn Entity>) -> bool {
    if event != GameEvent::ENTITY_DIE {
        return false;
    }
    // `context.sourceEntity() instanceof LivingEntity mob`
    let Some(mob) = live.filter(|mob| mob.living().is_some()) else { return false };
    // `!mob.wasExperienceConsumed()`
    if mob.living().is_some_and(|living| !living.skip_drop_experience) {
        let last_damage_source = mob.living().and_then(|living| living.last_damage_source.clone());
        let experience_would_drop = get_experience_reward(&mut *mob, level, last_damage_source.as_ref());
        if mob.should_drop_experience() && experience_would_drop > 0 {
            let start = BlockPos::containing_position(&source_position.relative(Direction::Up, 0.5));
            SculkCatalystBlockEntity::with(level, pos, |catalyst| catalyst.sculk_spreader.add_cursors(start, experience_would_drop));
            try_award_it_spreads_advancement(level, &*mob);
        }
        // `mob.skipDropExperience()`
        if let Some(living) = mob.living_mut() {
            living.skip_drop_experience = true;
        }
        if let Some(block_state) = SculkCatalystBlockEntity::with(level, pos, |catalyst| catalyst.listener_block_state) {
            bloom(level, pos, block_state);
        }
    }
    true
}

/// `tryAwardItSpreadsAdvancement(level, mob)`.
fn try_award_it_spreads_advancement(level: &ServerLevel, mob: &dyn Entity) {
    let player = mob.living().and_then(|living| living.last_hurt_by_mob).and_then(|uuid| level.get_entity_by_uuid(uuid)).filter(|e| e.as_any().is::<ServerPlayer>());
    if let Some(_player) = player {
        // SEAM(advancements): `CriteriaTriggers::KILL_MOB_NEAR_SCULK_CATALYST.trigger(level, player, mob, damage source)`
        // (the damage source is `mob.getLastDamageSource()`, else `damageSources().playerAttack(player)`) once the trigger API exists.
    }
}

/// `CatalystListener.bloom(level, pos, state, random)`.
fn bloom(level: &mut ServerLevel, pos: BlockPos, state: BlockState) {
    mc_game_core::world::level::LevelAccessor::set_block_and_update(level, pos, state.set(props::BLOOM, true));
    level.schedule_tick_block(pos, state.block(), PULSE_TICKS);
    level.send_particles(ParticleOptions::Simple(mc_data::ParticleType::SCULK_SOUL), f64::from(pos.x()) + 0.5, f64::from(pos.y()) + 1.15, f64::from(pos.z()) + 0.5, 2, 0.2, 0.0, 0.2, 0.0);
    let pitch = 0.6 + level.random().next_float() * 0.4;
    level.play_sound(None, pos, SoundEvent::BLOCK_SCULK_CATALYST_BLOOM, SoundSource::Blocks, 2.0, pitch);
}

impl BlockEntity for SculkCatalystBlockEntity {
    block_entity_base!();

    fn load_additional(&mut self, tag: &CompoundTag) {
        self.sculk_spreader.load(tag);
    }

    fn save_additional(&self, tag: &mut CompoundTag) {
        self.sculk_spreader.save(tag);
    }

    fn is_ticking(&self) -> bool {
        true
    }

    fn level_ticker(&self) -> Option<fn(&mut ServerLevel, BlockPos)> {
        Some(SculkCatalystBlockEntity::server_tick)
    }

    fn game_event_listener(&self) -> Option<BlockEntityListener> {
        Some(BlockEntityListener { source: PositionSource::Block(self.get_block_pos()), radius: 8, delivery: DeliveryMode::ByDistance, handle: handle_game_event })
    }
}

#[distributed_slice(BLOCK_ENTITY_TYPES)]
static SCULK_CATALYST: BlockEntityRegistration =
    BlockEntityRegistration { ty: BlockEntityType::SCULK_CATALYST, create: |pos, state| Box::new(SculkCatalystBlockEntity::new(pos, state)) };
