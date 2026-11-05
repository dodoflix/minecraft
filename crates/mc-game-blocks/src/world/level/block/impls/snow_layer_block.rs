//! `net.minecraft.world.level.block.SnowLayerBlock`.
use mc_game_core::{register_block_class, update_shape};
use mc_game_core::world::level::block::impls::prelude::*;
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::item::BlockPlaceContext;
use mc_game_core::world::level::LightLayer;
use mc_game_core::world::level::gameevent::GameEventContext;
use mc_data::GameEvent;

pub struct SnowLayerBlock;

impl BlockBehaviour for SnowLayerBlock {
    fn can_survive(&self, state: BlockState, level: &dyn LevelReader, pos: BlockPos) -> bool {
        let below_state = level.get_block_state(pos.below());
        if below_state.is_tag("cannot_support_snow_layer") {
            false
        } else if below_state.is_tag("support_override_snow_layer") {
            true
        } else {
            helpers::is_face_full(&below_state.get_collision_shape(level, pos.below()), Direction::Up)
                || (below_state.is_block(state.block()) && below_state.get(props::LAYERS) == 8)
        }
    }
    fn random_tick(&self, state: BlockState, level: &mut ServerLevel, pos: BlockPos) {
        if level.get_brightness(LightLayer::Block, pos) > 11 {
            helpers::drop_resources(level, state, pos, None);
            level.remove_block(pos, false);
            level.game_event_at_block(GameEvent::BLOCK_DESTROY, pos, &GameEventContext::of_state(Some(state)));
        }
    }
    fn can_be_replaced(&self, state: BlockState, context: &BlockPlaceContext) -> bool {
        let layers = state.get(props::LAYERS);
        let held = context.use_on.get_item_in_hand();
        if held.is_empty() || Some(held.item()) != state.block().item() || layers >= 8 {
            layers == 1
        } else {
            // shortcut: a dispenser's DirectionalPlaceContext recurses forever in vanilla here; it counts as replacing the
            // clicked block (checked against the face like a player click)
            if context.replacing_clicked_on_block(true) { context.get_clicked_face() == Direction::Up } else { true }
        }
    }
    fn get_state_for_placement(&self, block: Block, level: &mut ServerLevel, context: &BlockPlaceContext) -> Option<BlockState> {
        let state = level.get_block_state(context.get_clicked_pos());
        if state.is_block(block) {
            Some(state.set(props::LAYERS, (state.get(props::LAYERS) + 1).min(8)))
        } else {
            Some(block.default_state())
        }
    }
    update_shape!(|self, state, level, pos, direction, neighbour_pos, neighbour_state| {
        if !state.can_survive(&*level, pos) { air() } else { state }
    });
}

register_block_class!("SnowLayerBlock", |_| Box::new(SnowLayerBlock));
