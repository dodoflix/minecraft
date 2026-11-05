//! `net.minecraft.core.cauldron.CauldronInteractions` / `CauldronInteraction.Dispatcher`: what a right-click with an
//! item does to a cauldron of each kind (`EMPTY`, `WATER`, `LAVA`, `POWDER_SNOW`).
use super::layered_cauldron_block::lower_fill_level;
use super::prelude::*;
use crate::server::level::ServerLevel;
use crate::stats::stat::Stat;
use crate::stats::stats::Stats;
use crate::world::entity::ServerPlayer;
use crate::world::entity::player::menu_ctx::PlayerCtx;
use crate::world::item::{InteractionHand, InteractionResult, ItemStack, create_filled_result};
use crate::world::level::gameevent::GameEventContext;
use mc_data::{GameEvent, Item, SoundEvent};
use mc_protocol::components::{BANNER_PATTERNS, POTION_CONTENTS};
use mc_protocol::packets::play::level::SoundSource;

/// The `CauldronInteractions` dispatcher a cauldron block was constructed with.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CauldronKind {
    Empty,
    Water,
    Lava,
    PowderSnow,
}

/// `Dispatcher.get(itemStack).interact(state, level, pos, player, hand, itemInHand)`; the held stack is a copy, every
/// change of it is written back with `setItemInHand`.
pub fn interact(kind: CauldronKind, state: BlockState, level: &mut ServerLevel, pos: BlockPos, player: &mut ServerPlayer, hand: InteractionHand, stack: &ItemStack) -> InteractionResult {
    let item = stack.item();
    // the tag entries come first (`Dispatcher.get` iterates `tags` before the item map)
    if kind == CauldronKind::Water && mc_data::tags::get::<Item>("cauldron_can_remove_dye").is_some_and(|tag| tag.contains(item)) {
        return dyed_item_iteration(state, level, pos, player, hand, stack);
    }
    match (kind, item) {
        (_, Item::LAVA_BUCKET) => fill_lava_interaction(level, pos, player, hand, stack),
        (_, Item::WATER_BUCKET) => fill_water_interaction(level, pos, player, hand, stack),
        (_, Item::POWDER_SNOW_BUCKET) => fill_powder_snow_interaction(level, pos, player, hand, stack),
        (CauldronKind::Empty, Item::POTION) => {
            if is_water_potion(stack) {
                use_potion_into_cauldron(Block::WATER_CAULDRON.default_state(), level, pos, player, hand, stack);
                InteractionResult::SUCCESS
            } else {
                InteractionResult::TRY_WITH_EMPTY_HAND
            }
        }
        (CauldronKind::Water, Item::BUCKET) => fill_bucket(state, level, pos, player, hand, stack, ItemStack::new(Item::WATER_BUCKET), |s| s.get(props::LEVEL_CAULDRON) == 3, SoundEvent::ITEM_BUCKET_FILL),
        (CauldronKind::Water, Item::GLASS_BOTTLE) => {
            let filled = water_bottle();
            let new_stack = create_filled_result(stack.clone(), &mut PlayerCtx { player: &mut *player, level: &mut *level }, filled, true);
            player.set_item_in_hand(hand, new_stack);
            player.award_stat_once(level, Stats::USE_CAULDRON);
            player.award_stat_once(level, Stat::Used(item));
            lower_fill_level(state, level, pos);
            level.play_sound(None, pos, SoundEvent::ITEM_BOTTLE_FILL, SoundSource::Blocks, 1.0, 1.0);
            level.game_event_at_block(GameEvent::FLUID_PICKUP, pos, &GameEventContext::default());
            InteractionResult::SUCCESS
        }
        (CauldronKind::Water, Item::POTION) => {
            if state.get(props::LEVEL_CAULDRON) == 3 {
                InteractionResult::TRY_WITH_EMPTY_HAND
            } else if is_water_potion(stack) {
                use_potion_into_cauldron(state.cycle(props::LEVEL_CAULDRON), level, pos, player, hand, stack);
                InteractionResult::SUCCESS
            } else {
                InteractionResult::TRY_WITH_EMPTY_HAND
            }
        }
        (CauldronKind::Water, i) if is_banner(i) => banner_interaction(state, level, pos, player, hand, stack),
        (CauldronKind::Water, i) if is_dyed_shulker_box(i) => shulker_box_interaction(state, level, pos, player, hand, stack),
        (CauldronKind::Lava, Item::BUCKET) => fill_bucket(state, level, pos, player, hand, stack, ItemStack::new(Item::LAVA_BUCKET), |_| true, SoundEvent::ITEM_BUCKET_FILL_LAVA),
        (CauldronKind::PowderSnow, Item::BUCKET) => {
            fill_bucket(state, level, pos, player, hand, stack, ItemStack::new(Item::POWDER_SNOW_BUCKET), |s| s.get(props::LEVEL_CAULDRON) == 3, SoundEvent::ITEM_BUCKET_FILL_POWDER_SNOW)
        }
        _ => InteractionResult::TRY_WITH_EMPTY_HAND,
    }
}

/// `Items.BANNER` (the 16 dyed banners).
fn is_banner(item: Item) -> bool {
    let name = item.data().name;
    name.ends_with("_banner") && name != "minecraft:ominous_banner"
}

/// `Items.DYED_SHULKER_BOX`.
fn is_dyed_shulker_box(item: Item) -> bool {
    let name = item.data().name;
    name.ends_with("_shulker_box") && name != "minecraft:shulker_box"
}

/// `potion != null && potion.is(Potions.WATER)`.
fn is_water_potion(stack: &ItemStack) -> bool {
    stack.get(&POTION_CONTENTS).is_some_and(|contents| contents.potion == Some(mc_data::Potion::WATER))
}

/// `PotionContents.createItemStack(Items.POTION, Potions.WATER)`.
fn water_bottle() -> ItemStack {
    let mut stack = ItemStack::new(Item::POTION);
    stack.set(POTION_CONTENTS, mc_protocol::components::PotionContents { potion: Some(mc_data::Potion::WATER), custom_color: None, custom_effects: Vec::new(), custom_name: None });
    stack
}

/// The shared body of both water-potion interactions: the bottle empties into the cauldron.
fn use_potion_into_cauldron(new_state: BlockState, level: &mut ServerLevel, pos: BlockPos, player: &mut ServerPlayer, hand: InteractionHand, stack: &ItemStack) {
    let used_item = stack.item();
    let new_stack = create_filled_result(stack.clone(), &mut PlayerCtx { player: &mut *player, level: &mut *level }, ItemStack::new(Item::GLASS_BOTTLE), true);
    player.set_item_in_hand(hand, new_stack);
    player.award_stat_once(level, Stats::USE_CAULDRON);
    player.award_stat_once(level, Stat::Used(used_item));
    level.set_block_and_update(pos, new_state);
    level.play_sound(None, pos, SoundEvent::ITEM_BOTTLE_EMPTY, SoundSource::Blocks, 1.0, 1.0);
    level.game_event_at_block(GameEvent::FLUID_PLACE, pos, &GameEventContext::default());
}

/// `CauldronInteractions.fillBucket`.
#[allow(clippy::too_many_arguments)]
pub fn fill_bucket(
    state: BlockState,
    level: &mut ServerLevel,
    pos: BlockPos,
    player: &mut ServerPlayer,
    hand: InteractionHand,
    item_in_hand: &ItemStack,
    new_item: ItemStack,
    can_fill: impl Fn(BlockState) -> bool,
    sound: SoundEvent,
) -> InteractionResult {
    if !can_fill(state) {
        return InteractionResult::TRY_WITH_EMPTY_HAND;
    }
    let item_used = item_in_hand.item();
    let new_stack = create_filled_result(item_in_hand.clone(), &mut PlayerCtx { player: &mut *player, level: &mut *level }, new_item, true);
    player.set_item_in_hand(hand, new_stack);
    player.award_stat_once(level, Stats::USE_CAULDRON);
    player.award_stat_once(level, Stat::Used(item_used));
    level.set_block_and_update(pos, Block::CAULDRON.default_state());
    level.play_sound(None, pos, sound, SoundSource::Blocks, 1.0, 1.0);
    level.game_event_at_block(GameEvent::FLUID_PICKUP, pos, &GameEventContext::default());
    InteractionResult::SUCCESS
}

/// `CauldronInteractions.emptyBucket`.
pub fn empty_bucket(level: &mut ServerLevel, pos: BlockPos, player: &mut ServerPlayer, hand: InteractionHand, item_in_hand: &ItemStack, new_state: BlockState, sound: SoundEvent) -> InteractionResult {
    let item_used = item_in_hand.item();
    let new_stack = create_filled_result(item_in_hand.clone(), &mut PlayerCtx { player: &mut *player, level: &mut *level }, ItemStack::new(Item::BUCKET), true);
    player.set_item_in_hand(hand, new_stack);
    player.award_stat_once(level, Stats::FILL_CAULDRON);
    player.award_stat_once(level, Stat::Used(item_used));
    level.set_block_and_update(pos, new_state);
    level.play_sound(None, pos, sound, SoundSource::Blocks, 1.0, 1.0);
    level.game_event_at_block(GameEvent::FLUID_PLACE, pos, &GameEventContext::default());
    InteractionResult::SUCCESS
}

fn fill_water_interaction(level: &mut ServerLevel, pos: BlockPos, player: &mut ServerPlayer, hand: InteractionHand, stack: &ItemStack) -> InteractionResult {
    empty_bucket(level, pos, player, hand, stack, Block::WATER_CAULDRON.default_state().set(props::LEVEL_CAULDRON, 3), SoundEvent::ITEM_BUCKET_EMPTY)
}

fn fill_lava_interaction(level: &mut ServerLevel, pos: BlockPos, player: &mut ServerPlayer, hand: InteractionHand, stack: &ItemStack) -> InteractionResult {
    if is_under_water(level, pos) {
        InteractionResult::CONSUME
    } else {
        empty_bucket(level, pos, player, hand, stack, Block::LAVA_CAULDRON.default_state(), SoundEvent::ITEM_BUCKET_EMPTY_LAVA)
    }
}

fn fill_powder_snow_interaction(level: &mut ServerLevel, pos: BlockPos, player: &mut ServerPlayer, hand: InteractionHand, stack: &ItemStack) -> InteractionResult {
    if is_under_water(level, pos) {
        InteractionResult::CONSUME
    } else {
        empty_bucket(level, pos, player, hand, stack, Block::POWDER_SNOW_CAULDRON.default_state().set(props::LEVEL_CAULDRON, 3), SoundEvent::ITEM_BUCKET_EMPTY_POWDER_SNOW)
    }
}

fn shulker_box_interaction(state: BlockState, level: &mut ServerLevel, pos: BlockPos, player: &mut ServerPlayer, hand: InteractionHand, stack: &ItemStack) -> InteractionResult {
    // `itemInHand.transmuteCopy(Blocks.SHULKER_BOX, 1)`
    let cleaned = ItemStack::with_patch(Item::SHULKER_BOX, 1, &stack.get_components_patch());
    let new_stack = create_filled_result(stack.clone(), &mut PlayerCtx { player: &mut *player, level: &mut *level }, cleaned, false);
    player.set_item_in_hand(hand, new_stack);
    player.award_stat_once(level, Stats::CLEAN_SHULKER_BOX);
    lower_fill_level(state, level, pos);
    InteractionResult::SUCCESS
}

fn banner_interaction(state: BlockState, level: &mut ServerLevel, pos: BlockPos, player: &mut ServerPlayer, hand: InteractionHand, stack: &ItemStack) -> InteractionResult {
    let mut layers = stack.get(&BANNER_PATTERNS).cloned().unwrap_or(mc_protocol::components::BannerPatternLayers { layers: Vec::new() });
    if layers.layers.is_empty() {
        return InteractionResult::TRY_WITH_EMPTY_HAND;
    }
    let mut cleaned = stack.copy_with_count(1);
    layers.layers.pop();
    cleaned.set(BANNER_PATTERNS, layers);
    let new_stack = create_filled_result(stack.clone(), &mut PlayerCtx { player: &mut *player, level: &mut *level }, cleaned, false);
    player.set_item_in_hand(hand, new_stack);
    player.award_stat_once(level, Stats::CLEAN_BANNER);
    lower_fill_level(state, level, pos);
    InteractionResult::SUCCESS
}

fn dyed_item_iteration(state: BlockState, level: &mut ServerLevel, pos: BlockPos, player: &mut ServerPlayer, hand: InteractionHand, stack: &ItemStack) -> InteractionResult {
    if !stack.has(mc_data::DataComponentType::DYED_COLOR) {
        return InteractionResult::TRY_WITH_EMPTY_HAND;
    }
    let mut cleaned = stack.clone();
    cleaned.remove(mc_data::DataComponentType::DYED_COLOR);
    player.set_item_in_hand(hand, cleaned);
    player.award_stat_once(level, Stats::CLEAN_ARMOR);
    lower_fill_level(state, level, pos);
    InteractionResult::SUCCESS
}

/// `isUnderWater(level, pos)`.
fn is_under_water(level: &ServerLevel, pos: BlockPos) -> bool {
    level.get_fluid_state(pos.above()).is_tag("water")
}
