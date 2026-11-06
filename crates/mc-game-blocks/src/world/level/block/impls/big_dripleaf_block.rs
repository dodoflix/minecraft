//! `net.minecraft.world.level.block.BigDripleafBlock` (extends `HorizontalDirectionalBlock`).
use mc_core::random::RandomSource;
use mc_data::SoundEvent;
use mc_data::enums::Tilt;
use mc_data::fluid::{Fluid, FluidState};
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::level::block::bonemeal::BonemealSource;
use mc_game_core::world::level::block::impls::prelude::*;
use mc_game_core::world::level::gameevent::GameEventContext;
use mc_game_core::{register_block_class, update_shape};
use mc_protocol::packets::play::level::SoundSource;

pub struct BigDripleafBlock;

/// `Tilt.causesVibration`.
fn causes_vibration(tilt: Tilt) -> bool {
    tilt != Tilt::Unstable
}

/// `DELAY_UNTIL_NEXT_TILT_STATE` (`-1` = `NO_TICK`).
fn delay_until_next_tilt_state(tilt: Tilt) -> i32 {
    match tilt {
        Tilt::Unstable | Tilt::Partial => 10,
        Tilt::Full => 100,
        Tilt::None => -1,
    }
}

/// `BigDripleafBlock.placeWithRandomHeight`.
pub fn place_with_random_height(
    level: &mut dyn LevelAccessor,
    stem_bottom_pos: BlockPos,
    facing: Direction,
) {
    let desired_height = mc_core::math::mth::next_int(level.random(), 2, 5);
    let mut pos = stem_bottom_pos;
    let mut height = 0;
    while height < desired_height && can_place_at(&*level, pos) {
        height += 1;
        pos = pos.above();
    }
    let leaf_y = stem_bottom_pos.y() + height - 1;
    pos = BlockPos::new(pos.x(), stem_bottom_pos.y(), pos.z());
    while pos.y() < leaf_y {
        let fluid = level.get_fluid_state(pos);
        super::big_dripleaf_stem_block::place(level, pos, fluid, facing);
        pos = pos.above();
    }
    let fluid = level.get_fluid_state(pos);
    place(level, pos, fluid, facing);
}

/// `canReplace`.
fn can_replace(old_state: BlockState) -> bool {
    old_state.is_air()
        || old_state.is_block(Block::WATER)
        || old_state.is_block(Block::SMALL_DRIPLEAF)
}

/// `canPlaceAt` (= `canGrowInto`).
pub fn can_place_at(level: &dyn LevelReader, pos: BlockPos) -> bool {
    can_grow_into(level, pos)
}

/// `canGrowInto`.
pub fn can_grow_into(level: &dyn LevelReader, pos: BlockPos) -> bool {
    !level.is_outside_build_height(pos.y()) && can_replace(level.get_block_state(pos))
}

/// `BigDripleafBlock.place`.
pub fn place(
    level: &mut dyn LevelAccessor,
    pos: BlockPos,
    fluid_state: FluidState,
    facing: Direction,
) -> bool {
    let new_state = Block::BIG_DRIPLEAF
        .default_state()
        .set(
            props::WATERLOGGED,
            fluid_state.is_source_of_type(Fluid::WATER),
        )
        .set(props::HORIZONTAL_FACING, facing);
    level.set_block_and_update(pos, new_state)
}

/// `playTiltSound`.
fn play_tilt_sound(level: &mut ServerLevel, pos: BlockPos, tilt_sound: SoundEvent) {
    let pitch = mc_core::math::mth::random_between(level.random(), 0.8, 1.2);
    level.play_sound(None, pos, tilt_sound, SoundSource::Blocks, 1.0, pitch);
}

/// `setTilt`.
fn set_tilt(state: BlockState, level: &mut ServerLevel, pos: BlockPos, tilt: Tilt) {
    let previous_tilt = state.get(props::TILT);
    level.set_block_flags(pos, state.set(props::TILT, tilt), 2);
    if causes_vibration(tilt) && tilt != previous_tilt {
        level.game_event_at_block(
            mc_data::GameEvent::BLOCK_CHANGE,
            pos,
            &GameEventContext::of_entity(None),
        );
    }
}

/// `resetTilt`.
fn reset_tilt(state: BlockState, level: &mut ServerLevel, pos: BlockPos) {
    set_tilt(state, level, pos, Tilt::None);
    if state.get(props::TILT) != Tilt::None {
        play_tilt_sound(level, pos, SoundEvent::BLOCK_BIG_DRIPLEAF_TILT_UP);
    }
}

/// `setTiltAndScheduleTick`.
fn set_tilt_and_schedule_tick(
    state: BlockState,
    level: &mut ServerLevel,
    pos: BlockPos,
    tilt: Tilt,
    sound: Option<SoundEvent>,
) {
    set_tilt(state, level, pos, tilt);
    if let Some(sound) = sound {
        play_tilt_sound(level, pos, sound);
    }
    let tick_delay = delay_until_next_tilt_state(tilt);
    if tick_delay != -1 {
        level.schedule_tick_block(pos, Block::BIG_DRIPLEAF, tick_delay);
    }
}

impl BlockBehaviour for BigDripleafBlock {
    mc_game_core::simple_waterlogged!();
    fn on_projectile_hit(
        &self,
        level: &mut ServerLevel,
        state: BlockState,
        hit: &mc_core::phys::BlockHitResult,
        _projectile: &mut dyn mc_game_core::world::entity::projectile::Projectile,
    ) {
        set_tilt_and_schedule_tick(
            state,
            level,
            hit.block_pos,
            Tilt::Full,
            Some(SoundEvent::BLOCK_BIG_DRIPLEAF_TILT_DOWN),
        );
    }
    fn entity_inside(
        &self,
        state: BlockState,
        level: &mut ServerLevel,
        pos: BlockPos,
        entity: &mut dyn mc_game_core::world::entity::Entity,
        _effect_applier: &mut dyn mc_game_core::world::entity::inside_block_effects::InsideBlockEffectApplier,
        _is_precise: bool,
    ) {
        // `canEntityTilt`: `onGround() && position().y > pos.getY() + 0.6875F`
        let can_entity_tilt = entity.base().on_ground()
            && entity.position().y > f64::from(pos.y() as f32 + 0.6875_f32);
        if state.get(props::TILT) == Tilt::None
            && can_entity_tilt
            && !mc_game_core::world::level::signal_getter::has_neighbor_signal(&*level, pos)
        {
            set_tilt_and_schedule_tick(state, level, pos, Tilt::Unstable, None);
        }
    }
    fn can_survive(&self, state: BlockState, level: &dyn LevelReader, pos: BlockPos) -> bool {
        let below_state = level.get_block_state(pos.below());
        below_state.is_block(state.block())
            || below_state.is_block(Block::BIG_DRIPLEAF_STEM)
            || below_state.is_tag("supports_big_dripleaf")
    }
    update_shape!(
        |self, state, level, pos, direction, neighbour_pos, neighbour_state| {
            if direction == Direction::Down && !state.can_survive(&*level, pos) {
                air()
            } else {
                if state.get(props::WATERLOGGED) {
                    helpers::schedule_water_tick(level, pos);
                }
                if direction == Direction::Up && neighbour_state.is_block(state.block()) {
                    Block::BIG_DRIPLEAF_STEM
                        .default_state()
                        .with_properties_of(state)
                } else {
                    state
                }
            }
        }
    );
    fn tick(&self, state: BlockState, level: &mut ServerLevel, pos: BlockPos) {
        if mc_game_core::world::level::signal_getter::has_neighbor_signal(&*level, pos) {
            reset_tilt(state, level, pos);
        } else {
            match state.get(props::TILT) {
                Tilt::Unstable => set_tilt_and_schedule_tick(
                    state,
                    level,
                    pos,
                    Tilt::Partial,
                    Some(SoundEvent::BLOCK_BIG_DRIPLEAF_TILT_DOWN),
                ),
                Tilt::Partial => set_tilt_and_schedule_tick(
                    state,
                    level,
                    pos,
                    Tilt::Full,
                    Some(SoundEvent::BLOCK_BIG_DRIPLEAF_TILT_DOWN),
                ),
                Tilt::Full => reset_tilt(state, level, pos),
                Tilt::None => {}
            }
        }
    }
    fn neighbor_changed(
        &self,
        state: BlockState,
        level: &mut ServerLevel,
        pos: BlockPos,
        _block: Block,
        _orientation: Option<mc_game_core::world::level::redstone::Orientation>,
        _moved_by_piston: bool,
    ) {
        if mc_game_core::world::level::signal_getter::has_neighbor_signal(&*level, pos) {
            reset_tilt(state, level, pos);
        }
    }
    fn get_state_for_placement(
        &self,
        block: Block,
        level: &mut ServerLevel,
        context: &mc_game_core::world::item::BlockPlaceContext,
    ) -> Option<BlockState> {
        let clicked_pos = context.get_clicked_pos();
        let below_state = level.get_block_state(clicked_pos.below());
        let fluid_state = level.get_fluid_state(clicked_pos);
        let below_is_dripleaf_part = below_state.is_block(Block::BIG_DRIPLEAF)
            || below_state.is_block(Block::BIG_DRIPLEAF_STEM);
        Some(
            block
                .default_state()
                .set(
                    props::WATERLOGGED,
                    fluid_state.is_source_of_type(Fluid::WATER),
                )
                .set(
                    props::HORIZONTAL_FACING,
                    if below_is_dripleaf_part {
                        below_state.get(props::HORIZONTAL_FACING)
                    } else {
                        context.use_on.get_horizontal_direction().get_opposite()
                    },
                ),
        )
    }
    fn is_bonemealable(&self) -> bool {
        true
    }
    fn is_valid_bonemeal_target(
        &self,
        level: &dyn LevelReader,
        pos: BlockPos,
        _state: BlockState,
        _source: BonemealSource,
    ) -> bool {
        can_grow_into(level, pos.above())
    }
    fn perform_bonemeal(
        &self,
        level: &mut ServerLevel,
        pos: BlockPos,
        state: BlockState,
        _source: BonemealSource,
    ) {
        let above_pos = pos.above();
        if can_place_at(&*level, above_pos) {
            let facing = state.get(props::HORIZONTAL_FACING);
            super::big_dripleaf_stem_block::place(level, pos, state.get_fluid_state(), facing);
            let fluid = level.get_block_state(above_pos).get_fluid_state();
            place(level, above_pos, fluid, facing);
        }
    }
    fn rotate(&self, state: BlockState, rotation: Rotation) -> BlockState {
        helpers::rotate_facing(state, props::HORIZONTAL_FACING, rotation)
    }
    fn mirror(&self, state: BlockState, mirror: Mirror) -> BlockState {
        helpers::mirror_facing(state, props::HORIZONTAL_FACING, mirror)
    }
}

register_block_class!("BigDripleafBlock", |_| Box::new(BigDripleafBlock));
