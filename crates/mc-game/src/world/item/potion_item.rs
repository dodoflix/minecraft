//! `PotionItem` / `TippedArrowItem`: only the name of the potion contents (`getName`) so far.

use super::{ItemBehaviour, ItemStack, PlainItem};
use crate::register_item_class;
use mc_protocol::components::POTION_CONTENTS;
use mc_text::Component;

/// `PotionContents.getName(descriptionId + ".effect.")` when the stack has `potion_contents`, else `Item.getName`.
pub struct PotionNamedItem {
    description_id: &'static str,
}

impl ItemBehaviour for PotionNamedItem {
    fn get_name(&self, stack: &ItemStack) -> Component {
        match stack.get(&POTION_CONTENTS) {
            Some(contents) => {
                Component::translatable(crate::world::effect::potion_contents::get_name_key(contents, &format!("{}.effect.", self.description_id)), Vec::new())
            }
            None => PlainItem.get_name(stack),
        }
    }
}

register_item_class!("PotionItem", |item| Box::new(PotionNamedItem { description_id: item.data().description_id }));
register_item_class!("TippedArrowItem", |item| Box::new(PotionNamedItem { description_id: item.data().description_id }));
