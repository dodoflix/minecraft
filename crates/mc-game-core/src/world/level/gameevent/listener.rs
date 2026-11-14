use crate::server::level::ServerLevel;
use crate::world::entity::EntityId;
use mc_core::phys::Vec3;
use mc_core::pos::BlockPos;
use mc_data::GameEvent;
use mc_data::block::BlockState;
use uuid::Uuid;

/// What the vibration system reads from `GameEvent.Context.sourceEntity()`. Vanilla keeps the entity object, which a
/// delayed vibration still reads after the entity was removed, so the facts are read from the visible entity (stored
/// or lent, see `ServerLevel::expose`) when a vibration is scheduled and kept with it (`vibrations::source_entity_info`).
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
    /// Facts a delayed vibration carries from the entity it saw; `None` = read the entity through the level.
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
    /// The second listener of an entity (`Entity::handle_secondary_game_event`).
    EntitySecondary(EntityId),
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
/// handler is a function of the level and the block entity position instead of a method of the taken-out entity.
pub type BlockEntityGameEventHandler =
    fn(&mut ServerLevel, BlockPos, GameEvent, &GameEventContext, Vec3) -> bool;

/// `EntityBlock.getListener(level, blockEntity)`: the `GameEventListener` of a block entity.
#[derive(Clone, Copy)]
pub struct BlockEntityListener {
    pub source: PositionSource,
    pub radius: i32,
    pub delivery: DeliveryMode,
    pub handle: BlockEntityGameEventHandler,
}
