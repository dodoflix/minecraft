//! The `VibrationSystem` of block entities: [`BlockEntityVibration`] is a `Copy` handle (position + kind) that looks
//! the block entity up in the level on every access, so the block entity stays in its chunk (visible to comparators and
//! neighbour updates) while its vibration logic changes its block. It is vanilla's `SculkSensorBlockEntity.VibrationUser`,
//! `CalibratedSculkSensorBlockEntity.VibrationUser` and `SculkShriekerBlockEntity.VibrationUser`.

use super::{SculkShriekerBlockEntity, SculkSensorBlockEntity};
use crate::server::level::ServerLevel;
use crate::world::level::block::BlockStateExt;
use crate::world::level::block::impls::sculk_sensor_block;
use crate::world::level::gameevent::vibrations::{self, VibrationData, VibrationSystem, get_game_event_frequency, get_redstone_strength_for_distance};
use crate::world::level::gameevent::{BlockEntityListener, DeliveryMode, GameEventContext, PositionSource, SourceEntityInfo};
use crate::world::level::BlockGetter;
use mc_core::phys::Vec3;
use mc_core::pos::BlockPos;
use mc_data::BlockEntityType;
use mc_data::GameEvent;
use mc_data::props;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    SculkSensor,
    CalibratedSculkSensor,
    SculkShrieker,
}

#[derive(Clone, Copy, Debug)]
pub struct BlockEntityVibration {
    pub pos: BlockPos,
    pub kind: Kind,
}

impl BlockEntityVibration {
    /// The handle of the vibration block entity at `pos`.
    pub fn at(level: &ServerLevel, pos: BlockPos) -> Option<Self> {
        let kind = match level.get_block_entity(pos)?.get_type() {
            BlockEntityType::SCULK_SENSOR => Kind::SculkSensor,
            BlockEntityType::CALIBRATED_SCULK_SENSOR => Kind::CalibratedSculkSensor,
            BlockEntityType::SCULK_SHRIEKER => Kind::SculkShrieker,
            _ => return None,
        };
        Some(Self { pos, kind })
    }

    /// `getListener()` as the chunk registers it (`EntityBlock.getListener`).
    pub fn listener(pos: BlockPos, kind: Kind) -> BlockEntityListener {
        let handle = Self { pos, kind };
        BlockEntityListener { source: handle.get_position_source(), radius: handle.get_listener_radius(), delivery: DeliveryMode::Unspecified, handle: handle_game_event }
    }

    /// `getBackSignal(level, pos, state)` of the calibrated sensor's user.
    fn get_back_signal(&self, level: &ServerLevel) -> i32 {
        let direction = level.get_block_state(self.pos).get(props::HORIZONTAL_FACING).get_opposite();
        crate::world::level::signal_getter::get_signal(level, self.pos.relative(direction), direction)
    }

    pub fn sensor<R>(level: &mut ServerLevel, pos: BlockPos, f: impl FnOnce(&mut SculkSensorBlockEntity) -> R) -> Option<R> {
        level.get_block_entity_immediate(pos)?.as_any_mut().downcast_mut::<SculkSensorBlockEntity>().map(f)
    }
}

/// `VibrationSystem.Listener.handleGameEvent` of the block entity at `pos`.
fn handle_game_event(level: &mut ServerLevel, pos: BlockPos, event: GameEvent, context: &GameEventContext, source_position: Vec3, _live: Option<&mut dyn crate::world::entity::Entity>) -> bool {
    match BlockEntityVibration::at(level, pos) {
        Some(mut system) => vibrations::handle_game_event(level, &mut system, event, context, source_position),
        None => false,
    }
}

/// The `getTicker` of the vibration block entities: `VibrationSystem.Ticker.tick`.
pub fn tick(level: &mut ServerLevel, pos: BlockPos) {
    if let Some(mut system) = BlockEntityVibration::at(level, pos) {
        vibrations::tick(level, &mut system);
    }
}

impl VibrationSystem for BlockEntityVibration {
    fn with_vibration_data<R>(&mut self, level: &mut ServerLevel, f: impl FnOnce(&mut VibrationData) -> R) -> R {
        match level.get_block_entity_immediate(self.pos).and_then(|block_entity| block_entity.vibration_data_mut()) {
            Some(data) => f(data),
            // the block entity is gone (its block was replaced meanwhile): nothing to keep
            None => f(&mut VibrationData::default()),
        }
    }

    fn get_listener_radius(&self) -> i32 {
        if self.kind == Kind::CalibratedSculkSensor { 16 } else { 8 }
    }

    fn get_position_source(&self) -> PositionSource {
        PositionSource::Block(self.pos)
    }

    fn can_trigger_avoid_vibration(&self) -> bool {
        self.kind != Kind::SculkShrieker
    }

    fn requires_adjacent_chunks_to_be_ticking(&self) -> bool {
        true
    }

    fn get_listenable_events(&self) -> &'static str {
        if self.kind == Kind::SculkShrieker { "shrieker_can_listen" } else { "vibrations" }
    }

    fn can_receive_vibration(&mut self, level: &mut ServerLevel, pos: BlockPos, event: GameEvent, context: &GameEventContext) -> bool {
        let state = level.get_block_state(self.pos);
        match self.kind {
            Kind::SculkShrieker => {
                !state.get(props::SHRIEKING) && vibrations::resolve_source_entity(level, context).is_some_and(|source| source.player.is_some())
            }
            Kind::SculkSensor | Kind::CalibratedSculkSensor => {
                if self.kind == Kind::CalibratedSculkSensor {
                    let comparison_type = self.get_back_signal(level);
                    if comparison_type != 0 && get_game_event_frequency(event) != comparison_type {
                        return false;
                    }
                }
                if pos == self.pos && (event == GameEvent::BLOCK_DESTROY || event == GameEvent::BLOCK_PLACE) {
                    return false;
                }
                get_game_event_frequency(event) != 0 && sculk_sensor_block::can_activate(state)
            }
        }
    }

    fn on_receive_vibration(
        &mut self,
        level: &mut ServerLevel,
        _pos: BlockPos,
        event: GameEvent,
        source_entity: Option<SourceEntityInfo>,
        projectile_owner: Option<SourceEntityInfo>,
        receiving_distance: f32,
    ) {
        match self.kind {
            Kind::SculkShrieker => {
                let player = projectile_owner.or(source_entity).and_then(|entity| entity.player);
                SculkShriekerBlockEntity::try_shriek(level, self.pos, player);
            }
            Kind::SculkSensor | Kind::CalibratedSculkSensor => {
                let state = level.get_block_state(self.pos);
                if sculk_sensor_block::can_activate(state) {
                    let event_frequency = get_game_event_frequency(event);
                    Self::sensor(level, self.pos, |sensor| sensor.last_vibration_frequency = event_frequency);
                    let calculated_power = get_redstone_strength_for_distance(receiving_distance, self.get_listener_radius());
                    if state.block() == mc_data::block::Block::CALIBRATED_SCULK_SENSOR || state.block() == mc_data::block::Block::SCULK_SENSOR {
                        sculk_sensor_block::activate(level, source_entity, self.pos, state, calculated_power, event_frequency);
                    }
                }
            }
        }
    }

    fn on_data_changed(&mut self, level: &mut ServerLevel) {
        let state = level.get_block_state(self.pos);
        super::set_changed(level, self.pos, state);
    }
}
