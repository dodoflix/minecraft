//! `net.minecraft.world.level.block.ComparatorBlock` (extends `DiodeBlock`, `EntityBlock`).
use crate::world::block_entity::ComparatorBlockEntity;
use mc_data::enums::ComparatorMode;
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::container::unpack_loot_for_signal;
use mc_game_core::world::entity::Entity as _;
use mc_game_core::world::item::InteractionResult;
use mc_game_core::world::level::block::impls::diode_block::{self, Diode, diode_behaviour};
use mc_game_core::world::level::block::impls::prelude::*;
use mc_game_core::{register_block_class, update_shape};
use mc_protocol::packets::play::level::SoundSource;
use mc_world::ticks::TickPriority;

pub struct ComparatorBlock;

/// `level.getBlockEntity(pos) instanceof ComparatorBlockEntity be ? be.getOutputSignal() : 0`.
fn stored_output(level: &dyn BlockGetter, pos: BlockPos) -> i32 {
    level
        .get_block_entity(pos)
        .and_then(|be| be.as_any().downcast_ref::<ComparatorBlockEntity>())
        .map_or(0, ComparatorBlockEntity::get_output_signal)
}

/// `ComparatorBlock.calculateOutputSignal`.
fn calculate_output_signal(level: &mut ServerLevel, pos: BlockPos, state: BlockState) -> i32 {
    let input_signal = ComparatorBlock.get_input_signal(level, pos, state);
    if input_signal == 0 {
        return 0;
    }
    let alternate_signal = diode_block::get_alternate_signal(&ComparatorBlock, level, pos, state);
    if alternate_signal > input_signal {
        0
    } else if state.get(props::MODE_COMPARATOR) == ComparatorMode::Subtract {
        input_signal - alternate_signal
    } else {
        input_signal
    }
}

/// `ComparatorBlock.refreshOutputState`.
fn refresh_output_state(level: &mut ServerLevel, pos: BlockPos, state: BlockState) {
    let output_value = calculate_output_signal(level, pos, state);
    let mut old_value = 0;
    if let Some(be) = level.get_block_entity_immediate(pos)
        && let Some(comparator) = be.as_any_mut().downcast_mut::<ComparatorBlockEntity>()
    {
        old_value = comparator.get_output_signal();
        comparator.set_output_signal(output_value);
    }
    if old_value != output_value || state.get(props::MODE_COMPARATOR) == ComparatorMode::Compare {
        let source_on = ComparatorBlock.should_turn_on(level, pos, state);
        let is_on = state.get(props::POWERED);
        if is_on && !source_on {
            level.set_block_flags(pos, state.set(props::POWERED, false), 2);
        } else if !is_on && source_on {
            level.set_block_flags(pos, state.set(props::POWERED, true), 2);
        }
        diode_block::update_neighbors_in_front(level, pos, state);
    }
}

impl Diode for ComparatorBlock {
    fn get_delay(&self, _state: BlockState) -> i32 {
        2
    }
    fn get_output_signal(&self, level: &dyn BlockGetter, pos: BlockPos, _state: BlockState) -> i32 {
        stored_output(level, pos)
    }
    fn should_turn_on(&self, level: &mut ServerLevel, pos: BlockPos, state: BlockState) -> bool {
        let input = self.get_input_signal(level, pos, state);
        if input == 0 {
            return false;
        }
        let side_input = diode_block::get_alternate_signal(self, level, pos, state);
        input > side_input
            || (input == side_input && state.get(props::MODE_COMPARATOR) == ComparatorMode::Compare)
    }
    fn get_input_signal(&self, level: &mut ServerLevel, pos: BlockPos, state: BlockState) -> i32 {
        let mut result_signal = diode_block::base_input_signal(level, pos, state);
        let direction = state.get(props::HORIZONTAL_FACING);
        let mut target_pos = pos.relative(direction);
        let mut target_state = level.get_block_state(target_pos);
        if target_state.has_analog_output_signal() {
            unpack_loot_for_signal(level, target_state, target_pos);
            result_signal =
                target_state.get_analog_output_signal(level, target_pos, direction.get_opposite());
        } else if result_signal < 15 && target_state.is_redstone_conductor() {
            target_pos = target_pos.relative(direction);
            target_state = level.get_block_state(target_pos);
            let item_frame_signal = item_frame_analog_output(level, direction, target_pos);
            let block_signal = if target_state.has_analog_output_signal() {
                unpack_loot_for_signal(level, target_state, target_pos);
                target_state.get_analog_output_signal(level, target_pos, direction.get_opposite())
            } else {
                i32::MIN
            };
            let item_frame_or_block_signal =
                item_frame_signal.unwrap_or(i32::MIN).max(block_signal);
            if item_frame_or_block_signal != i32::MIN {
                result_signal = item_frame_or_block_signal;
            }
        }
        result_signal
    }
    /// `tick`.
    fn diode_tick(&self, state: BlockState, level: &mut ServerLevel, pos: BlockPos) {
        refresh_output_state(level, pos, state);
    }
    fn check_tick_on_neighbor(&self, level: &mut ServerLevel, pos: BlockPos, state: BlockState) {
        if !level.will_tick_block_this_tick(pos, state.block()) {
            let output_value = calculate_output_signal(level, pos, state);
            let old_value = stored_output(&*level, pos);
            if output_value != old_value
                || state.get(props::POWERED) != self.should_turn_on(level, pos, state)
            {
                let priority = if diode_block::should_prioritize(&*level, pos, state) {
                    TickPriority::High
                } else {
                    TickPriority::Normal
                };
                level.schedule_block_tick(pos, state.block(), 2, priority);
            }
        }
    }
}

/// `getItemFrame(level, direction, pos)` + `ItemFrame.getAnalogOutput()`: the single item frame in `pos` facing
/// `direction`, if any.
fn item_frame_analog_output(
    level: &ServerLevel,
    direction: Direction,
    pos: BlockPos,
) -> Option<i32> {
    use mc_game_core::world::entity::decoration::item_frame::analog_output_of;
    let bb = mc_core::phys::AABB::new(
        f64::from(pos.x()),
        f64::from(pos.y()),
        f64::from(pos.z()),
        f64::from(pos.x() + 1),
        f64::from(pos.y() + 1),
        f64::from(pos.z() + 1),
    );
    let frames: Vec<i32> = level
        .get_entities_of_class(
            &bb,
            |e| analog_output_of(e).is_some(),
            |e| {
                analog_output_of(e).is_some_and(|(frame_direction, _)| frame_direction == direction)
            },
        )
        .into_iter()
        .filter_map(|id| {
            level
                .get_entity(id)
                .and_then(analog_output_of)
                .map(|(_, output)| output)
        })
        .collect();
    if frames.len() == 1 {
        Some(frames[0])
    } else {
        None
    }
}

impl BlockBehaviour for ComparatorBlock {
    diode_behaviour!();
    fn get_state_for_placement(
        &self,
        block: Block,
        _level: &mut ServerLevel,
        context: &mc_game_core::world::item::BlockPlaceContext,
    ) -> Option<BlockState> {
        Some(diode_block::get_state_for_placement(block, context))
    }
    update_shape!(
        |self, state, level, pos, direction, neighbour_pos, neighbour_state| {
            if direction == Direction::Down
                && !diode_block::can_survive_on(&*level, neighbour_pos, neighbour_state)
            {
                air()
            } else {
                state
            }
        }
    );
    fn use_without_item(
        &self,
        state: BlockState,
        level: &mut ServerLevel,
        pos: BlockPos,
        player: &mut mc_game_core::world::entity::ServerPlayer,
        _hit: &mc_core::phys::BlockHitResult,
    ) -> InteractionResult {
        if !player.player.abilities.may_build {
            return InteractionResult::PASS;
        }
        let state = state.cycle(props::MODE_COMPARATOR);
        let pitch = if state.get(props::MODE_COMPARATOR) == ComparatorMode::Subtract {
            0.55
        } else {
            0.5
        };
        level.play_sound(
            Some(player.id()),
            pos,
            mc_data::SoundEvent::BLOCK_COMPARATOR_CLICK,
            SoundSource::Blocks,
            0.3,
            pitch,
        );
        level.set_block_flags(pos, state, 2);
        if level.get_block_state(pos).is_block(state.block()) {
            refresh_output_state(level, pos, state);
        }
        InteractionResult::SUCCESS
    }
}

register_block_class!("ComparatorBlock", |_| Box::new(ComparatorBlock));
