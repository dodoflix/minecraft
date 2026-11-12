//! Guard of the dispenser registry (hermetic).
//!
//! `DispenseItemBehavior.bootStrap()` registers behaviours for 82 items and `DispenserBlock.getDefaultDispenseMethod` picks
//! one of three more by item component. An item without a registration is silently dropped like any plain item, so every
//! vanilla entry is either registered here or listed below, one item per line, with the behaviour that is missing. The
//! entry goes away with the commit that registers the item.
use mc_data::{DataComponentType, Item};
use mc_game::world::dispenser::{
    DISPENSE_BEHAVIORS, default_behavior, get_default_dispense_method,
};
use mc_game::world::item::ItemStack;
use std::collections::BTreeSet;

/// The items of `DispenseItemBehavior.bootStrap()` (`registerBehavior` / `registerProjectileBehavior`, the shulker boxes
/// through `Items.DYED_SHULKER_BOX`).
const VANILLA: &[&str] = &[
    "minecraft:arrow",
    "minecraft:tipped_arrow",
    "minecraft:spectral_arrow",
    "minecraft:egg",
    "minecraft:blue_egg",
    "minecraft:brown_egg",
    "minecraft:snowball",
    "minecraft:experience_bottle",
    "minecraft:splash_potion",
    "minecraft:lingering_potion",
    "minecraft:firework_rocket",
    "minecraft:fire_charge",
    "minecraft:wind_charge",
    "minecraft:armor_stand",
    "minecraft:chest",
    "minecraft:oak_boat",
    "minecraft:spruce_boat",
    "minecraft:birch_boat",
    "minecraft:jungle_boat",
    "minecraft:dark_oak_boat",
    "minecraft:acacia_boat",
    "minecraft:cherry_boat",
    "minecraft:mangrove_boat",
    "minecraft:poplar_boat",
    "minecraft:pale_oak_boat",
    "minecraft:bamboo_raft",
    "minecraft:oak_chest_boat",
    "minecraft:spruce_chest_boat",
    "minecraft:birch_chest_boat",
    "minecraft:jungle_chest_boat",
    "minecraft:dark_oak_chest_boat",
    "minecraft:acacia_chest_boat",
    "minecraft:cherry_chest_boat",
    "minecraft:mangrove_chest_boat",
    "minecraft:poplar_chest_boat",
    "minecraft:pale_oak_chest_boat",
    "minecraft:bamboo_chest_raft",
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
    "minecraft:bucket",
    "minecraft:flint_and_steel",
    "minecraft:bone_meal",
    "minecraft:tnt",
    "minecraft:wither_skeleton_skull",
    "minecraft:carved_pumpkin",
    "minecraft:shulker_box",
    "minecraft:glass_bottle",
    "minecraft:glowstone",
    "minecraft:shears",
    "minecraft:brush",
    "minecraft:honeycomb",
    "minecraft:potion",
    "minecraft:minecart",
    "minecraft:chest_minecart",
    "minecraft:furnace_minecart",
    "minecraft:tnt_minecart",
    "minecraft:hopper_minecart",
    "minecraft:command_block_minecart",
    "minecraft:white_shulker_box",
    "minecraft:orange_shulker_box",
    "minecraft:magenta_shulker_box",
    "minecraft:light_blue_shulker_box",
    "minecraft:yellow_shulker_box",
    "minecraft:lime_shulker_box",
    "minecraft:pink_shulker_box",
    "minecraft:gray_shulker_box",
    "minecraft:light_gray_shulker_box",
    "minecraft:cyan_shulker_box",
    "minecraft:purple_shulker_box",
    "minecraft:blue_shulker_box",
    "minecraft:brown_shulker_box",
    "minecraft:green_shulker_box",
    "minecraft:red_shulker_box",
    "minecraft:black_shulker_box",
];

/// Vanilla items whose behaviour no registration implements yet (item, the missing behaviour).
#[rustfmt::skip]
const NOT_IMPLEMENTED: &[(&str, &str)] = &[
    ("minecraft:egg", "the projectile entity is created from the item's ProjectileItem and shot with ProjectileDispenseBehavior (the item class is not a ProjectileItem yet, so there is nothing to register)"),
    ("minecraft:blue_egg", "the projectile entity is created from the item's ProjectileItem and shot with ProjectileDispenseBehavior (the item class is not a ProjectileItem yet, so there is nothing to register)"),
    ("minecraft:brown_egg", "the projectile entity is created from the item's ProjectileItem and shot with ProjectileDispenseBehavior (the item class is not a ProjectileItem yet, so there is nothing to register)"),
    ("minecraft:splash_potion", "the projectile entity is created from the item's ProjectileItem and shot with ProjectileDispenseBehavior (the item class is not a ProjectileItem yet, so there is nothing to register)"),
    ("minecraft:lingering_potion", "the projectile entity is created from the item's ProjectileItem and shot with ProjectileDispenseBehavior (the item class is not a ProjectileItem yet, so there is nothing to register)"),
    ("minecraft:fire_charge", "the projectile entity is created from the item's ProjectileItem and shot with ProjectileDispenseBehavior (the item class is not a ProjectileItem yet, so there is nothing to register)"),
    ("minecraft:chest", "equips a chest onto a tamed chested horse in front through slot 499, else drops the chest (no chested horse slot hook yet)"),
    ("minecraft:lava_bucket", "empties the bucket contents into the block in front (DispensibleContainerItem.emptyContents) and returns an empty bucket, else drops it"),
    ("minecraft:water_bucket", "empties the bucket contents into the block in front (DispensibleContainerItem.emptyContents) and returns an empty bucket, else drops it"),
    ("minecraft:powder_snow_bucket", "empties the bucket contents into the block in front (DispensibleContainerItem.emptyContents) and returns an empty bucket, else drops it"),
    ("minecraft:salmon_bucket", "empties the bucket contents into the block in front (DispensibleContainerItem.emptyContents) and returns an empty bucket, else drops it"),
    ("minecraft:cod_bucket", "empties the bucket contents into the block in front (DispensibleContainerItem.emptyContents) and returns an empty bucket, else drops it"),
    ("minecraft:pufferfish_bucket", "empties the bucket contents into the block in front (DispensibleContainerItem.emptyContents) and returns an empty bucket, else drops it"),
    ("minecraft:tropical_fish_bucket", "empties the bucket contents into the block in front (DispensibleContainerItem.emptyContents) and returns an empty bucket, else drops it"),
    ("minecraft:axolotl_bucket", "empties the bucket contents into the block in front (DispensibleContainerItem.emptyContents) and returns an empty bucket, else drops it"),
    ("minecraft:sulfur_cube_bucket", "empties the bucket contents into the block in front (DispensibleContainerItem.emptyContents) and returns an empty bucket, else drops it"),
    ("minecraft:tadpole_bucket", "empties the bucket contents into the block in front (DispensibleContainerItem.emptyContents) and returns an empty bucket, else drops it"),
    ("minecraft:bucket", "picks up the fluid block in front (BucketPickup.pickupBlock) and returns the filled bucket, else drops the bucket"),
    ("minecraft:flint_and_steel", "FlintAndSteelDispenseItemBehavior: sets fire, lights a campfire, candle or candle cake, primes TNT or an exploding sulfur cube in front and damages the tool"),
    ("minecraft:bone_meal", "grows the crop or water plant in front (BoneMealItem.growCrop/growWaterPlant) with level event 1505, fails with the failure click"),
    ("minecraft:wither_skeleton_skull", "places the skull in front and checks the wither pattern (WitherSkullBlock.canSpawnMob/checkSpawn), else equips it on an entity in front"),
    ("minecraft:carved_pumpkin", "places the pumpkin in front when it completes a golem pattern (CarvedPumpkinBlock.canSpawnGolem), else equips it on an entity in front"),
    ("minecraft:glass_bottle", "fills the bottle from a honey-full beehive or the water in front (takeLiquid with FLUID_PICKUP), else drops it"),
    ("minecraft:glowstone", "charges the respawn anchor in front (RespawnAnchorBlock.charge), else drops it"),
    ("minecraft:shears", "ShearsDispenseItemBehavior: shears a beehive or a shearable entity in front and damages the shears"),
    ("minecraft:brush", "brushes a scute off an armadillo in front (Armadillo.brushOffScute) and damages the brush"),
    ("minecraft:honeycomb", "waxes the block in front (HoneycombItem.getWaxed) with level event 3003, else drops it"),
    ("minecraft:minecart", "MinecartDispenseItemBehavior: places the minecart on a rail in front (no minecart entities yet)"),
    ("minecraft:chest_minecart", "MinecartDispenseItemBehavior: places the minecart on a rail in front (no minecart entities yet)"),
    ("minecraft:furnace_minecart", "MinecartDispenseItemBehavior: places the minecart on a rail in front (no minecart entities yet)"),
    ("minecraft:tnt_minecart", "MinecartDispenseItemBehavior: places the minecart on a rail in front (no minecart entities yet)"),
    ("minecraft:hopper_minecart", "MinecartDispenseItemBehavior: places the minecart on a rail in front (no minecart entities yet)"),
    ("minecraft:command_block_minecart", "MinecartDispenseItemBehavior: places the minecart on a rail in front (no minecart entities yet)"),
];

/// Registered items that only implement part of the vanilla behaviour (item, the missing part). The entry is deleted by
/// hand with the commit that completes the behaviour.
#[rustfmt::skip]
const INCOMPLETE: &[(&str, &str)] = &[
    ("minecraft:tnt", "SulfurCubeBlockDispenseItemBehavior.dispenseBlock: a sulfur cube in front swallows the TNT block instead of the TNT being primed (the TNT behaviour does not call it yet)"),
];

fn registered() -> BTreeSet<&'static str> {
    DISPENSE_BEHAVIORS
        .iter()
        .flat_map(|r| r.items.iter().copied())
        .collect()
}

#[test]
fn vanilla_list_is_82_existing_items() {
    let unique: BTreeSet<_> = VANILLA.iter().collect();
    assert_eq!(unique.len(), VANILLA.len(), "duplicate item in VANILLA");
    assert_eq!(VANILLA.len(), 82);
    let unknown: Vec<_> = VANILLA
        .iter()
        .filter(|n| Item::by_name(n).is_none())
        .collect();
    assert!(unknown.is_empty(), "not items: {unknown:?}");
}

/// Every vanilla item is registered or listed as missing; nothing else is registered.
#[test]
fn every_vanilla_item_is_registered_or_accounted_for() {
    let registered = registered();
    let vanilla: BTreeSet<&str> = VANILLA.iter().copied().collect();
    let mut problems = Vec::new();
    for name in &vanilla {
        let listed = NOT_IMPLEMENTED.iter().any(|(n, _)| n == name);
        match (registered.contains(name), listed) {
            (true, false) | (false, true) => {}
            (false, false) => problems.push(format!(
                "{name} has no dispenser behaviour: register it, or list it in NOT_IMPLEMENTED with the missing behaviour"
            )),
            (true, true) => problems.push(format!(
                "{name} is registered: delete it from NOT_IMPLEMENTED"
            )),
        }
    }
    for name in &registered {
        if !vanilla.contains(name) {
            problems.push(format!(
                "{name} has a dispenser behaviour that vanilla does not register"
            ));
        }
    }
    for (name, reason) in NOT_IMPLEMENTED.iter().chain(INCOMPLETE) {
        if !vanilla.contains(name) {
            problems.push(format!("{name} is not in VANILLA: delete its entry"));
        }
        if reason.is_empty() {
            problems.push(format!("{name} has no reason"));
        }
    }
    for (name, _) in INCOMPLETE {
        if !registered.contains(name) {
            problems.push(format!(
                "{name} is listed as incomplete but not registered: move it to NOT_IMPLEMENTED"
            ));
        }
    }
    assert!(problems.is_empty(), "{problems:#?}");
}

/// `getDefaultDispenseMethod` returns something other than the plain drop for equippable items, sulfur cube
/// swallowables and spawn eggs with entity data.
#[test]
fn component_defaults_are_not_the_plain_drop() {
    let swallowable = mc_data::tags::get::<Item>("sulfur_cube_swallowable")
        .and_then(|tag| Item::iter().find(|item| tag.contains(*item)))
        .expect("the sulfur_cube_swallowable tag has items");
    let stacks = [
        ("equippable", ItemStack::new(Item::LEATHER_HELMET)),
        ("sulfur_cube_swallowable", ItemStack::new(swallowable)),
        (
            "spawn_egg_with_entity_data",
            ItemStack::new(Item::ZOMBIE_SPAWN_EGG),
        ),
    ];
    assert!(stacks[0].1.has(DataComponentType::EQUIPPABLE));
    assert!(stacks[2].1.has(DataComponentType::ENTITY_DATA));
    let plain: Vec<_> = stacks
        .iter()
        .filter(|(_, stack)| {
            std::ptr::addr_eq(get_default_dispense_method(stack), default_behavior())
        })
        .map(|(name, _)| name)
        .collect();
    assert!(plain.is_empty(), "dispensed as a plain drop: {plain:?}");
}
