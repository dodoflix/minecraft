//! `DebugStickItem`: left click selects a block property, right click cycles its value.

use super::{InteractionResult, ItemBehaviour, ItemStack, UseOnContext};
use crate::register_item_class;
use crate::server::level::ServerLevel;
use crate::world::level::{BlockGetter, LevelAccessor, LevelReader};
use crate::world::entity::ServerPlayer;
use mc_core::pos::BlockPos;
use mc_data::block::BlockState;
use mc_data::props::PropertyDef;
use mc_protocol::components::DEBUG_STICK_STATE;
use mc_text::Component;

pub struct DebugStickItem {
    description_id: &'static str,
}

/// `Util.findNextInIterable(collection, current)` (`current` = index, `None` = null).
fn find_next(len: usize, current: Option<usize>) -> usize {
    match current {
        Some(i) if i + 1 < len => i + 1,
        _ => 0,
    }
}

/// `Util.findPreviousInIterable(collection, current)`.
fn find_previous(len: usize, current: Option<usize>) -> usize {
    match current {
        Some(0) | None => len - 1,
        Some(i) => i - 1,
    }
}

fn get_relative(len: usize, current: Option<usize>, backward: bool) -> usize {
    if backward { find_previous(len, current) } else { find_next(len, current) }
}

/// `getNameHelper(state, property)`: the serialized value name.
fn name_helper(state: BlockState, property: &PropertyDef) -> String {
    state.get_property_str(property.name).expect("property of the state's block").to_owned()
}

impl DebugStickItem {
    fn message(player: &ServerPlayer, message: Component) {
        player.send_overlay_message(message);
    }

    /// `handleInteraction(player, state, level, pos, cycle, itemStackInHand)`.
    fn handle_interaction(&self, player: &mut ServerPlayer, state: BlockState, level: &mut ServerLevel, pos: BlockPos, cycle: bool, stack: &mut ItemStack) -> bool {
        if !player.can_use_game_master_blocks() {
            return false;
        }
        let block = state.block();
        let properties: Vec<&'static PropertyDef> = block.properties().collect();
        if properties.is_empty() {
            Self::message(player, Component::translatable(format!("{}.empty", self.description_id), vec![mc_text::TranslatableArg::Component(Component::literal(block.data().name))]));
            return false;
        }
        let Some(debug_stick_state) = stack.get(&DEBUG_STICK_STATE).cloned() else { return false };
        let selected = debug_stick_state.properties.iter().find(|(b, _)| *b == block).and_then(|(_, name)| properties.iter().position(|p| p.name == name));
        let secondary = crate::world::entity::Entity::is_shift_key_down(&*player);
        if cycle {
            let index = selected.unwrap_or(0);
            let property = properties[index];
            let current = state.values().find(|(d, _)| d.name == property.name).map(|(_, i)| i);
            let new_index = get_relative(property.values.len(), current, secondary);
            let new_state = state.with_property_str(property.name, property.values[new_index]).expect("value of the property");
            level.set_block_flags(pos, new_state, 18);
            Self::message(player, Component::translatable(format!("{}.update", self.description_id), vec![mc_text::TranslatableArg::Component(Component::literal(property.name)), mc_text::TranslatableArg::Component(Component::literal(name_helper(new_state, property)))]));
        } else {
            let index = get_relative(properties.len(), selected, secondary);
            let property = properties[index];
            let mut new_state = debug_stick_state;
            match new_state.properties.iter_mut().find(|(b, _)| *b == block) {
                Some(entry) => entry.1 = property.name.to_owned(),
                None => new_state.properties.push((block, property.name.to_owned())),
            }
            stack.set(DEBUG_STICK_STATE, new_state);
            Self::message(player, Component::translatable(format!("{}.select", self.description_id), vec![mc_text::TranslatableArg::Component(Component::literal(property.name)), mc_text::TranslatableArg::Component(Component::literal(name_helper(state, property)))]));
        }
        true
    }
}

impl ItemBehaviour for DebugStickItem {
    fn can_destroy_block(&self, stack: &mut ItemStack, state: BlockState, level: &mut ServerLevel, pos: BlockPos, user: &mut ServerPlayer) -> bool {
        self.handle_interaction(user, state, level, pos, false, stack);
        false
    }

    fn use_on(&self, context: &mut UseOnContext, level: &mut ServerLevel, player: &mut ServerPlayer) -> InteractionResult {
        let pos = context.get_clicked_pos();
        let state = level.get_block_state(pos);
        if !self.handle_interaction(player, state, level, pos, true, &mut context.item_stack) {
            return InteractionResult::FAIL;
        }
        InteractionResult::SUCCESS
    }
}

register_item_class!("DebugStickItem", |item| Box::new(DebugStickItem { description_id: item.data().description_id }));
