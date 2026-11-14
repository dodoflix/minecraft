//! `DebugSubscriptions`: the registered subscriptions the server produces values for.

use mc_data::DebugSubscription;
use std::sync::LazyLock;

macro_rules! subscriptions {
    ($($(#[$m:meta])* $name:ident = $id:literal;)*) => {
        $(
            $(#[$m])*
            pub fn $name() -> DebugSubscription {
                static SUBSCRIPTION: LazyLock<DebugSubscription> = LazyLock::new(|| {
                    DebugSubscription::by_name($id).expect("registered debug subscription")
                });
                *SUBSCRIPTION
            }
        )*
    };
}

subscriptions! {
    bees = "minecraft:bees";
    brains = "minecraft:brains";
    breezes = "minecraft:breezes";
    goal_selectors = "minecraft:goal_selectors";
    entity_paths = "minecraft:entity_paths";
    bee_hives = "minecraft:bee_hives";
    pois = "minecraft:pois";
    village_sections = "minecraft:village_sections";
    raids = "minecraft:raids";
    structures = "minecraft:structures";
    game_event_listeners = "minecraft:game_event_listeners";
    game_events = "minecraft:game_events";
}
