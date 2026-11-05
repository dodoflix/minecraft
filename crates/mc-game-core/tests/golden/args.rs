//! Command argument types against the vanilla goldens (`../mc-game/tests/fixtures/args/<name>_golden.txt`, `scripts/args-golden.sh`).

use crate::args_common;

use mc_game::commands::arguments as a;
use std::sync::Arc;

macro_rules! case {
    ($test:ident, $name:literal, $reason:literal, $make:expr, $render:expr) => {
        #[test]
        #[ignore = $reason]
        fn $test() {
            args_common::replay($name, include_str!(concat!("../../../mc-game/tests/fixtures/args/", $name, "_golden.txt")), $make, $render);
        }
    };
}

/// `ResourceOrIdArgument` over one registry: every case shares the render and the known deviation.
macro_rules! resource_or_id_case {
    ($test:ident, $name:literal, $make:expr) => {
        case!(
            $test,
            $name,
            "known deviation: mc_dfu error text prints NBT values as JSON ({\"a\":1}, 1, 0.5) where vanilla prints SNBT ({a:1}, 1b, 0.5d) (feature: no Feature.DIRECT_CODEC yet)",
            $make,
            |v, _| args_common::render_resource_or_id(v.downcast_ref::<a::resource_arguments::ResourceOrId>().expect("resource or id"))
        );
    };
}

case!(
    component_matches_vanilla,
    "component",
    "known deviation: ComponentSerialization either-codec error text (Failed to parse either ...) differs; also NBT value text",
    |_ctx| Arc::new(a::ComponentArgument::text_component()),
    |v, _src| args_common::canon_json(&v.downcast_ref::<mc_text::Component>().expect("component").to_json().expect("json"))
);

case!(
    item_matches_vanilla,
    "item",
    "known deviation: mc_dfu error text prints NBT values as JSON ({\"a\":1}, 1, 0.5) where vanilla prints SNBT ({a:1}, 1b, 0.5d) (feature: no Feature.DIRECT_CODEC yet)",
    |ctx| Arc::new(a::item::item_argument::ItemArgument::item(ctx)),
    |v, _src| {
        let i = v.downcast_ref::<a::item::item_input::ItemInput>().expect("item");
        let patch = mc_dfu::encode_nbt(&mc_protocol::components::DataComponentPatch::codec(), &i.components).into_result().map_or("<encode failed>".to_owned(), |t| t.to_string());
        format!("{}|{patch}", i.item.name())
    }
);

case!(
    nbt_tag_matches_vanilla,
    "nbt_tag",
    "known deviation: SNBT suggestions miss the word terminals (bool, uuid, true, false) after an unquoted-string failure",
    |_ctx| Arc::new(a::NbtTagArgument::nbt_tag()),
    |v, _src| v.downcast_ref::<mc_nbt::Tag>().expect("tag").to_string()
);

case!(
    slot_source_matches_vanilla,
    "slot_source",
    "known deviation: SlotSourceArgument.list_suggestions drops the slot-range names (builder.add(sub)) and the registry elements",
    |ctx| Arc::new(a::slot_arguments::SlotSourceArgument::slot_source(ctx)),
    |v, _| {
        let r = v.downcast_ref::<a::slot_arguments::SlotSourceResult>().expect("slot source");
        format!("{}|{}", r.name().unwrap_or_else(|| "<unnamed>".to_owned()), if matches!(r, a::slot_arguments::SlotSourceResult::Literal(_)) { "literal" } else { "holder" })
    }
);

case!(
    style_matches_vanilla,
    "style",
    "known deviation: Style codec error text differs (Not a number / Not a map: [])",
    |_ctx| Arc::new(a::StyleArgument::style()),
    |v, _src| args_common::canon_json(&v.downcast_ref::<mc_text::Style>().expect("style").to_json().expect("json"))
);

case!(
    swizzle_matches_vanilla,
    "swizzle",
    "known limit: vanilla's message contains a lone surrogate (UTF-16 split of an emoji), not representable in a Rust String",
    |_| Arc::new(a::coordinates::swizzle_argument::SwizzleArgument::swizzle()),
    |v, _| format!("[{}]", v.downcast_ref::<std::collections::BTreeSet<mc_core::direction::Axis>>().expect("axes").iter().map(|a| format!("{a:?}").to_lowercase()).collect::<Vec<_>>().join(", "))
);

resource_or_id_case!(resource_or_id_dialog_matches_vanilla, "resource_or_id_dialog", |ctx| Arc::new(a::resource_arguments::DialogArgument::dialog(ctx)));
resource_or_id_case!(resource_or_id_feature_matches_vanilla, "resource_or_id_feature", |ctx| Arc::new(a::resource_arguments::FeatureArgument::feature(ctx)));
resource_or_id_case!(resource_or_id_float_provider_matches_vanilla, "resource_or_id_float_provider", |ctx| Arc::new(a::resource_arguments::ContextFloatProviderArgument::float_provider(ctx)));
resource_or_id_case!(resource_or_id_int_provider_matches_vanilla, "resource_or_id_int_provider", |ctx| Arc::new(a::resource_arguments::ContextIntProviderArgument::int_provider(ctx)));
resource_or_id_case!(resource_or_id_loot_modifier_matches_vanilla, "resource_or_id_loot_modifier", |ctx| Arc::new(a::resource_arguments::LootModifierArgument::loot_modifier(ctx)));
resource_or_id_case!(resource_or_id_loot_predicate_matches_vanilla, "resource_or_id_loot_predicate", |ctx| Arc::new(a::resource_arguments::LootPredicateArgument::loot_predicate(ctx)));
