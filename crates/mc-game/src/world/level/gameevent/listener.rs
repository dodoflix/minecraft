use crate::server::level::ServerLevel;
use crate::world::entity::EntityId;
use mc_core::phys::Vec3;
use mc_core::pos::BlockPos;
use mc_data::GameEvent;
use mc_data::block::BlockState;
use uuid::Uuid;

/// `GameEvent.Context`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct GameEventContext {
    pub source_entity: Option<EntityId>,
    pub affected_state: Option<BlockState>,
}

impl GameEventContext {
    pub fn of_entity(source_entity: Option<EntityId>) -> Self {
        Self { source_entity, affected_state: None }
    }
    pub fn of_state(state: Option<BlockState>) -> Self {
        Self { source_entity: None, affected_state: state }
    }
    pub fn of(source_entity: Option<EntityId>, state: Option<BlockState>) -> Self {
        Self { source_entity, affected_state: state }
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
            PositionSource::Entity { uuid, y_offset } => entity_position(*uuid).map(|p| p.add_xyz(0.0, f64::from(*y_offset), 0.0)),
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
    fn handle_game_event(&mut self, level: &mut ServerLevel, event: GameEvent, context: &GameEventContext, source_position: Vec3) -> bool;
    fn get_delivery_mode(&self) -> DeliveryMode {
        DeliveryMode::Unspecified
    }
}
