use crate::server::level::ServerLevel;
use crate::world::entity::{Entity, EntityId};
use mc_core::phys::Vec3;
use mc_core::pos::BlockPos;
use mc_data::GameEvent;
use mc_data::block::BlockState;
use uuid::Uuid;

/// What the vibration system reads from `GameEvent.Context.sourceEntity()` (vanilla keeps the entity object; here the
/// entity may be out of the level's slot while it ticks, so the facts are captured when the context is built, see
/// `vibrations::capture_source_entity`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SourceEntityInfo {
    /// `getId()`.
    pub id: EntityId,
    /// `getUUID()`.
    pub uuid: Uuid,
    /// `instanceof ServerPlayer`.
    pub is_server_player: bool,
    /// `isSpectator()`.
    pub spectator: bool,
    /// `isSteppingCarefully()`.
    pub stepping_carefully: bool,
    /// `dampensVibrations()`.
    pub dampens_vibrations: bool,
    /// `SculkShriekerBlockEntity.tryGetPlayer(entity)`: the server player behind the entity.
    pub player: Option<EntityId>,
    /// `VibrationInfo.getProjectileOwner(entity)`: the owner of a projectile.
    pub projectile_owner: Option<Uuid>,
}

/// `GameEvent.Context`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct GameEventContext {
    pub source_entity: Option<EntityId>,
    pub affected_state: Option<BlockState>,
    /// Captured facts of `source_entity` (see [`SourceEntityInfo`]); `None` = resolve through the level when needed.
    pub source_info: Option<SourceEntityInfo>,
}

impl GameEventContext {
    pub fn of_entity(source_entity: Option<EntityId>) -> Self {
        Self {
            source_entity,
            affected_state: None,
            source_info: None,
        }
    }
    pub fn of_state(state: Option<BlockState>) -> Self {
        Self {
            source_entity: None,
            affected_state: state,
            source_info: None,
        }
    }
    pub fn of(source_entity: Option<EntityId>, state: Option<BlockState>) -> Self {
        Self {
            source_entity,
            affected_state: state,
            source_info: None,
        }
    }
    /// `Context.of(entity, state)` with the entity's facts captured (the entity may be out of the level's slot).
    pub fn of_entity_ref(
        level: &ServerLevel,
        entity: &dyn Entity,
        state: Option<BlockState>,
    ) -> Self {
        Self {
            affected_state: state,
            ..Self::of_captured(super::vibrations::capture_source_entity(level, entity))
        }
    }
    /// `Context.of(entity)` with the entity's facts captured.
    pub fn of_captured(info: SourceEntityInfo) -> Self {
        Self {
            source_entity: Some(info.id),
            affected_state: None,
            source_info: Some(info),
        }
    }
}

/// `GameEventListener.DeliveryMode`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum DeliveryMode {
    #[default]
    Unspecified,
    ByDistance,
}

/// `PositionSource` (`BlockPositionSource` / `EntityPositionSource`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PositionSource {
    Block(BlockPos),
    Entity { uuid: Uuid, y_offset: f32 },
}

impl PositionSource {
    /// `getPosition(level)`; `entity_position` resolves `level.getEntity(uuid)` to its position.
    pub fn get_position(&self, entity_position: &dyn Fn(Uuid) -> Option<Vec3>) -> Option<Vec3> {
        match self {
            PositionSource::Block(pos) => Some(Vec3::at_center_of(pos)),
            PositionSource::Entity { uuid, y_offset } => {
                entity_position(*uuid).map(|p| p.add_xyz(0.0, f64::from(*y_offset), 0.0))
            }
        }
    }
}

/// Who owns a registered listener (resolved through the level's take-out storage when delivering).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ListenerOwner {
    BlockEntity(BlockPos),
    Entity(EntityId),
}

/// `GameEventListener`.
pub trait GameEventListener {
    fn get_listener_source(&self) -> PositionSource;
    fn get_listener_radius(&self) -> i32;
    /// `handleGameEvent(level, event, context, sourcePosition)`.
    fn handle_game_event(
        &mut self,
        level: &mut ServerLevel,
        event: GameEvent,
        context: &GameEventContext,
        source_position: Vec3,
    ) -> bool;
    fn get_delivery_mode(&self) -> DeliveryMode {
        DeliveryMode::Unspecified
    }
}

/// `handleGameEvent` of a block entity listener. Block entities stay in their chunk while they listen (their logic
/// changes the block, whose neighbours read the block entity again, e.g. comparators reading a sculk sensor), so the
/// handler is a function of the level and the block entity position instead of a method of the taken-out entity. The
/// last argument is the source entity itself when it is at hand (`ENTITY_DIE`).
pub type BlockEntityGameEventHandler = fn(
    &mut ServerLevel,
    BlockPos,
    GameEvent,
    &GameEventContext,
    Vec3,
    Option<&mut dyn crate::world::entity::Entity>,
) -> bool;

/// `EntityBlock.getListener(level, blockEntity)`: the `GameEventListener` of a block entity.
#[derive(Clone, Copy)]
pub struct BlockEntityListener {
    pub source: PositionSource,
    pub radius: i32,
    pub delivery: DeliveryMode,
    pub handle: BlockEntityGameEventHandler,
}
