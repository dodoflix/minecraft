//! `net.minecraft.world.level.block.FlowerPotBlock`.
use mc_data::GameEvent;
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::entity::ServerPlayer;
use mc_game_core::world::inventory::Prediction;
use mc_game_core::world::item::{InteractionHand, InteractionResult, ItemStack, ItemStackExt};
use mc_game_core::world::level::block::impls::prelude::*;
use mc_game_core::world::level::gameevent::GameEventContext;
use mc_game_core::{register_block_class, update_shape};
use mc_protocol::packets::play::level::SoundSource;
use std::sync::OnceLock;

/// The pot's `potted` content (`Blocks.AIR` for the empty pot).
pub struct FlowerPotBlock {
    potted: Block,
}

/// `FlowerPotBlock.POTTED_BY_CONTENT`: `(content, pot)` of every flower pot block (`new FlowerPotBlock(potted, p)` in
/// `Blocks.java`: `potted_<x>` holds `<x>`, apart from the two azalea bushes).
fn potted_by_content() -> &'static [(Block, Block)] {
    static MAP: OnceLock<Vec<(Block, Block)>> = OnceLock::new();
    MAP.get_or_init(|| {
        Block::iter()
            .filter(|pot| pot.data().classes.first() == Some(&"FlowerPotBlock"))
            .map(|pot| (potted_content(pot), pot))
            .collect()
    })
}

fn potted_content(pot: Block) -> Block {
    match pot.data().name {
        "minecraft:flower_pot" => Block::AIR,
        "minecraft:potted_azalea_bush" => Block::AZALEA,
        "minecraft:potted_flowering_azalea_bush" => Block::FLOWERING_AZALEA,
        name => Block::by_name(&format!(
            "minecraft:{}",
            name.strip_prefix("minecraft:potted_")
                .expect("a potted block")
        ))
        .expect("the potted content exists"),
    }
}

impl FlowerPotBlock {
    fn is_empty(&self) -> bool {
        self.potted == Block::AIR
    }
}

impl BlockBehaviour for FlowerPotBlock {
    update_shape!(
        |self, state, level, pos, direction, neighbour_pos, neighbour_state| {
            if direction == Direction::Down && !state.can_survive(&*level, pos) {
                air()
            } else {
                state
            }
        }
    );
    fn use_item_on(
        &self,
        stack: &ItemStack,
        _state: BlockState,
        level: &mut ServerLevel,
        pos: BlockPos,
        player: &mut ServerPlayer,
        hand: InteractionHand,
        _hit: &mc_core::phys::BlockHitResult,
    ) -> InteractionResult {
        let content = if stack.is_empty() {
            None
        } else {
            stack.item().block()
        };
        let pot = content.and_then(|block| {
            potted_by_content()
                .iter()
                .find(|(c, _)| *c == block)
                .map(|(_, pot)| *pot)
        });
        let Some(new_contents) = pot.filter(|pot| *pot != Block::AIR) else {
            return InteractionResult::TRY_WITH_EMPTY_HAND;
        };
        if !self.is_empty() {
            return InteractionResult::CONSUME;
        }
        level.set_block_and_update(pos, new_contents.default_state());
        level.game_event_at_block(
            GameEvent::BLOCK_CHANGE,
            pos,
            &GameEventContext::of(Some(player.base_id()), None),
        );
        player.award_stat_once(&*level, mc_game_core::stats::stats::Stats::POT_FLOWER);
        let mut held = player.get_item_in_hand(hand);
        held.consume(1, player.player.abilities.instabuild);
        player.set_item_in_hand(hand, held);
        InteractionResult::SUCCESS
    }
    fn use_without_item(
        &self,
        _state: BlockState,
        level: &mut ServerLevel,
        pos: BlockPos,
        player: &mut ServerPlayer,
        _hit: &mc_core::phys::BlockHitResult,
    ) -> InteractionResult {
        if self.is_empty() {
            return InteractionResult::CONSUME;
        }
        let mut plant = ItemStack::new(self.potted.item().expect("a potted plant has an item"));
        if !player
            .player
            .inventory
            .add(&mut plant, player.player.abilities.instabuild)
        {
            player.drop(level, plant, false, Prediction::Predicted);
        }
        level.set_block_and_update(pos, Block::FLOWER_POT.default_state());
        level.game_event_at_block(
            GameEvent::BLOCK_CHANGE,
            pos,
            &GameEventContext::of(Some(player.base_id()), None),
        );
        InteractionResult::SUCCESS
    }
    fn get_clone_item_stack(
        &self,
        state: BlockState,
        _level: &ServerLevel,
        _pos: BlockPos,
        _include_data: bool,
    ) -> ItemStack {
        if self.is_empty() {
            state
                .block()
                .item()
                .map_or_else(ItemStack::empty, ItemStack::new)
        } else {
            ItemStack::new(self.potted.item().expect("a potted plant has an item"))
        }
    }
    fn random_tick(&self, state: BlockState, level: &mut ServerLevel, pos: BlockPos) {
        if state.is_block(Block::POTTED_OPEN_EYEBLOSSOM)
            || state.is_block(Block::POTTED_CLOSED_EYEBLOSSOM)
        {
            let is_open = self.potted == Block::OPEN_EYEBLOSSOM;
            let should_be_open = level
                .attribute_tri_state(
                    mc_game_core::world::attribute::attributes::EYEBLOSSOM_OPEN,
                    pos,
                )
                .to_boolean(is_open);
            if is_open != should_be_open {
                level.set_block_and_update(pos, opposite(state));
                let new_type = super::eyeblossom_block::transformed_type(is_open);
                new_type.spawn_transform_particle(level, pos);
                level.play_sound(
                    None,
                    pos,
                    new_type.long_switch_sound(),
                    SoundSource::Blocks,
                    1.0,
                    1.0,
                );
            }
        }
    }
}

/// `FlowerPotBlock.opposite(state)`.
pub fn opposite(state: BlockState) -> BlockState {
    if state.is_block(Block::POTTED_OPEN_EYEBLOSSOM) {
        Block::POTTED_CLOSED_EYEBLOSSOM.default_state()
    } else if state.is_block(Block::POTTED_CLOSED_EYEBLOSSOM) {
        Block::POTTED_OPEN_EYEBLOSSOM.default_state()
    } else {
        state
    }
}

register_block_class!("FlowerPotBlock", |block| Box::new(FlowerPotBlock {
    potted: potted_content(block)
}));
