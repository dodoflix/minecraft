//! `net.minecraft.world.level.block.TrapDoorBlock` (extends `HorizontalDirectionalBlock`, `SimpleWaterloggedBlock`).
use mc_core::phys::{BlockHitResult, Vec3};
use mc_core::random::RandomSource;
use mc_data::GameEvent;
use mc_data::block::PathComputationType;
use mc_data::enums::Half;
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::entity::{EntityId, ServerPlayer};
use mc_game_core::world::item::{BlockPlaceContext, InteractionResult, ItemStack};
use mc_game_core::world::level::block::block_set_type::BlockSetType;
use mc_game_core::world::level::block::impls::prelude::*;
use mc_game_core::world::level::gameevent::GameEventContext;
use mc_game_core::world::level::redstone::Orientation;
use mc_game_core::world::level::signal_getter::has_neighbor_signal;
use mc_game_core::{register_block_class, update_shape};
use mc_protocol::packets::play::level::SoundSource;

pub struct TrapDoorBlock {
    pub set_type: &'static BlockSetType,
}

impl TrapDoorBlock {
    fn new(block: Block) -> Self {
        Self {
            set_type: BlockSetType::for_block(block).expect("trapdoor without BlockSetType"),
        }
    }

    /// `TrapDoorBlock.playSound(player, level, pos, opening)`.
    fn play_sound(
        &self,
        player: Option<EntityId>,
        level: &mut ServerLevel,
        pos: BlockPos,
        opening: bool,
    ) {
        let pitch = level.random().next_float() * 0.1 + 0.9;
        let sound = if opening {
            self.set_type.trapdoor_open
        } else {
            self.set_type.trapdoor_close
        };
        level.play_sound(player, pos, sound, SoundSource::Blocks, 1.0, pitch);
        let event = if opening {
            GameEvent::BLOCK_OPEN
        } else {
            GameEvent::BLOCK_CLOSE
        };
        level.game_event(
            event,
            Vec3::at_center_of(&pos),
            &GameEventContext::of(player, None),
        );
    }

    /// `TrapDoorBlock.toggle(state, level, pos, player)`.
    fn toggle(
        &self,
        state: BlockState,
        level: &mut ServerLevel,
        pos: BlockPos,
        player: Option<EntityId>,
    ) {
        let updated = state.cycle(props::OPEN);
        level.set_block_flags(pos, updated, 2);
        if updated.get(props::WATERLOGGED) {
            helpers::schedule_water_tick(level, pos);
        }
        self.play_sound(player, level, pos, updated.get(props::OPEN));
    }

    fn on_explosion_hit_inner(
        &self,
        state: BlockState,
        level: &mut ServerLevel,
        pos: BlockPos,
        can_trigger_blocks: bool,
    ) {
        if can_trigger_blocks && self.set_type.can_open_by_wind_charge && !state.get(props::POWERED)
        {
            self.toggle(state, level, pos, None);
        }
    }
}

impl BlockBehaviour for TrapDoorBlock {
    fn on_explosion_hit(
        &self,
        state: BlockState,
        level: &mut ServerLevel,
        pos: BlockPos,
        explosion: &dyn mc_game_core::world::level::explosion::Explosion,
        on_hit: &mut dyn FnMut(ItemStack, BlockPos),
    ) {
        self.on_explosion_hit_inner(state, level, pos, explosion.can_trigger_blocks());
        mc_game_core::world::level::explosion::default_on_explosion_hit(
            state, level, pos, explosion, on_hit,
        );
    }
    mc_game_core::simple_waterlogged!();
    update_shape!(
        |self, state, level, pos, direction, neighbour_pos, neighbour_state| {
            if state.get(props::WATERLOGGED) {
                helpers::schedule_water_tick(level, pos);
            }
            state
        }
    );
    fn is_pathfindable(&self, state: BlockState, ty: PathComputationType) -> bool {
        match ty {
            PathComputationType::Land | PathComputationType::Air => state.get(props::OPEN),
            PathComputationType::Water => state.get(props::WATERLOGGED),
        }
    }
    fn use_without_item(
        &self,
        state: BlockState,
        level: &mut ServerLevel,
        pos: BlockPos,
        player: &mut ServerPlayer,
        _hit: &BlockHitResult,
    ) -> InteractionResult {
        if !self.set_type.can_open_by_hand {
            InteractionResult::PASS
        } else {
            self.toggle(state, level, pos, Some(player.base_id()));
            InteractionResult::SUCCESS
        }
    }
    fn neighbor_changed(
        &self,
        state: BlockState,
        level: &mut ServerLevel,
        pos: BlockPos,
        _block: Block,
        _orientation: Option<Orientation>,
        _moved_by_piston: bool,
    ) {
        let signal = has_neighbor_signal(&*level, pos);
        if signal != state.get(props::POWERED) {
            let mut state = state;
            if state.get(props::OPEN) != signal {
                state = state.set(props::OPEN, signal);
                self.play_sound(None, level, pos, signal);
            }
            level.set_block_flags(pos, state.set(props::POWERED, signal), 2);
            if state.get(props::WATERLOGGED) {
                helpers::schedule_water_tick(level, pos);
            }
        }
    }
    fn get_state_for_placement(
        &self,
        block: Block,
        level: &mut ServerLevel,
        context: &BlockPlaceContext,
    ) -> Option<BlockState> {
        let mut state = block.default_state();
        let pos = context.get_clicked_pos();
        let replaced_fluid_state = level.get_fluid_state(pos);
        let clicked_face = context.get_clicked_face();
        if !helpers::replacing_clicked_on_block(level, context)
            && clicked_face.get_axis().is_horizontal()
        {
            let top = context.use_on.get_click_location().y - pos.y() as f64 > 0.5;
            state = state
                .set(props::HORIZONTAL_FACING, clicked_face)
                .set(props::HALF, if top { Half::Top } else { Half::Bottom });
        } else {
            state = state
                .set(
                    props::HORIZONTAL_FACING,
                    context.get_horizontal_direction().get_opposite(),
                )
                .set(
                    props::HALF,
                    if clicked_face == Direction::Up {
                        Half::Bottom
                    } else {
                        Half::Top
                    },
                );
        }
        if has_neighbor_signal(&*level, pos) {
            state = state.set(props::OPEN, true).set(props::POWERED, true);
        }
        Some(state.set(
            props::WATERLOGGED,
            replaced_fluid_state.get_type() == mc_data::fluid::Fluid::WATER,
        ))
    }
    fn rotate(&self, state: BlockState, rotation: Rotation) -> BlockState {
        helpers::rotate_facing(state, props::HORIZONTAL_FACING, rotation)
    }
    fn mirror(&self, state: BlockState, mirror: Mirror) -> BlockState {
        helpers::mirror_facing(state, props::HORIZONTAL_FACING, mirror)
    }
}

register_block_class!("TrapDoorBlock", |block| Box::new(TrapDoorBlock::new(block)));
