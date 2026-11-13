//! `SculkShriekerBlockEntity`: `warning_level` and the vibration system's `listener` data; shrieking, warden warnings
//! (`WardenSpawnTracker`), the delayed response and the warden summon. The vibration user is
//! [`BlockEntityVibration`](super::vibration::BlockEntityVibration).

use super::vibration::{self, BlockEntityVibration, Kind};
use super::{
    BLOCK_ENTITY_TYPES, BlockEntity, BlockEntityData, BlockEntityRegistration, read_codec,
};
use crate::server::level::ServerLevel;
use crate::world::effect::mob_effect_util;
use crate::world::entity::effect::MobEffectInstance;
use crate::world::entity::spawn_util::{self, Strategy};
use crate::world::entity::{Entity, EntityId};
use crate::world::level::BlockGetter;
use crate::world::level::LevelAccessor as _;
use crate::world::level::flags::UPDATE_CLIENTS;
use crate::world::level::gameevent::vibrations::VibrationData;
use crate::world::level::gameevent::{BlockEntityListener, GameEventContext};
use linkme::distributed_slice;
use mc_core::math::mth;
use mc_core::phys::Vec3;
use mc_core::pos::BlockPos;
use mc_data::block::BlockState;
use mc_data::enums::EntitySpawnReason;
use mc_data::props;
use mc_data::{BlockEntityType, EntityType, GameEvent, MobEffect, SoundEvent};
use mc_nbt::CompoundTag;
use mc_protocol::packets::play::level::SoundSource;

const WARDEN_SPAWN_ATTEMPTS: i32 = 20;
const WARDEN_SPAWN_RANGE_XZ: i32 = 5;
const WARDEN_SPAWN_RANGE_Y: i32 = 6;
const DARKNESS_RADIUS: i32 = 40;
const SHRIEKING_TICKS: i32 = 90;

/// `SOUND_BY_LEVEL`.
fn sound_by_level(warning_level: i32) -> Option<SoundEvent> {
    match warning_level {
        1 => Some(SoundEvent::ENTITY_WARDEN_NEARBY_CLOSE),
        2 => Some(SoundEvent::ENTITY_WARDEN_NEARBY_CLOSER),
        3 => Some(SoundEvent::ENTITY_WARDEN_NEARBY_CLOSEST),
        4 => Some(SoundEvent::ENTITY_WARDEN_LISTENING_ANGRY),
        _ => None,
    }
}

/// `Warden.applyDarknessAround(level, position, source, darknessRadius)`.
pub fn apply_darkness_around(
    level: &mut ServerLevel,
    position: Vec3,
    source: Option<&dyn Entity>,
    darkness_radius: i32,
) {
    let darkness = MobEffectInstance::new(MobEffect::DARKNESS, 260, 0, false, false, false, None);
    mob_effect_util::add_effect_to_players_around(
        level,
        source,
        position,
        f64::from(darkness_radius),
        &darkness,
        200,
    );
}

pub struct SculkShriekerBlockEntity {
    base: BlockEntityData,
    pub warning_level: i32,
    pub vibration_data: VibrationData,
}

impl SculkShriekerBlockEntity {
    pub fn new(pos: BlockPos, state: BlockState) -> Self {
        Self {
            base: BlockEntityData::new(BlockEntityType::SCULK_SHRIEKER, pos, state),
            warning_level: 0,
            vibration_data: VibrationData::default(),
        }
    }

    fn with<R>(
        level: &mut ServerLevel,
        pos: BlockPos,
        f: impl FnOnce(&mut SculkShriekerBlockEntity) -> R,
    ) -> Option<R> {
        level
            .get_block_entity_immediate(pos)?
            .as_any_mut()
            .downcast_mut::<SculkShriekerBlockEntity>()
            .map(f)
    }

    /// `tryGetPlayer(sourceEntity)` for an entity given directly (`SculkShriekerBlock.stepOn`).
    pub fn try_get_player(level: &ServerLevel, entity: &dyn Entity) -> Option<EntityId> {
        crate::world::level::gameevent::vibrations::source_entity_info(level, entity).player
    }

    /// `tryShriek(level, player)` of the block entity at `pos`.
    pub fn try_shriek(level: &mut ServerLevel, pos: BlockPos, player: Option<EntityId>) {
        let Some(player) = player else { return };
        let state = level.get_block_state(pos);
        if !state.get(props::SHRIEKING) {
            Self::with(level, pos, |shrieker| shrieker.warning_level = 0);
            if !Self::can_respond(level, pos) || Self::try_to_warn(level, pos, player) {
                Self::shriek(level, pos, player);
            }
        }
    }

    /// `tryToWarn(level, player)`.
    fn try_to_warn(level: &mut ServerLevel, pos: BlockPos, player: EntityId) -> bool {
        use crate::world::entity::player::server_player::WardenSpawnTracker;
        match WardenSpawnTracker::try_warn(level, pos, player) {
            Some(warning_level) => {
                Self::with(level, pos, |shrieker| {
                    shrieker.warning_level = warning_level
                });
                true
            }
            None => false,
        }
    }

    /// `shriek(level, sourceEntity)`.
    fn shriek(level: &mut ServerLevel, pos: BlockPos, source_entity: EntityId) {
        let state = level.get_block_state(pos);
        crate::world::level::LevelAccessor::set_block_flags(
            level,
            pos,
            state.set(props::SHRIEKING, true),
            UPDATE_CLIENTS,
        );
        level.schedule_tick_block(pos, state.block(), SHRIEKING_TICKS);
        level.level_event_impl(None, 3007, pos, 0);
        let context = GameEventContext::of_entity(Some(source_entity));
        level.game_event_at_block(GameEvent::SHRIEK, pos, &context);
    }

    /// `canRespond(level)`.
    fn can_respond(level: &ServerLevel, pos: BlockPos) -> bool {
        Self::can_respond_for(level, level.get_block_state(pos))
    }

    /// `canRespond` for a given state of this block.
    fn can_respond_for(level: &ServerLevel, state: BlockState) -> bool {
        state.get(props::CAN_SUMMON)
            && level.difficulty() != mc_world::level_storage::Difficulty::Peaceful
            && level.spawn_wardens()
    }

    /// `tryRespond(level)`.
    pub fn try_respond(level: &mut ServerLevel, pos: BlockPos) {
        let warning_level = Self::with(level, pos, |shrieker| shrieker.warning_level).unwrap_or(0);
        let state = level.get_block_state(pos);
        Self::respond(level, pos, state, warning_level);
    }

    /// `tryRespond` with the state and warning level at hand (`canRespond` reads the block entity's cached state, which
    /// is the old state while the block entity is being removed).
    fn respond(level: &mut ServerLevel, pos: BlockPos, state: BlockState, warning_level: i32) {
        if Self::can_respond_for(level, state) && warning_level > 0 {
            if !Self::try_summon_warden(level, pos, warning_level) {
                Self::play_warden_reply_sound(level, pos, warning_level);
            }
            apply_darkness_around(level, Vec3::at_center_of(&pos), None, DARKNESS_RADIUS);
        }
    }

    /// `playWardenReplySound(level)`.
    fn play_warden_reply_sound(level: &mut ServerLevel, pos: BlockPos, warning_level: i32) {
        if let Some(sound) = sound_by_level(warning_level) {
            let random = level.random();
            let x = pos.x() + mth::random_between_inclusive(random, -10, 10);
            let y = pos.y() + mth::random_between_inclusive(random, -10, 10);
            let z = pos.z() + mth::random_between_inclusive(random, -10, 10);
            level.play_sound_at(
                None,
                f64::from(x),
                f64::from(y),
                f64::from(z),
                sound,
                SoundSource::Hostile,
                5.0,
                1.0,
            );
        }
    }

    /// `trySummonWarden(level)`.
    fn try_summon_warden(level: &mut ServerLevel, pos: BlockPos, warning_level: i32) -> bool {
        warning_level >= 4
            && spawn_util::try_spawn_mob(
                level,
                EntityType::WARDEN,
                EntitySpawnReason::Triggered,
                pos,
                WARDEN_SPAWN_ATTEMPTS,
                WARDEN_SPAWN_RANGE_XZ,
                WARDEN_SPAWN_RANGE_Y,
                Strategy::OnTopOfCollider,
                false,
            )
            .is_some()
    }
}

impl BlockEntity for SculkShriekerBlockEntity {
    block_entity_base!();

    fn load_additional(&mut self, tag: &CompoundTag) {
        self.warning_level = tag.get_int_or("warning_level", 0);
        self.vibration_data =
            read_codec(tag, "listener", VibrationData::codec()).unwrap_or_default();
    }

    fn save_additional(&self, tag: &mut CompoundTag) {
        tag.put_int("warning_level", self.warning_level);
        if let Ok(listener) =
            mc_dfu::encode_nbt(VibrationData::codec(), &self.vibration_data).into_result()
        {
            tag.put("listener", listener);
        }
    }

    fn is_ticking(&self) -> bool {
        true
    }

    fn level_ticker(&self) -> Option<fn(&mut ServerLevel, BlockPos)> {
        Some(vibration::tick)
    }

    fn vibration_data_mut(&mut self) -> Option<&mut VibrationData> {
        Some(&mut self.vibration_data)
    }

    fn game_event_listener(&self) -> Option<BlockEntityListener> {
        Some(BlockEntityVibration::listener(
            self.get_block_pos(),
            Kind::SculkShrieker,
        ))
    }

    /// `preRemoveSideEffects(pos, state)`: a shrieking shrieker answers before it is removed.
    fn pre_remove_side_effects(
        &mut self,
        level: &mut ServerLevel,
        pos: BlockPos,
        state: BlockState,
    ) {
        if state.get(props::SHRIEKING) {
            SculkShriekerBlockEntity::respond(level, pos, state, self.warning_level);
        }
    }
}

#[distributed_slice(BLOCK_ENTITY_TYPES)]
static SCULK_SHRIEKER: BlockEntityRegistration = BlockEntityRegistration {
    ty: BlockEntityType::SCULK_SHRIEKER,
    create: |pos, state| Box::new(SculkShriekerBlockEntity::new(pos, state)),
};
