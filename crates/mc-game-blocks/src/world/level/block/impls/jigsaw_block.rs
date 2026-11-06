//! `net.minecraft.world.level.block.JigsawBlock`.
use mc_game_core::register_block_class;
use mc_game_core::world::level::block::impls::prelude::*;

pub struct JigsawBlock;

/// `mc_data`'s `FrontAndTop` (the property value) as the `mc_core` one that has the geometry (same constant order).
fn core_orientation(state: BlockState) -> mc_core::direction::FrontAndTop {
    mc_core::direction::FrontAndTop::ALL[state.get(props::ORIENTATION).ordinal()]
}

fn with_orientation(state: BlockState, orientation: mc_core::direction::FrontAndTop) -> BlockState {
    state.set(
        props::ORIENTATION,
        mc_data::enums::FrontAndTop::ALL[orientation.ordinal()],
    )
}

impl BlockBehaviour for JigsawBlock {
    fn rotate(&self, state: BlockState, rotation: Rotation) -> BlockState {
        with_orientation(
            state,
            rotation
                .rotation()
                .rotate_front_and_top(core_orientation(state))
                .expect("rotated jigsaw orientation"),
        )
    }
    fn mirror(&self, state: BlockState, mirror: Mirror) -> BlockState {
        with_orientation(
            state,
            mirror
                .rotation()
                .rotate_front_and_top(core_orientation(state))
                .expect("mirrored jigsaw orientation"),
        )
    }

    /// `getStateForPlacement`: the clicked face is the front; the top is up, or away from the player on a vertical face.
    fn get_state_for_placement(
        &self,
        block: Block,
        _level: &mut mc_game_core::server::level::ServerLevel,
        context: &mc_game_core::world::item::BlockPlaceContext,
    ) -> Option<BlockState> {
        let front = context.get_clicked_face();
        let top = if front.get_axis() == Axis::Y {
            context.get_horizontal_direction().get_opposite()
        } else {
            Direction::Up
        };
        Some(with_orientation(
            block.default_state(),
            mc_core::direction::FrontAndTop::from_front_and_top(front, top)
                .expect("valid jigsaw orientation"),
        ))
    }

    /// `useWithoutItem`: `player.openJigsawBlock(be)` is client-side only.
    fn use_without_item(
        &self,
        _state: BlockState,
        level: &mut mc_game_core::server::level::ServerLevel,
        pos: BlockPos,
        player: &mut mc_game_core::world::entity::ServerPlayer,
        _hit: &mc_core::phys::BlockHitResult,
    ) -> mc_game_core::world::item::InteractionResult {
        let is_jigsaw = level.get_block_entity_immediate(pos).is_some_and(|be| {
            be.as_any()
                .is::<mc_game_core::world::block_entity::JigsawBlockEntity>()
        });
        if is_jigsaw && player.can_use_game_master_blocks() {
            mc_game_core::world::item::InteractionResult::SUCCESS
        } else {
            mc_game_core::world::item::InteractionResult::PASS
        }
    }
}

/// `JigsawBlock.getFrontFacing(state)`.
pub fn get_front_facing(state: BlockState) -> Direction {
    core_orientation(state).front()
}

/// `JigsawBlock.getTopFacing(state)`.
pub fn get_top_facing(state: BlockState) -> Direction {
    core_orientation(state).top()
}

register_block_class!("JigsawBlock", |_| Box::new(JigsawBlock));
