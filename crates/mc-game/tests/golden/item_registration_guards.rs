//! Guards of the item registrations (hermetic).
//!
//! `behaviour(item)` binds an item to its `ITEMS` registration by name, else to the FIRST registered class of its Java
//! class chain (`ItemData::classes`), else to `PlainItem` (vanilla's base `Item`). An item class with behaviour of its own
//! that nobody registered therefore silently does nothing: every vanilla item class of `ref/extracted/items.json` must be
//! registered, be plain in vanilla too, or be listed below with the behaviour that is missing.
use mc_data::Item;
use mc_game::world::item::{BlockItem, ITEM_CLASSES, ITEMS};
use std::collections::{BTreeMap, BTreeSet};

/// Vanilla classes that add nothing to `Item` (data components only): `PlainItem` is their correct behaviour.
const EXPECTED_PLAIN: &[&str] = &["Item", "SmithingTemplateItem", "DiscFragmentItem"];

/// Vanilla classes with behaviour of their own that no registration implements yet (class, the missing behaviour). The
/// entry goes away with the commit that registers the class.
const EXPECTED_MISSING: &[(&str, &str)] = &[
    (
        "EggItem",
        "use plays the throw sound and spawns a ThrownEgg (no egg entity yet)",
    ),
    (
        "EndCrystalItem",
        "useOn on obsidian or bedrock places an EndCrystal and tries to respawn the dragon fight (no end crystal entity, no dragon fight respawn yet)",
    ),
    (
        "EnderpearlItem",
        "use spawns a ThrownEnderpearl and starts the item cooldown (no ender pearl entity yet)",
    ),
    (
        "FishingRodItem",
        "use casts a FishingHook or reels it in and damages the rod (no fishing hook entity yet)",
    ),
    (
        "FoodOnAStickItem",
        "use boosts the ridden pig or strider and damages the item",
    ),
    (
        "LeadItem",
        "useOn on a fence attaches the leashed mobs nearby to a LeashFenceKnotEntity (no leash knot entity yet)",
    ),
    (
        "MinecartItem",
        "useOn on a rail places the minecart entity of the item (no minecart entities yet)",
    ),
];

/// Classes that ARE registered but only implement part of the vanilla class (class, the missing behaviour). Registering
/// the class cannot tell these from a complete one, so the entry is deleted by hand with the commit that completes it.
const EXPECTED_INCOMPLETE: &[(&str, &str)] = &[
    (
        "EnderEyeItem",
        "use throws an EyeOfEnder towards the nearest stronghold (no eye of ender entity yet: the eye is neither thrown nor consumed)",
    ),
    (
        "LingeringPotionItem",
        "use spawns a ThrownLingeringPotion (no potion entity yet: the stack is consumed and nothing is thrown)",
    ),
    (
        "SplashPotionItem",
        "use spawns a ThrownSplashPotion (no potion entity yet: the stack is consumed and nothing is thrown)",
    ),
];

/// Whether `item` is bound to a registration (`behaviour::build`).
fn registered(item: Item) -> bool {
    let data = item.data();
    ITEMS.iter().any(|r| r.item == data.name)
        || data
            .classes
            .iter()
            .any(|class| ITEM_CLASSES.iter().any(|r| r.class == *class))
}

/// The vanilla item classes with the items that use them.
fn classes() -> BTreeMap<&'static str, Vec<Item>> {
    let mut classes: BTreeMap<_, Vec<_>> = BTreeMap::new();
    for item in Item::iter() {
        classes.entry(item.data().class).or_default().push(item);
    }
    classes
}

#[test]
fn every_item_class_is_registered_or_accounted_for() {
    let classes = classes();
    assert_eq!(
        classes.len(),
        66,
        "the vanilla item classes changed: classify the new ones"
    );
    let mut problems = Vec::new();
    for (class, items) in &classes {
        let bound = items.iter().any(|i| registered(*i));
        let plain = EXPECTED_PLAIN.contains(class);
        let missing = EXPECTED_MISSING.iter().any(|(c, _)| c == class);
        let incomplete = EXPECTED_INCOMPLETE.iter().any(|(c, _)| c == class);
        match (bound, plain, missing, incomplete) {
            (true, false, false, _) | (false, true, false, false) | (false, false, true, false) => {}
            (false, false, false, _) => problems.push(format!(
                "{class} ({} items) has no registration: register it, or list it in EXPECTED_PLAIN / EXPECTED_MISSING",
                items.len()
            )),
            (true, true, ..) => problems.push(format!(
                "{class} is registered but listed as plain in vanilla: delete it from EXPECTED_PLAIN"
            )),
            (true, _, true, _) => problems.push(format!(
                "{class} is registered: delete it from EXPECTED_MISSING"
            )),
            (false, _, _, true) => problems.push(format!(
                "{class} is listed as incomplete but has no registration: move it to EXPECTED_MISSING"
            )),
            _ => problems.push(format!("{class} is listed in several expectation lists")),
        }
    }
    for (class, _) in EXPECTED_MISSING.iter().chain(EXPECTED_INCOMPLETE) {
        if !classes.contains_key(class) {
            problems.push(format!(
                "{class} is not a vanilla item class any more: delete its entry"
            ));
        }
    }
    for class in EXPECTED_PLAIN {
        if !classes.contains_key(class) {
            problems.push(format!(
                "{class} is not a vanilla item class any more: delete it from EXPECTED_PLAIN"
            ));
        }
    }
    assert!(problems.is_empty(), "{problems:#?}");
}

/// A registration names a vanilla class: a typo registers nothing.
#[test]
fn every_registered_class_and_item_exists() {
    let known: BTreeSet<&str> = Item::iter()
        .flat_map(|i| i.data().classes.iter().copied())
        .collect();
    let unknown: Vec<_> = ITEM_CLASSES
        .iter()
        .map(|r| r.class)
        .filter(|c| !known.contains(c))
        .collect();
    assert!(
        unknown.is_empty(),
        "registered classes no item has: {unknown:?}"
    );
    let names: BTreeSet<&str> = Item::iter().map(|i| i.data().name).collect();
    let unknown: Vec<_> = ITEMS
        .iter()
        .map(|r| r.item)
        .filter(|n| !names.contains(n))
        .collect();
    assert!(
        unknown.is_empty(),
        "registered items that do not exist: {unknown:?}"
    );
}

/// The `BlockItem`s that place a different block on a wall are exactly the vanilla `StandingAndWallBlockItem` family
/// (torches, coral fans, signs, hanging signs, heads, banners).
#[test]
fn standing_and_wall_items_match_the_vanilla_class_chain() {
    let mut vanilla = BTreeSet::new();
    let mut ours = BTreeSet::new();
    for item in Item::iter() {
        let data = item.data();
        if data.classes.contains(&"StandingAndWallBlockItem") {
            vanilla.insert(data.name);
        }
        if data.classes.contains(&"BlockItem") && BlockItem::for_item(item).wall_block().is_some() {
            ours.insert(data.name);
        }
    }
    assert_eq!(vanilla.len(), 63);
    assert_eq!(
        ours.symmetric_difference(&vanilla).collect::<Vec<_>>(),
        Vec::<&&str>::new(),
        "items that differ between the wall block table and the vanilla class chain"
    );
}
