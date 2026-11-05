//! Every item's default components resolve with the *default* (vanilla data pack) registry context.
use mc_protocol::components::ItemStack;

#[test]
fn every_item_can_be_created_with_default_registry_context() {
    for item in mc_data::Item::iter() {
        let s = ItemStack::new(item);
        assert_eq!(s.count(), if item == mc_data::Item::AIR { 0 } else { 1 });
    }
}
