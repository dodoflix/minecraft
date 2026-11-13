//! `net.minecraft.world.level.block.SculkSensorBlock` (also the base of `CalibratedSculkSensorBlock`, which shares the
//! free functions below with a different number of active ticks).
use super::prelude::*;
use crate::server::level::ServerLevel;
use crate::world::block_entity::SculkSensorBlockEntity;
use crate::world::block_entity::vibration::BlockEntityVibration;
use crate::world::entity::Entity;
use crate::world::item::BlockPlaceContext;
use crate::world::level::flags::{UPDATE_CLIENTS, UPDATE_KNOWN_SHAPE};
use crate::world::level::gameevent::vibrations::{
    self, VibrationSystem, get_resonance_event_by_frequency,
};
use crate::world::level::gameevent::{GameEventContext, SourceEntityInfo};
use mc_data::enums::SculkSensorPhase;
use mc_data::fluid::Fluid;
use mc_data::{EntityType, GameEvent, SoundEvent};
use mc_protocol::packets::play::level::SoundSource;
use std::sync::LazyLock;

pub const ACTIVE_TICKS: i32 = 30;
pub const COOLDOWN_TICKS: i32 = 10;

/// `RESONANCE_PITCH_BEND`.
static RESONANCE_PITCH_BEND: LazyLock<[f32; 16]> = LazyLock::new(|| {
    let tone_map = [0, 0, 2, 4, 6, 7, 9, 10, 12, 14, 15, 18, 19, 21, 22, 24];
    // `NoteBlock.getPitchFromNote(note)`
    tone_map.map(|note: i32| 2.0f64.powf(f64::from(note - 12) / 12.0) as f32)
});

/// `getPhase(state)`.
pub fn get_phase(state: BlockState) -> SculkSensorPhase {
    state.get(props::SCULK_SENSOR_PHASE)
}

/// `canActivate(state)`.
pub fn can_activate(state: BlockState) -> bool {
    get_phase(state) == SculkSensorPhase::Inactive
}

/// `getActiveTicks()`.
fn active_ticks(state: BlockState) -> i32 {
    if state.block() == Block::CALIBRATED_SCULK_SENSOR {
        10
    } else {
        ACTIVE_TICKS
    }
}

/// `updateNeighbours(level, pos, state)`.
fn update_neighbours(level: &mut ServerLevel, pos: BlockPos, state: BlockState) {
    let block = state.block();
    level.update_neighbors_at(pos, block);
    level.update_neighbors_at(pos.below(), block);
}

/// `deactivate(level, pos, state)`.
pub fn deactivate(level: &mut ServerLevel, pos: BlockPos, state: BlockState) {
    level.set_block_and_update(
        pos,
        state
            .set(props::SCULK_SENSOR_PHASE, SculkSensorPhase::Cooldown)
            .set(props::POWER, 0),
    );
    level.schedule_tick_block(pos, state.block(), COOLDOWN_TICKS);
    update_neighbours(level, pos, state);
}

/// The `Context.of(sourceEntity)` of an entity captured in a vibration.
fn entity_context(source: Option<SourceEntityInfo>, state: Option<BlockState>) -> GameEventContext {
    GameEventContext {
        source_entity: source.map(|e| e.id),
        affected_state: state,
        source_info: source,
    }
}

/// `activate(sourceEntity, level, pos, state, calculatedPower, vibrationFrequency)`.
pub fn activate(
    level: &mut ServerLevel,
    source_entity: Option<SourceEntityInfo>,
    pos: BlockPos,
    state: BlockState,
    calculated_power: i32,
    vibration_frequency: i32,
) {
    level.set_block_and_update(
        pos,
        state
            .set(props::SCULK_SENSOR_PHASE, SculkSensorPhase::Active)
            .set(props::POWER, calculated_power),
    );
    level.schedule_tick_block(pos, state.block(), active_ticks(state));
    update_neighbours(level, pos, state);
    try_resonate_vibration(level, source_entity, pos, vibration_frequency);
    level.game_event_at_block(
        GameEvent::SCULK_SENSOR_TENDRILS_CLICKING,
        pos,
        &entity_context(source_entity, None),
    );
    if !state.get(props::WATERLOGGED) {
        let pitch = level.random().next_float() * 0.2 + 0.8;
        level.play_sound_at(
            None,
            f64::from(pos.x()) + 0.5,
            f64::from(pos.y()) + 0.5,
            f64::from(pos.z()) + 0.5,
            SoundEvent::BLOCK_SCULK_SENSOR_CLICKING,
            SoundSource::Blocks,
            1.0,
            pitch,
        );
    }
}

/// `tryResonateVibration(sourceEntity, level, pos, vibrationFrequency)`.
pub fn try_resonate_vibration(
    level: &mut ServerLevel,
    source_entity: Option<SourceEntityInfo>,
    pos: BlockPos,
    vibration_frequency: i32,
) {
    for direction in Direction::VALUES {
        let relative_pos = pos.relative(direction);
        let block_state = level.get_block_state(relative_pos);
        if block_state.is_tag("vibration_resonators") {
            level.game_event_at_block(
                get_resonance_event_by_frequency(vibration_frequency),
                relative_pos,
                &entity_context(source_entity, Some(block_state)),
            );
            let pitch = RESONANCE_PITCH_BEND[vibration_frequency as usize];
            level.play_sound(
                None,
                relative_pos,
                SoundEvent::BLOCK_AMETHYST_BLOCK_RESONATE,
                SoundSource::Blocks,
                1.0,
                pitch,
            );
        }
    }
}

/// `SculkSensorBlock.stepOn` (the same for the calibrated sensor).
pub fn step_on_sensor(
    level: &mut ServerLevel,
    pos: BlockPos,
    on_state: BlockState,
    entity: &mut dyn Entity,
) {
    if can_activate(on_state)
        && entity.get_type() != EntityType::WARDEN
        && let Some(mut sensor) = BlockEntityVibration::at(level, pos)
        && sensor.can_receive_vibration(
            level,
            pos,
            GameEvent::STEP,
            &GameEventContext::of_state(Some(on_state)),
        )
    {
        let context = GameEventContext::of_entity(Some(entity.id()));
        level.expose(&*entity, |level| {
            vibrations::force_schedule_vibration(
                level,
                &mut sensor,
                GameEvent::STEP,
                &context,
                entity.position(),
            )
        });
    }
}

pub struct SculkSensorBlock;

impl BlockBehaviour for SculkSensorBlock {
    crate::simple_waterlogged!();
    fn get_state_for_placement(
        &self,
        block: Block,
        level: &mut ServerLevel,
        context: &BlockPlaceContext,
    ) -> Option<BlockState> {
        let replaced_fluid = level.get_fluid_state(context.get_clicked_pos());
        Some(
            block
                .default_state()
                .set(props::WATERLOGGED, replaced_fluid.fluid() == Fluid::WATER),
        )
    }

    fn tick(&self, state: BlockState, level: &mut ServerLevel, pos: BlockPos) {
        match get_phase(state) {
            SculkSensorPhase::Active => deactivate(level, pos, state),
            SculkSensorPhase::Cooldown => {
                level.set_block_and_update(
                    pos,
                    state.set(props::SCULK_SENSOR_PHASE, SculkSensorPhase::Inactive),
                );
                if !state.get(props::WATERLOGGED) {
                    let pitch = level.random().next_float() * 0.2 + 0.8;
                    level.play_sound(
                        None,
                        pos,
                        SoundEvent::BLOCK_SCULK_SENSOR_CLICKING_STOP,
                        SoundSource::Blocks,
                        1.0,
                        pitch,
                    );
                }
            }
            SculkSensorPhase::Inactive => {}
        }
    }

    fn step_on(
        &self,
        level: &mut ServerLevel,
        pos: BlockPos,
        on_state: BlockState,
        entity: &mut dyn Entity,
    ) {
        step_on_sensor(level, pos, on_state, entity);
    }

    fn on_place(
        &self,
        state: BlockState,
        level: &mut ServerLevel,
        pos: BlockPos,
        old_state: BlockState,
        _moved_by_piston: bool,
    ) {
        if state.block() != old_state.block()
            && state.get(props::POWER) > 0
            && !level.has_scheduled_block_tick(pos, state.block())
        {
            level.set_block_flags(
                pos,
                state.set(props::POWER, 0),
                UPDATE_CLIENTS | UPDATE_KNOWN_SHAPE,
            );
        }
    }

    fn affect_neighbors_after_removal(
        &self,
        state: BlockState,
        level: &mut ServerLevel,
        pos: BlockPos,
        _moved_by_piston: bool,
    ) {
        if get_phase(state) == SculkSensorPhase::Active {
            update_neighbours(level, pos, state);
        }
    }

    update_shape!(
        |self, state, level, pos, direction, neighbour_pos, neighbour_state| {
            if state.get(props::WATERLOGGED) {
                helpers::schedule_water_tick(level, pos);
            }
            state
        }
    );

    fn spawn_after_break(
        &self,
        _state: BlockState,
        level: &mut ServerLevel,
        pos: BlockPos,
        tool: &crate::world::item::ItemStack,
        drop_experience: bool,
    ) {
        if drop_experience {
            helpers::try_drop_experience(
                level,
                pos,
                tool,
                &crate::registry::typed::int_provider::IntProvider::Constant(5),
            );
        }
    }

    fn own_signal(&self, state: BlockState, _level: &dyn BlockGetter, _pos: BlockPos) -> i32 {
        state.get(props::POWER)
    }

    fn get_direct_signal(
        &self,
        state: BlockState,
        level: &dyn BlockGetter,
        pos: BlockPos,
        direction: Direction,
    ) -> i32 {
        if direction == Direction::Up {
            state.get_signal(level, pos, direction)
        } else {
            0
        }
    }

    fn get_analog_output_signal(
        &self,
        state: BlockState,
        level: &ServerLevel,
        pos: BlockPos,
        _direction: Direction,
    ) -> i32 {
        analog_output_signal(state, level, pos)
    }
}

/// `SculkSensorBlock.getAnalogOutputSignal`.
pub fn analog_output_signal(state: BlockState, level: &ServerLevel, pos: BlockPos) -> i32 {
    match level
        .get_block_entity(pos)
        .and_then(|be| be.as_any().downcast_ref::<SculkSensorBlockEntity>())
    {
        Some(sculk) if get_phase(state) == SculkSensorPhase::Active => {
            sculk.last_vibration_frequency
        }
        _ => 0,
    }
}

register_block_class!("SculkSensorBlock", |_| Box::new(SculkSensorBlock));
