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
                let suffix = contents.custom_name.clone().or_else(|| contents.potion.map(|p| p.data().base_name.to_owned())).unwrap_or_else(|| "empty".to_owned());
                Component::translatable(format!("{}.effect.{suffix}", self.description_id), Vec::new())
            }
            None => PlainItem.get_name(stack),
        }
    }
}

register_item_class!("PotionItem", |item| Box::new(PotionNamedItem { description_id: item.data().description_id }));
register_item_class!("TippedArrowItem", |item| Box::new(PotionNamedItem { description_id: item.data().description_id }));
