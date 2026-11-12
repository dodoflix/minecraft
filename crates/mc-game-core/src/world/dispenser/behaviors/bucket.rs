//! The bucket behaviors of `DispenseItemBehavior.bootStrap`: `filledBucketBehavior` (every `DispensibleContainerItem`
//! bucket) and `Items.BUCKET` (`BucketPickup`).

use crate::world::dispenser::{
    BlockSource, DispenseTemplate, Templated, consume_with_remainder, default_behavior,
    default_execute,
};
use crate::world::item::ItemStack;
use crate::world::item::bucket_item::{check_extra_content, empty_contents};
use crate::world::level::BlockGetter;
use crate::world::level::block::behaviour as block_behaviour;
use crate::world::level::gameevent::GameEventContext;
use mc_data::{GameEvent, Item};

/// `filledBucketBehavior`: empties the bucket in front, else drops it through a second default behavior (which is why the
/// default click and smoke fire twice).
struct FilledBucketDispense;

impl DispenseTemplate for FilledBucketDispense {
    fn execute(&self, source: &mut BlockSource<'_>, dispensed: ItemStack) -> ItemStack {
        let target = source.pos.relative(source.facing());
        let item = dispensed.item();
        if empty_contents(item, None, source.level, target, None) == Some(true) {
            check_extra_content(item, None, source.level, &dispensed, target);
            return consume_with_remainder(source, dispensed, ItemStack::new(Item::BUCKET));
        }
        default_behavior().dispense(source, dispensed)
    }
}

/// `Items.BUCKET`: picks up the `BucketPickup` block in front, else drops the bucket.
struct EmptyBucketDispense;

impl DispenseTemplate for EmptyBucketDispense {
    fn execute(&self, source: &mut BlockSource<'_>, dispensed: ItemStack) -> ItemStack {
        let target = source.pos.relative(source.facing());
        let level = &mut *source.level;
        let state = level.get_block_state(target);
        let block = block_behaviour(state.block());
        if !block.is_bucket_pickup() {
            return default_execute(source, dispensed);
        }
        let pickup = block.pickup_block(None, level, target, state);
        if pickup.is_empty() {
            return default_execute(source, dispensed);
        }
        level.game_event_at_block(
            GameEvent::FLUID_PICKUP,
            target,
            &GameEventContext::of_entity(None),
        );
        let filled = ItemStack::new(pickup.item());
        consume_with_remainder(source, dispensed, filled)
    }
}

crate::register_dispense_behavior!(
    &[
        "minecraft:lava_bucket",
        "minecraft:water_bucket",
        "minecraft:powder_snow_bucket",
        "minecraft:salmon_bucket",
        "minecraft:cod_bucket",
        "minecraft:pufferfish_bucket",
        "minecraft:tropical_fish_bucket",
        "minecraft:axolotl_bucket",
        "minecraft:sulfur_cube_bucket",
        "minecraft:tadpole_bucket",
    ],
    |_| Box::new(Templated(FilledBucketDispense))
);
crate::register_dispense_behavior!(&["minecraft:bucket"], |_| Box::new(Templated(
    EmptyBucketDispense
)));
