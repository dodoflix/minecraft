//! `StartAdmiringItemIfSeen`.

use mc_game_core::world::entity::ai::behavior::{BehaviorBuilder, OneShot};
use mc_game_core::world::entity::ai::memory::memory_module_type::{
    ADMIRING_DISABLED, ADMIRING_ITEM, DISABLE_WALK_TO_ADMIRE_ITEM, NEAREST_VISIBLE_WANTED_ITEM,
};
use mc_game_core::world::entity::ai::sensing::repellent_sensors::is_loved_item;
use mc_game_core::world::entity::item::ItemEntity;

pub fn create(admire_duration: i32) -> OneShot {
    let mut i = BehaviorBuilder::instance();
    let nearest_item = i.present(&NEAREST_VISIBLE_WANTED_ITEM);
    let admiring = i.absent(&ADMIRING_ITEM);
    let _admiring_disabled = i.absent(&ADMIRING_DISABLED);
    let _walk_disabled = i.absent(&DISABLE_WALK_TO_ADMIRE_ITEM);
    i.create(move |c| {
        let item = c.get(&nearest_item).0;
        let loved = c
            .level
            .get_entity_or_recently_removed(item)
            .and_then(|e| e.as_any().downcast_ref::<ItemEntity>())
            .is_some_and(|e| is_loved_item(&e.get_item()));
        if !loved {
            false
        } else {
            c.set_with_expiry(&admiring, true, i64::from(admire_duration));
            true
        }
    })
}
