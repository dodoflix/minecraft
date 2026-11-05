//! Helpers on top of `mc_protocol::components::ItemStack` that the menu/inventory/item code needs
//! (`ItemStack` methods that depend on game types, or that the value type does not carry).

use super::enchantment_helper;
use mc_nbt::{CompoundTag, Tag};
use mc_protocol::components::{ENCHANTMENTS, ItemStack};
use mc_protocol::components::{CUSTOM_NAME, DataComponentPatch, RARITY, Rarity, WRITTEN_BOOK_CONTENT};
use mc_text::{ChatFormatting, Component, HoverEvent, ItemStackTemplate};

pub trait ItemStackExt {
    /// `isEnchanted()`.
    fn is_enchanted(&self) -> bool;
    /// `ItemStack.MAP_CODEC` fields (`id`, `count`, `components`) merged into `tag`.
    fn save_into(&self, tag: &mut CompoundTag);
    /// `consume(amount, owner)`: shrink unless the owner has infinite materials.
    fn consume(&mut self, amount: i32, owner_has_infinite_materials: bool);
    /// `consumeAndReturn(amount, owner)`.
    fn consume_and_return(&mut self, amount: i32, owner_has_infinite_materials: bool) -> ItemStack;
    /// `isBroken()`.
    fn is_broken(&self) -> bool;
    /// `nextDamageWillBreak()`.
    fn next_damage_will_break(&self) -> bool;
    /// `EnchantmentHelper.has(stack, effectComponent)` (e.g. `minecraft:prevent_armor_change`).
    fn has_enchantment_effect(&self, effect_component: &str) -> bool;
    /// `getCustomName()`: `custom_name`, else a non-blank written book title.
    fn get_custom_name(&self) -> Option<Component>;
    /// `getHoverName()`.
    fn get_hover_name(&self) -> Component;
    /// `getRarity()`.
    fn get_rarity(&self) -> Rarity;
    /// `canBeHurtBy(source)`: false when the `damage_resistant` component covers the damage type of `source`.
    fn can_be_hurt_by(&self, level: &crate::server::level::ServerLevel, source: &crate::world::damagesource::DamageSource) -> bool;
    /// `getDisplayName()`: the bracketed, rarity-coloured name with the `show_item` hover.
    fn get_display_name(&self) -> Component;
}

impl ItemStackExt for ItemStack {
    fn is_enchanted(&self) -> bool {
        self.get(&ENCHANTMENTS).is_some_and(|e| !e.is_empty())
    }

    fn can_be_hurt_by(&self, level: &crate::server::level::ServerLevel, source: &crate::world::damagesource::DamageSource) -> bool {
        use mc_protocol::components::{DAMAGE_RESISTANT, HolderSet};
        let Some(resistant) = self.get(&DAMAGE_RESISTANT) else { return true };
        let resistant_to = match &resistant.types {
            HolderSet::Named(tag) => level.registries.is_in_tag(crate::registry::keys::DAMAGE_TYPE, &source.damage_type, tag),
            HolderSet::Direct(holders) => holders.iter().any(|h| h.name() == Some(&source.damage_type)),
        };
        !resistant_to
    }

    fn get_custom_name(&self) -> Option<Component> {
        if let Some(name) = self.get(&CUSTOM_NAME) {
            return Some(name.clone());
        }
        let title = &self.get(&WRITTEN_BOOK_CONTENT)?.title.raw;
        // `StringUtil.isBlank`: `Character.isWhitespace || Character.isSpaceChar` (Rust's White_Space differs at U+0085 and U+001C..1F).
        let whitespace = |c: char| (c.is_whitespace() && c != '\u{85}') || ('\u{1c}'..='\u{1f}').contains(&c);
        (!title.chars().all(whitespace)).then(|| Component::literal(title.clone()))
    }

    fn get_hover_name(&self) -> Component {
        self.get_custom_name().unwrap_or_else(|| super::behaviour(self.item()).get_name(self))
    }

    fn get_rarity(&self) -> Rarity {
        let base = self.get(&RARITY).copied().unwrap_or(Rarity::Common);
        if !self.is_enchanted() {
            return base;
        }
        match base {
            Rarity::Common | Rarity::Uncommon => Rarity::Rare,
            Rarity::Rare => Rarity::Epic,
            other => other,
        }
    }

    fn get_display_name(&self) -> Component {
        let mut hover_name = Component::empty().append(self.get_hover_name());
        if self.get(&CUSTOM_NAME).is_some() {
            hover_name = hover_name.with_style_fn(|s| s.with_italic(Some(true)));
        }
        let result = mc_text::util::wrap_in_square_brackets(hover_name);
        if self.is_empty() {
            return result;
        }
        let color = match self.get_rarity() {
            Rarity::Common => ChatFormatting::White,
            Rarity::Uncommon => ChatFormatting::Yellow,
            Rarity::Rare => ChatFormatting::Aqua,
            Rarity::Epic => ChatFormatting::LightPurple,
        };
        let patch = self.components().map_or_else(DataComponentPatch::empty, |c| c.as_patch());
        let components = if patch.is_empty() { None } else { mc_dfu::encode_nbt(&DataComponentPatch::codec(), &patch).into_result().ok() };
        let hover = HoverEvent::ShowItem(ItemStackTemplate { id: self.item().identifier(), count: self.count(), components });
        result.with_style_fn(|s| s.with_color_formatting(Some(color)).with_hover_event(Some(hover)))
    }

    fn save_into(&self, tag: &mut CompoundTag) {
        if let Ok(Tag::Compound(c)) = self.to_tag().into_result() {
            tag.merge(&c);
        }
    }

    fn consume(&mut self, amount: i32, owner_has_infinite_materials: bool) {
        if !owner_has_infinite_materials {
            self.shrink(amount);
        }
    }

    fn consume_and_return(&mut self, amount: i32, owner_has_infinite_materials: bool) -> ItemStack {
        let split = self.copy_with_count(amount);
        self.consume(amount, owner_has_infinite_materials);
        split
    }

    fn is_broken(&self) -> bool {
        self.is_damageable_item() && self.get_damage_value() >= self.get_max_damage()
    }

    fn next_damage_will_break(&self) -> bool {
        self.is_damageable_item() && self.get_damage_value() >= self.get_max_damage() - 1
    }

    fn has_enchantment_effect(&self, effect_component: &str) -> bool {
        enchantment_helper::has(self, effect_component)
    }
}

/// Decodes the `MAP_CODEC` fields of `tag` (`None` = undecodable entry: unknown item, count outside 1..=99, ...).
pub fn load_item_stack(tag: &CompoundTag) -> Option<ItemStack> {
    ItemStack::from_tag(&Tag::Compound(tag.clone())).into_result().ok()
}
