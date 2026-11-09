//! `net.minecraft.world.level.block.PumpkinBlock`: shears carve it.
use mc_core::phys::BlockHitResult;
use mc_data::{GameEvent, Item, SoundEvent};
use mc_game_core::register_block_class;
use mc_game_core::server::level::ServerLevel;
use mc_game_core::stats::Stat;
use mc_game_core::world::entity::item::ItemEntity;
use mc_game_core::world::entity::{Entity, ServerPlayer};
use mc_game_core::world::item::{InteractionHand, InteractionResult, ItemStack, hurt_and_break};
use mc_game_core::world::level::LevelAccessor;
use mc_game_core::world::level::block::impls::prelude::*;
use mc_game_core::world::level::gameevent::GameEventContext;
use mc_game_core::world::level::storage::loot::block_drops::block_interact_drops;
use mc_protocol::packets::play::level::SoundSource;

pub struct PumpkinBlock;

impl BlockBehaviour for PumpkinBlock {
    fn use_item_on(
        &self,
        stack: &ItemStack,
        state: BlockState,
        level: &mut ServerLevel,
        pos: BlockPos,
        player: &mut ServerPlayer,
        hand: InteractionHand,
        hit: &BlockHitResult,
    ) -> InteractionResult {
        if !stack.is(Item::SHEARS) {
            return InteractionResult::TRY_WITH_EMPTY_HAND;
        }
        let clicked_direction = hit.direction;
        let direction = if clicked_direction.get_axis() == Axis::Y {
            player.get_direction().get_opposite()
        } else {
            clicked_direction
        };
        let (step_x, step_z) = (
            f64::from(direction.get_step_x()),
            f64::from(direction.get_step_z()),
        );
        let drops = block_interact_drops(
            level,
            "carve/pumpkin",
            pos,
            state,
            Some(stack),
            Some(&*player),
        );
        for pumpkin_seeds in drops {
            let mut entity = ItemEntity::with_stack(
                level,
                f64::from(pos.x()) + 0.5 + step_x * 0.65,
                f64::from(pos.y()) + 0.1,
                f64::from(pos.z()) + 0.5 + step_z * 0.65,
                pumpkin_seeds,
            );
            let random = level.random();
            let (dx, dz) = (random.next_double() * 0.02, random.next_double() * 0.02);
            entity
                .base_mut()
                .set_delta_movement_xyz(0.05 * step_x + dx, 0.05, 0.05 * step_z + dz);
            level.add_fresh_entity(Box::new(entity));
        }
        level.play_sound(
            None,
            pos,
            SoundEvent::BLOCK_PUMPKIN_CARVE,
            SoundSource::Blocks,
            1.0,
            1.0,
        );
        level.set_block_flags(
            pos,
            Block::CARVED_PUMPKIN
                .default_state()
                .set(props::HORIZONTAL_FACING, direction),
            11,
        );
        let mut used = stack.clone();
        hurt_and_break(&mut used, 1, level, player, hand.as_equipment_slot());
        player.set_item_in_hand(hand, used);
        level.game_event_at_block(
            GameEvent::SHEAR,
            pos,
            &GameEventContext::of_entity(Some(player.id())),
        );
        player.award_stat_once(level, Stat::Used(Item::SHEARS));
        InteractionResult::SUCCESS
    }
}

register_block_class!("PumpkinBlock", |_| Box::new(PumpkinBlock));
