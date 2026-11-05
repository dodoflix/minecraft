//! `net.minecraft.world.level.block.RedStoneOreBlock` (`isRandomlyTicking` = lit, static data). Particles are client side
//! but `spawnParticles` consumes the level random on the server too.
use super::prelude::*;
use crate::server::level::ServerLevel;
use crate::world::entity::{Entity, ServerPlayer};
use crate::world::item::{BlockPlaceContext, InteractionHand, InteractionResult, ItemStack, UseOnContext};
use mc_core::phys::BlockHitResult;
use mc_core::random::RandomSource;

pub struct RedStoneOreBlock;

/// `spawnParticles(level, pos)`: `addParticle` is a no-op on the server, only the `nextFloat` calls remain.
fn spawn_particles(level: &mut ServerLevel, pos: BlockPos) {
    for direction in Direction::ALL {
        if !BlockGetter::get_block_state(level, pos.relative(direction)).is_solid_render() {
            let axis = direction.get_axis();
            let random = LevelAccessor::random(level);
            for a in [Axis::X, Axis::Y, Axis::Z] {
                if axis != a {
                    random.next_float();
                }
            }
        }
    }
}

/// `interact(state, level, pos)`.
fn interact(state: BlockState, level: &mut ServerLevel, pos: BlockPos) {
    spawn_particles(level, pos);
    if !state.get(props::LIT) {
        level.set_block_and_update(pos, state.set(props::LIT, true));
    }
}

impl BlockBehaviour for RedStoneOreBlock {
    fn attack(&self, state: BlockState, level: &mut ServerLevel, pos: BlockPos, _player: &mut ServerPlayer) {
        interact(state, level, pos);
    }
    fn step_on(&self, level: &mut ServerLevel, pos: BlockPos, on_state: BlockState, entity: &mut dyn Entity) {
        // shortcut: `Entity.isSteppingCarefully` = `isShiftKeyDown` (Cat/Ocelot also check `isCrouching`), switch once the trait has it
        if !entity.is_shift_key_down() {
            interact(on_state, level, pos);
        }
    }
    fn use_item_on(&self, stack: &ItemStack, state: BlockState, level: &mut ServerLevel, pos: BlockPos, player: &mut ServerPlayer, hand: InteractionHand, hit: &BlockHitResult) -> InteractionResult {
        interact(state, level, pos);
        let clicked = hit.block_pos;
        let place_context = BlockPlaceContext::new(UseOnContext::new(Some(player.context_player()), hand, stack.clone(), *hit), |c| {
            BlockGetter::get_block_state(level, clicked).can_be_replaced_by(c)
        });
        let can_place = place_context.can_place(|c| BlockGetter::get_block_state(level, c.get_clicked_pos()).can_be_replaced_by(c));
        if !stack.is_empty() && stack.item().data().classes.contains(&"BlockItem") && can_place { InteractionResult::PASS } else { InteractionResult::SUCCESS }
    }
    fn random_tick(&self, state: BlockState, level: &mut ServerLevel, pos: BlockPos) {
        if state.get(props::LIT) {
            level.set_block_and_update(pos, state.set(props::LIT, false));
        }
    }
}

register_block_class!("RedStoneOreBlock", |_| Box::new(RedStoneOreBlock));
