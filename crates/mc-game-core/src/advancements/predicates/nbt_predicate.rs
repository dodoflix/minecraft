//! `NbtPredicate.matches(Entity)` / `getEntityTagToCompare(entity)` (codec and `NbtUtils.compareNbt` matching live in
//! mc-protocol).

use crate::server::level::ServerLevel;
use crate::world::entity::Entity;
use mc_nbt::{CompoundTag, Tag};
use mc_protocol::components::NbtPredicate;

/// `NbtPredicate.SELECTED_ITEM_TAG`.
pub const SELECTED_ITEM_TAG: &str = "SelectedItem";

/// `NbtPredicate.getEntityTagToCompare(entity)`: `saveWithoutId` plus the selected item of a player.
pub fn get_entity_tag_to_compare(level: &ServerLevel, entity: &dyn Entity) -> CompoundTag {
    let mut out = CompoundTag::new();
    entity.save_without_id(&mut out, &|id| level.get_entity(id));
    if let Some(player) = entity.player() {
        let selected = player.inventory.get_selected_item();
        if !selected.is_empty()
            && let Ok(tag) = selected.to_tag().into_result()
        {
            out.put(SELECTED_ITEM_TAG, tag);
        }
    }
    out
}

pub trait NbtPredicateEntityExt {
    /// `matches(Entity)`.
    fn matches_entity(&self, level: &ServerLevel, entity: &dyn Entity) -> bool;
}

impl NbtPredicateEntityExt for NbtPredicate {
    fn matches_entity(&self, level: &ServerLevel, entity: &dyn Entity) -> bool {
        self.matches_tag(Some(&Tag::Compound(get_entity_tag_to_compare(
            level, entity,
        ))))
    }
}
