//! `net.minecraft.world.level.block.BasePressurePlateBlock` and its two subclasses `PressurePlateBlock` (powered
//! boolean, sensitivity from the `BlockSetType`) and `WeightedPressurePlateBlock` (power from the entity count).
use mc_core::math::mth;
use mc_core::phys::AABB;
use mc_data::GameEvent;
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::entity::{Entity, EntityId};
use mc_game_core::world::level::block::block_set_type::{BlockSetType, PressurePlateSensitivity};
use mc_game_core::world::level::block::impls::prelude::*;
use mc_game_core::world::level::gameevent::GameEventContext;
use mc_game_core::{register_block_class, update_shape};
use mc_protocol::packets::play::level::SoundSource;

/// `PressurePlateBlock` (`max_weight == None`) or `WeightedPressurePlateBlock`.
pub struct BasePressurePlateBlock {
    ty: &'static BlockSetType,
    max_weight: Option<i32>,
}

/// `BasePressurePlateBlock.TOUCH_AABB` = `Block.column(14.0, 0.0, 4.0).toAabbs().getFirst()`.
fn touch_aabb(pos: BlockPos) -> AABB {
    AABB::new(1.0 / 16.0, 0.0, 1.0 / 16.0, 15.0 / 16.0, 0.25, 15.0 / 16.0).move_(
        f64::from(pos.x()),
        f64::from(pos.y()),
        f64::from(pos.z()),
    )
}

impl BasePressurePlateBlock {
    pub fn new(block: Block, max_weight: Option<i32>) -> Self {
        Self {
            ty: BlockSetType::for_block(block).expect("every pressure plate has a block set type"),
            max_weight,
        }
    }

    /// `getSignalForState`.
    fn get_signal_for_state(&self, state: BlockState) -> i32 {
        match self.max_weight {
            None => {
                if state.get(props::POWERED) {
                    15
                } else {
                    0
                }
            }
            Some(_) => state.get(props::POWER),
        }
    }

    /// `setSignalForState`.
    fn set_signal_for_state(&self, state: BlockState, signal: i32) -> BlockState {
        match self.max_weight {
            None => state.set(props::POWERED, signal > 0),
            Some(_) => state.set(props::POWER, signal),
        }
    }

    /// `getPressedTime`.
    fn get_pressed_time(&self) -> i32 {
        if self.max_weight.is_some() { 10 } else { 20 }
    }

    /// `getEntityCount(level, TOUCH_AABB.move(pos), entityClass)`: `living_only` selects `LivingEntity.class`.
    fn get_entity_count(level: &ServerLevel, pos: BlockPos, living_only: bool) -> i32 {
        level
            .get_entities_of_class(
                &touch_aabb(pos),
                |e| !living_only || e.living().is_some(),
                |e| !e.is_spectator() && !e.is_ignoring_block_triggers(),
            )
            .len() as i32
    }

    /// `getSignalStrength`.
    fn get_signal_strength(&self, level: &ServerLevel, pos: BlockPos) -> i32 {
        match self.max_weight {
            None => {
                let living_only =
                    self.ty.pressure_plate_sensitivity == PressurePlateSensitivity::Mobs;
                if Self::get_entity_count(level, pos, living_only) > 0 {
                    15
                } else {
                    0
                }
            }
            Some(max_weight) => {
                let count = Self::get_entity_count(level, pos, false).min(max_weight);
                if count > 0 {
                    let percent = count.min(max_weight) as f32 / max_weight as f32;
                    mth::ceil_f32(percent * 15.0)
                } else {
                    0
                }
            }
        }
    }

    /// `checkPressed(sourceEntity, level, pos, state, oldSignal)`.
    fn check_pressed(
        &self,
        source: Option<EntityId>,
        level: &mut ServerLevel,
        pos: BlockPos,
        state: BlockState,
        old_signal: i32,
    ) {
        let signal = self.get_signal_strength(level, pos);
        let was_pressed = old_signal > 0;
        let is_pressed = signal > 0;
        if old_signal != signal {
            let new_state = self.set_signal_for_state(state, signal);
            level.set_block_flags(pos, new_state, 2);
            self.update_neighbours(level, pos, state.block());
        }
        if !is_pressed && was_pressed {
            level.play_sound(
                None,
                pos,
                self.ty.pressure_plate_click_off,
                SoundSource::Blocks,
                1.0,
                1.0,
            );
            Self::emit(level, source, GameEvent::BLOCK_DEACTIVATE, pos);
        } else if is_pressed && !was_pressed {
            level.play_sound(
                None,
                pos,
                self.ty.pressure_plate_click_on,
                SoundSource::Blocks,
                1.0,
                1.0,
            );
            Self::emit(level, source, GameEvent::BLOCK_ACTIVATE, pos);
        }
        if is_pressed {
            level.schedule_tick_block(pos, state.block(), self.get_pressed_time());
        }
    }

    /// `gameEvent(sourceEntity, event, pos)`.
    fn emit(level: &mut ServerLevel, source: Option<EntityId>, event: GameEvent, pos: BlockPos) {
        level.game_event_at_block(event, pos, &GameEventContext::of_entity(source));
    }

    /// `updateNeighbours`.
    fn update_neighbours(&self, level: &mut ServerLevel, pos: BlockPos, block: Block) {
        level.update_neighbors_at(pos, block);
        level.update_neighbors_at(pos.below(), block);
    }
}

impl BlockBehaviour for BasePressurePlateBlock {
    fn can_survive(&self, _state: BlockState, level: &dyn LevelReader, pos: BlockPos) -> bool {
        let below = pos.below();
        helpers::can_support_rigid_block(level, below)
            || helpers::can_support_center(level, below, Direction::Up)
    }
    update_shape!(
        |self, state, level, pos, direction, neighbour_pos, neighbour_state| {
            if direction == Direction::Down && !state.can_survive(&*level, pos) {
                air()
            } else {
                state
            }
        }
    );
    fn is_possible_to_respawn_in_this(&self, _state: BlockState) -> bool {
        true
    }
    fn tick(&self, state: BlockState, level: &mut ServerLevel, pos: BlockPos) {
        let signal = self.get_signal_for_state(state);
        if signal > 0 {
            self.check_pressed(None, level, pos, state, signal);
        }
    }
    fn entity_inside(
        &self,
        state: BlockState,
        level: &mut ServerLevel,
        pos: BlockPos,
        entity: &mut dyn Entity,
        _effect_applier: &mut dyn mc_game_core::world::entity::inside_block_effects::InsideBlockEffectApplier,
        _is_precise: bool,
    ) {
        let signal = self.get_signal_for_state(state);
        if signal == 0 {
            // the entity runs outside the level's storage: lent, the box query finds it
            let id = entity.id();
            level.lend(entity, |level| {
                self.check_pressed(Some(id), level, pos, state, signal)
            });
        }
    }
    fn affect_neighbors_after_removal(
        &self,
        state: BlockState,
        level: &mut ServerLevel,
        pos: BlockPos,
        moved_by_piston: bool,
    ) {
        if !moved_by_piston && self.get_signal_for_state(state) > 0 {
            self.update_neighbours(level, pos, state.block());
        }
    }
    fn get_direct_signal(
        &self,
        state: BlockState,
        _level: &dyn BlockGetter,
        _pos: BlockPos,
        direction: Direction,
    ) -> i32 {
        if direction == Direction::Up {
            self.get_signal_for_state(state)
        } else {
            0
        }
    }
    fn own_signal(&self, state: BlockState, _level: &dyn BlockGetter, _pos: BlockPos) -> i32 {
        self.get_signal_for_state(state)
    }
}

register_block_class!("PressurePlateBlock", |block| Box::new(
    BasePressurePlateBlock::new(block, None)
));
register_block_class!("WeightedPressurePlateBlock", |block| {
    let max_weight = if block.data().name == "minecraft:light_weighted_pressure_plate" {
        15
    } else {
        150
    };
    Box::new(BasePressurePlateBlock::new(block, Some(max_weight)))
});
