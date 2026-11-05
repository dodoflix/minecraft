//! `KnowledgeBookItem`: `use` awards the recipes of the `recipes` component and consumes the book.

use super::{InteractionHand, InteractionResult, ItemBehaviour, ItemStackExt};
use crate::register_item_class;
use crate::server::level::ServerLevel;
use crate::world::entity::ServerPlayer;
use mc_protocol::components::RECIPES;
use mc_protocol::packets::play::Stat;

pub struct KnowledgeBookItem;

impl ItemBehaviour for KnowledgeBookItem {
    fn use_item(&self, level: &mut ServerLevel, player: &mut ServerPlayer, hand: InteractionHand) -> InteractionResult {
        let mut stack = player.get_item_in_hand(hand);
        let recipe_ids = stack.get(&RECIPES).cloned().unwrap_or_default();
        stack.consume(1, player.player.abilities.instabuild);
        let item = player.get_item_in_hand(hand).item();
        player.set_item_in_hand(hand, stack);
        if recipe_ids.is_empty() {
            return InteractionResult::FAIL;
        }
        let manager = level.recipe_access();
        let mut recipes = Vec::with_capacity(recipe_ids.len());
        for recipe_id in &recipe_ids {
            match manager.by_key(recipe_id) {
                Some(recipe) => recipes.push(recipe.clone()),
                None => {
                    log::error!("Invalid recipe: {recipe_id}");
                    return InteractionResult::FAIL;
                }
            }
        }
        player.award_recipes(level, &recipes);
        player.award_stat_once(level, Stat::Used(item));
        InteractionResult::SUCCESS
    }
}

register_item_class!("KnowledgeBookItem", |_| Box::new(KnowledgeBookItem));
