//! Argument types against the vanilla goldens (`fixtures/args/<name>_golden.txt`, `scripts/args-golden.sh`): each case replays one
//! recording through `args_common::replay` and expands to a `<name>_matches_vanilla` test.

use crate::args_common;

use mc_game::commands::arguments;
use mc_game::commands::arguments::coordinates::vec2_argument::Vec2Argument;
use mc_game::commands::arguments::coordinates::vec3_argument::Vec3Argument;
use mc_game::commands::arguments::entity_anchor_argument::EntityAnchorArgument;
use mc_game::commands::arguments::selector::EntitySelector;
use std::sync::Arc;

/// `make` builds the argument type from the command build context, `render` mirrors the generator's value rendering and
/// the optional last argument is the suggestion provider on the argument node. `$scope` is the module whose items the
/// closures name unqualified.
macro_rules! args_case {
    ($test:ident, $name:literal, $($scope:ident)::+, $make:expr, $render:expr $(,)?) => {
        args_case!(@emit $test, $name, [$($scope)::+], $make, $render, None);
    };
    ($test:ident, $name:literal, $($scope:ident)::+, $make:expr, $render:expr, $provider:expr $(,)?) => {
        args_case!(@emit $test, $name, [$($scope)::+], $make, $render, Some($provider));
    };
    (@emit $test:ident, $name:literal, [$($scope:ident)::+], $make:expr, $render:expr, $provider:expr) => {
        #[test]
        fn $test() {
            #[allow(unused_imports, reason = "the glob brings the scope module's names in; cases that need none leave it unused")]
            use $($scope)::+::*;
            args_common::replay_with($name, &args_common::golden_text($name), $make, $render, $provider);
        }
    };
}

args_case!(
    angle_matches_vanilla,
    "angle",
    arguments,
    |_| Arc::new(AngleArgument::angle()),
    |v, _src| mc_core::math::java::java_float_to_string(
        v.downcast_ref::<SingleAngle>()
            .expect("angle")
            .get_angle(_src)
    ),
);

args_case!(
    block_pos_matches_vanilla,
    "block_pos",
    arguments,
    |_| Arc::new(coordinates::block_pos_argument::BlockPosArgument::block_pos()),
    |v, src| args_common::render_coordinates(
        v.downcast_ref::<coordinates::Coordinates>()
            .expect("coordinates"),
        src
    ),
);

args_case!(
    block_predicate_matches_vanilla,
    "block_predicate",
    arguments,
    |ctx| {
        Arc::new(blocks::block_predicate_argument::BlockPredicateArgument::block_predicate(ctx))
    },
    |v, _src| {
        args_common::render_block_predicate(
            v.downcast_ref::<blocks::block_predicate_argument::BlockPredicate>()
                .expect("predicate"),
        )
    },
);

args_case!(
    block_state_matches_vanilla,
    "block_state",
    arguments,
    |ctx| { Arc::new(blocks::block_state_argument::BlockStateArgument::block(ctx)) },
    |v, _src| {
        let b = v
            .downcast_ref::<blocks::block_input::BlockInput>()
            .expect("block");
        let mut names: Vec<&str> = b
            .get_defined_properties()
            .iter()
            .map(|(p, _)| p.name)
            .collect();
        names.sort();
        format!(
            "{}|[{}]|{}",
            args_common::render_state(b.get_state()),
            names.join(", "),
            b.tag()
                .map_or("null".to_owned(), |t| mc_nbt::Tag::Compound(t.clone())
                    .to_string())
        )
    },
);

args_case!(
    column_pos_matches_vanilla,
    "column_pos",
    arguments,
    |_| Arc::new(coordinates::column_pos_argument::ColumnPosArgument::column_pos()),
    |v, src| args_common::render_coordinates(
        v.downcast_ref::<coordinates::Coordinates>()
            .expect("coordinates"),
        src
    ),
);

args_case!(
    compound_tag_matches_vanilla,
    "compound_tag",
    arguments,
    |_ctx| { Arc::new(CompoundTagArgument::compound_tag()) },
    |v, _src| {
        mc_nbt::Tag::Compound(
            v.downcast_ref::<mc_nbt::CompoundTag>()
                .expect("compound")
                .clone(),
        )
        .to_string()
    },
);

args_case!(
    dimension_matches_vanilla,
    "dimension",
    arguments,
    |_| Arc::new(DimensionArgument::dimension()),
    |v, _src| v
        .downcast_ref::<mc_core::ident::Identifier>()
        .expect("id")
        .to_string(),
);

args_case!(
    entity_anchor_matches_vanilla,
    "entity_anchor",
    arguments,
    |_| Arc::new(EntityAnchorArgument::anchor()),
    |v, _src| format!(
        "{:?}",
        v.downcast_ref::<mc_game::commands::arguments::entity_anchor_argument::Anchor>()
            .expect("a")
    )
    .to_uppercase(),
);

args_case!(
    entity_entities_matches_vanilla,
    "entity_entities",
    arguments,
    |_| Arc::new(EntityArgument::entities()),
    |v, _| v
        .downcast_ref::<EntitySelector>()
        .expect("selector")
        .describe(),
);

args_case!(
    entity_entity_matches_vanilla,
    "entity_entity",
    arguments,
    |_| Arc::new(EntityArgument::entity()),
    |v, _| v
        .downcast_ref::<EntitySelector>()
        .expect("selector")
        .describe(),
);

args_case!(
    entity_player_matches_vanilla,
    "entity_player",
    arguments,
    |_| Arc::new(EntityArgument::player()),
    |v, _| v
        .downcast_ref::<EntitySelector>()
        .expect("selector")
        .describe(),
);

args_case!(
    entity_players_matches_vanilla,
    "entity_players",
    arguments,
    |_| Arc::new(EntityArgument::players()),
    |v, _| v
        .downcast_ref::<EntitySelector>()
        .expect("selector")
        .describe(),
);

args_case!(
    float_range_matches_vanilla,
    "float_range",
    arguments,
    |_| Arc::new(FloatRangeArgument::float_range()),
    |v, _src| args_common::render_doubles(
        v.downcast_ref::<min_max_bounds::Doubles>()
            .expect("doubles")
    ),
);

args_case!(
    game_mode_matches_vanilla,
    "game_mode",
    arguments,
    |_| Arc::new(GameModeArgument::game_mode()),
    |v, _src| v
        .downcast_ref::<mc_world::level_storage::GameType>()
        .expect("gm")
        .get_name()
        .to_owned(),
);

args_case!(
    game_profile_matches_vanilla,
    "game_profile",
    arguments,
    |_ctx| { Arc::new(GameProfileArgument::game_profile()) },
    |v, _src| {
        match v
            .downcast_ref::<game_profile_argument::GameProfileResult>()
            .expect("result")
        {
            game_profile_argument::GameProfileResult::Selector(s) => {
                format!("selector {}", s.describe())
            }
            _ => "plain".to_owned(),
        }
    },
);

args_case!(
    heightmap_type_matches_vanilla,
    "heightmap_type",
    arguments,
    |_| Arc::new(HeightmapTypeArgument),
    |v, _src| v
        .downcast_ref::<mc_world::heightmap::HeightmapType>()
        .expect("h")
        .serialization_key()
        .to_owned(),
);

args_case!(
    hex_color_matches_vanilla,
    "hex_color",
    arguments,
    |_| Arc::new(HexColorArgument::hex_color()),
    |v, _| v.downcast_ref::<i32>().expect("int").to_string(),
);

args_case!(
    identifier_matches_vanilla,
    "identifier",
    arguments,
    |_| Arc::new(IdentifierArgument::id()),
    |v, _src| v
        .downcast_ref::<mc_core::ident::Identifier>()
        .expect("id")
        .to_string(),
);

args_case!(
    int_range_matches_vanilla,
    "int_range",
    arguments,
    |_| Arc::new(IntRangeArgument::int_range()),
    |v, _src| args_common::render_bounds(
        &v.downcast_ref::<min_max_bounds::Ints>()
            .expect("ints")
            .bounds,
        |x| x.to_string()
    ),
);

args_case!(
    message_matches_vanilla,
    "message",
    arguments,
    |_ctx| { Arc::new(MessageArgument::message()) },
    |v, _src| {
        args_common::render_message(
            v.downcast_ref::<message_argument::Message>()
                .expect("message"),
        )
    },
);

args_case!(
    message_noselectors_matches_vanilla,
    "message_noselectors",
    arguments,
    |_ctx| { Arc::new(MessageArgument::message()) },
    |v, _src| {
        args_common::render_message(
            v.downcast_ref::<message_argument::Message>()
                .expect("message"),
        )
    },
);

args_case!(
    objective_criteria_matches_vanilla,
    "objective_criteria",
    arguments,
    |_| Arc::new(ObjectiveCriteriaArgument::criteria()),
    |v, _src| args_common::render_criteria(
        v.downcast_ref::<scores::ObjectiveCriteria>()
            .expect("criteria")
    ),
);

args_case!(
    objective_matches_vanilla,
    "objective",
    arguments,
    |_| Arc::new(ObjectiveArgument::objective()),
    |v, _src| v.downcast_ref::<String>().expect("name").clone(),
);

args_case!(
    operation_matches_vanilla,
    "operation",
    arguments,
    |_| Arc::new(OperationArgument::operation()),
    |v, _src| args_common::render_operation(*v.downcast_ref::<Operation>().expect("operation")),
);

args_case!(
    resource_attribute_matches_vanilla,
    "resource_attribute",
    arguments::resource_arguments,
    |ctx| Arc::new(ResourceArgument::resource(ctx, "attribute")),
    |v, _| v.downcast_ref::<ResourceKey>().expect("key").id.to_string(),
);

args_case!(
    resource_biome_matches_vanilla,
    "resource_biome",
    arguments::resource_arguments,
    |ctx| Arc::new(ResourceArgument::resource(ctx, "worldgen/biome")),
    |v, _| v.downcast_ref::<ResourceKey>().expect("key").id.to_string(),
);

args_case!(
    resource_block_matches_vanilla,
    "resource_block",
    arguments::resource_arguments,
    |ctx| Arc::new(ResourceArgument::resource(ctx, "block")),
    |v, _| v.downcast_ref::<ResourceKey>().expect("key").id.to_string(),
);

args_case!(
    resource_enchantment_matches_vanilla,
    "resource_enchantment",
    arguments::resource_arguments,
    |ctx| Arc::new(ResourceArgument::resource(ctx, "enchantment")),
    |v, _| v.downcast_ref::<ResourceKey>().expect("key").id.to_string(),
);

args_case!(
    resource_entity_type_matches_vanilla,
    "resource_entity_type",
    arguments::resource_arguments,
    |ctx| Arc::new(ResourceArgument::resource(ctx, "entity_type")),
    |v, _| v.downcast_ref::<ResourceKey>().expect("key").id.to_string(),
);

args_case!(
    resource_item_matches_vanilla,
    "resource_item",
    arguments::resource_arguments,
    |ctx| Arc::new(ResourceArgument::resource(ctx, "item")),
    |v, _| v.downcast_ref::<ResourceKey>().expect("key").id.to_string(),
);

args_case!(
    resource_key_attribute_matches_vanilla,
    "resource_key_attribute",
    arguments::resource_arguments,
    |_| Arc::new(ResourceKeyArgument::key("attribute")),
    |v, _| args_common::render_key(v.downcast_ref::<ResourceKey>().expect("key")),
);

args_case!(
    resource_key_biome_matches_vanilla,
    "resource_key_biome",
    arguments::resource_arguments,
    |_| Arc::new(ResourceKeyArgument::key("worldgen/biome")),
    |v, _| args_common::render_key(v.downcast_ref::<ResourceKey>().expect("key")),
);

args_case!(
    resource_key_block_matches_vanilla,
    "resource_key_block",
    arguments::resource_arguments,
    |_| Arc::new(ResourceKeyArgument::key("block")),
    |v, _| args_common::render_key(v.downcast_ref::<ResourceKey>().expect("key")),
);

args_case!(
    resource_key_enchantment_matches_vanilla,
    "resource_key_enchantment",
    arguments::resource_arguments,
    |_| Arc::new(ResourceKeyArgument::key("enchantment")),
    |v, _| args_common::render_key(v.downcast_ref::<ResourceKey>().expect("key")),
);

args_case!(
    resource_key_entity_type_matches_vanilla,
    "resource_key_entity_type",
    arguments::resource_arguments,
    |_| Arc::new(ResourceKeyArgument::key("entity_type")),
    |v, _| args_common::render_key(v.downcast_ref::<ResourceKey>().expect("key")),
);

args_case!(
    resource_key_item_matches_vanilla,
    "resource_key_item",
    arguments::resource_arguments,
    |_| Arc::new(ResourceKeyArgument::key("item")),
    |v, _| args_common::render_key(v.downcast_ref::<ResourceKey>().expect("key")),
);

args_case!(
    resource_key_mob_effect_matches_vanilla,
    "resource_key_mob_effect",
    arguments::resource_arguments,
    |_| Arc::new(ResourceKeyArgument::key("mob_effect")),
    |v, _| args_common::render_key(v.downcast_ref::<ResourceKey>().expect("key")),
);

args_case!(
    resource_key_structure_matches_vanilla,
    "resource_key_structure",
    arguments::resource_arguments,
    |_| Arc::new(ResourceKeyArgument::key("worldgen/structure")),
    |v, _| args_common::render_key(v.downcast_ref::<ResourceKey>().expect("key")),
);

args_case!(
    resource_mob_effect_matches_vanilla,
    "resource_mob_effect",
    arguments::resource_arguments,
    |ctx| Arc::new(ResourceArgument::resource(ctx, "mob_effect")),
    |v, _| v.downcast_ref::<ResourceKey>().expect("key").id.to_string(),
);

args_case!(
    resource_or_id_loot_table_matches_vanilla,
    "resource_or_id_loot_table",
    arguments::resource_arguments,
    |ctx| Arc::new(LootTableArgument::loot_table(ctx)),
    |v, _| args_common::render_resource_or_id(
        v.downcast_ref::<ResourceOrId>().expect("resource or id")
    ),
);

args_case!(
    resource_or_tag_attribute_matches_vanilla,
    "resource_or_tag_attribute",
    arguments::resource_arguments,
    |ctx| Arc::new(ResourceOrTagArgument::resource_or_tag(ctx, "attribute")),
    |v, _| {
        let r = v.downcast_ref::<ResourceOrTag>().expect("result");
        let kind = match r {
            ResourceOrTag::Resource(k) => format!("resource {}", k.registry),
            ResourceOrTag::Tag { key, members } => format!(
                "tag ResourceKey[minecraft:root / {}] n={}",
                key.registry,
                members.len()
            ),
        };
        format!("{}|{kind}", r.as_printable())
    },
);

args_case!(
    resource_or_tag_biome_matches_vanilla,
    "resource_or_tag_biome",
    arguments::resource_arguments,
    |ctx| Arc::new(ResourceOrTagArgument::resource_or_tag(
        ctx,
        "worldgen/biome"
    )),
    |v, _| {
        let r = v.downcast_ref::<ResourceOrTag>().expect("result");
        let kind = match r {
            ResourceOrTag::Resource(k) => format!("resource {}", k.registry),
            ResourceOrTag::Tag { key, members } => format!(
                "tag ResourceKey[minecraft:root / {}] n={}",
                key.registry,
                members.len()
            ),
        };
        format!("{}|{kind}", r.as_printable())
    },
);

args_case!(
    resource_or_tag_block_matches_vanilla,
    "resource_or_tag_block",
    arguments::resource_arguments,
    |ctx| Arc::new(ResourceOrTagArgument::resource_or_tag(ctx, "block")),
    |v, _| {
        let r = v.downcast_ref::<ResourceOrTag>().expect("result");
        let kind = match r {
            ResourceOrTag::Resource(k) => format!("resource {}", k.registry),
            ResourceOrTag::Tag { key, members } => format!(
                "tag ResourceKey[minecraft:root / {}] n={}",
                key.registry,
                members.len()
            ),
        };
        format!("{}|{kind}", r.as_printable())
    },
);

args_case!(
    resource_or_tag_enchantment_matches_vanilla,
    "resource_or_tag_enchantment",
    arguments::resource_arguments,
    |ctx| Arc::new(ResourceOrTagArgument::resource_or_tag(ctx, "enchantment")),
    |v, _| {
        let r = v.downcast_ref::<ResourceOrTag>().expect("result");
        let kind = match r {
            ResourceOrTag::Resource(k) => format!("resource {}", k.registry),
            ResourceOrTag::Tag { key, members } => format!(
                "tag ResourceKey[minecraft:root / {}] n={}",
                key.registry,
                members.len()
            ),
        };
        format!("{}|{kind}", r.as_printable())
    },
);

args_case!(
    resource_or_tag_entity_type_matches_vanilla,
    "resource_or_tag_entity_type",
    arguments::resource_arguments,
    |ctx| Arc::new(ResourceOrTagArgument::resource_or_tag(ctx, "entity_type")),
    |v, _| {
        let r = v.downcast_ref::<ResourceOrTag>().expect("result");
        let kind = match r {
            ResourceOrTag::Resource(k) => format!("resource {}", k.registry),
            ResourceOrTag::Tag { key, members } => format!(
                "tag ResourceKey[minecraft:root / {}] n={}",
                key.registry,
                members.len()
            ),
        };
        format!("{}|{kind}", r.as_printable())
    },
);

args_case!(
    resource_or_tag_item_matches_vanilla,
    "resource_or_tag_item",
    arguments::resource_arguments,
    |ctx| Arc::new(ResourceOrTagArgument::resource_or_tag(ctx, "item")),
    |v, _| {
        let r = v.downcast_ref::<ResourceOrTag>().expect("result");
        let kind = match r {
            ResourceOrTag::Resource(k) => format!("resource {}", k.registry),
            ResourceOrTag::Tag { key, members } => format!(
                "tag ResourceKey[minecraft:root / {}] n={}",
                key.registry,
                members.len()
            ),
        };
        format!("{}|{kind}", r.as_printable())
    },
);

args_case!(
    resource_or_tag_key_attribute_matches_vanilla,
    "resource_or_tag_key_attribute",
    arguments::resource_arguments,
    |_| Arc::new(ResourceOrTagKeyArgument::resource_or_tag_key("attribute")),
    |v, _| {
        let r = v.downcast_ref::<ResourceOrTagKey>().expect("result");
        let kind = match r {
            ResourceOrTagKey::Resource(k) => format!("resource {}", k.registry),
            ResourceOrTagKey::Tag(k) => format!("tag ResourceKey[minecraft:root / {}]", k.registry),
        };
        format!("{}|{kind}", r.as_printable())
    },
);

args_case!(
    resource_or_tag_key_biome_matches_vanilla,
    "resource_or_tag_key_biome",
    arguments::resource_arguments,
    |_| Arc::new(ResourceOrTagKeyArgument::resource_or_tag_key(
        "worldgen/biome"
    )),
    |v, _| {
        let r = v.downcast_ref::<ResourceOrTagKey>().expect("result");
        let kind = match r {
            ResourceOrTagKey::Resource(k) => format!("resource {}", k.registry),
            ResourceOrTagKey::Tag(k) => format!("tag ResourceKey[minecraft:root / {}]", k.registry),
        };
        format!("{}|{kind}", r.as_printable())
    },
);

args_case!(
    resource_or_tag_key_block_matches_vanilla,
    "resource_or_tag_key_block",
    arguments::resource_arguments,
    |_| Arc::new(ResourceOrTagKeyArgument::resource_or_tag_key("block")),
    |v, _| {
        let r = v.downcast_ref::<ResourceOrTagKey>().expect("result");
        let kind = match r {
            ResourceOrTagKey::Resource(k) => format!("resource {}", k.registry),
            ResourceOrTagKey::Tag(k) => format!("tag ResourceKey[minecraft:root / {}]", k.registry),
        };
        format!("{}|{kind}", r.as_printable())
    },
);

args_case!(
    resource_or_tag_key_enchantment_matches_vanilla,
    "resource_or_tag_key_enchantment",
    arguments::resource_arguments,
    |_| Arc::new(ResourceOrTagKeyArgument::resource_or_tag_key("enchantment")),
    |v, _| {
        let r = v.downcast_ref::<ResourceOrTagKey>().expect("result");
        let kind = match r {
            ResourceOrTagKey::Resource(k) => format!("resource {}", k.registry),
            ResourceOrTagKey::Tag(k) => format!("tag ResourceKey[minecraft:root / {}]", k.registry),
        };
        format!("{}|{kind}", r.as_printable())
    },
);

args_case!(
    resource_or_tag_key_entity_type_matches_vanilla,
    "resource_or_tag_key_entity_type",
    arguments::resource_arguments,
    |_| Arc::new(ResourceOrTagKeyArgument::resource_or_tag_key("entity_type")),
    |v, _| {
        let r = v.downcast_ref::<ResourceOrTagKey>().expect("result");
        let kind = match r {
            ResourceOrTagKey::Resource(k) => format!("resource {}", k.registry),
            ResourceOrTagKey::Tag(k) => format!("tag ResourceKey[minecraft:root / {}]", k.registry),
        };
        format!("{}|{kind}", r.as_printable())
    },
);

args_case!(
    resource_or_tag_key_item_matches_vanilla,
    "resource_or_tag_key_item",
    arguments::resource_arguments,
    |_| Arc::new(ResourceOrTagKeyArgument::resource_or_tag_key("item")),
    |v, _| {
        let r = v.downcast_ref::<ResourceOrTagKey>().expect("result");
        let kind = match r {
            ResourceOrTagKey::Resource(k) => format!("resource {}", k.registry),
            ResourceOrTagKey::Tag(k) => format!("tag ResourceKey[minecraft:root / {}]", k.registry),
        };
        format!("{}|{kind}", r.as_printable())
    },
);

args_case!(
    resource_or_tag_key_mob_effect_matches_vanilla,
    "resource_or_tag_key_mob_effect",
    arguments::resource_arguments,
    |_| Arc::new(ResourceOrTagKeyArgument::resource_or_tag_key("mob_effect")),
    |v, _| {
        let r = v.downcast_ref::<ResourceOrTagKey>().expect("result");
        let kind = match r {
            ResourceOrTagKey::Resource(k) => format!("resource {}", k.registry),
            ResourceOrTagKey::Tag(k) => format!("tag ResourceKey[minecraft:root / {}]", k.registry),
        };
        format!("{}|{kind}", r.as_printable())
    },
);

args_case!(
    resource_or_tag_key_structure_matches_vanilla,
    "resource_or_tag_key_structure",
    arguments::resource_arguments,
    |_| Arc::new(ResourceOrTagKeyArgument::resource_or_tag_key(
        "worldgen/structure"
    )),
    |v, _| {
        let r = v.downcast_ref::<ResourceOrTagKey>().expect("result");
        let kind = match r {
            ResourceOrTagKey::Resource(k) => format!("resource {}", k.registry),
            ResourceOrTagKey::Tag(k) => format!("tag ResourceKey[minecraft:root / {}]", k.registry),
        };
        format!("{}|{kind}", r.as_printable())
    },
);

args_case!(
    resource_or_tag_mob_effect_matches_vanilla,
    "resource_or_tag_mob_effect",
    arguments::resource_arguments,
    |ctx| Arc::new(ResourceOrTagArgument::resource_or_tag(ctx, "mob_effect")),
    |v, _| {
        let r = v.downcast_ref::<ResourceOrTag>().expect("result");
        let kind = match r {
            ResourceOrTag::Resource(k) => format!("resource {}", k.registry),
            ResourceOrTag::Tag { key, members } => format!(
                "tag ResourceKey[minecraft:root / {}] n={}",
                key.registry,
                members.len()
            ),
        };
        format!("{}|{kind}", r.as_printable())
    },
);

args_case!(
    resource_or_tag_structure_matches_vanilla,
    "resource_or_tag_structure",
    arguments::resource_arguments,
    |ctx| Arc::new(ResourceOrTagArgument::resource_or_tag(
        ctx,
        "worldgen/structure"
    )),
    |v, _| {
        let r = v.downcast_ref::<ResourceOrTag>().expect("result");
        let kind = match r {
            ResourceOrTag::Resource(k) => format!("resource {}", k.registry),
            ResourceOrTag::Tag { key, members } => format!(
                "tag ResourceKey[minecraft:root / {}] n={}",
                key.registry,
                members.len()
            ),
        };
        format!("{}|{kind}", r.as_printable())
    },
);

args_case!(
    resource_selector_attribute_matches_vanilla,
    "resource_selector_attribute",
    arguments::resource_arguments,
    |ctx| Arc::new(ResourceSelectorArgument::resource_selector(
        ctx,
        "attribute"
    )),
    |v, _| args_common::render_id_list(
        &v.downcast_ref::<Vec<ResourceKey>>()
            .expect("keys")
            .iter()
            .map(|k| k.id.to_string())
            .collect::<Vec<_>>()
    ),
);

args_case!(
    resource_selector_biome_matches_vanilla,
    "resource_selector_biome",
    arguments::resource_arguments,
    |ctx| Arc::new(ResourceSelectorArgument::resource_selector(
        ctx,
        "worldgen/biome"
    )),
    |v, _| args_common::render_id_list(
        &v.downcast_ref::<Vec<ResourceKey>>()
            .expect("keys")
            .iter()
            .map(|k| k.id.to_string())
            .collect::<Vec<_>>()
    ),
);

args_case!(
    resource_selector_block_matches_vanilla,
    "resource_selector_block",
    arguments::resource_arguments,
    |ctx| Arc::new(ResourceSelectorArgument::resource_selector(ctx, "block")),
    |v, _| args_common::render_id_list(
        &v.downcast_ref::<Vec<ResourceKey>>()
            .expect("keys")
            .iter()
            .map(|k| k.id.to_string())
            .collect::<Vec<_>>()
    ),
);

args_case!(
    resource_selector_enchantment_matches_vanilla,
    "resource_selector_enchantment",
    arguments::resource_arguments,
    |ctx| Arc::new(ResourceSelectorArgument::resource_selector(
        ctx,
        "enchantment"
    )),
    |v, _| args_common::render_id_list(
        &v.downcast_ref::<Vec<ResourceKey>>()
            .expect("keys")
            .iter()
            .map(|k| k.id.to_string())
            .collect::<Vec<_>>()
    ),
);

args_case!(
    resource_selector_entity_type_matches_vanilla,
    "resource_selector_entity_type",
    arguments::resource_arguments,
    |ctx| Arc::new(ResourceSelectorArgument::resource_selector(
        ctx,
        "entity_type"
    )),
    |v, _| args_common::render_id_list(
        &v.downcast_ref::<Vec<ResourceKey>>()
            .expect("keys")
            .iter()
            .map(|k| k.id.to_string())
            .collect::<Vec<_>>()
    ),
);

args_case!(
    resource_selector_item_matches_vanilla,
    "resource_selector_item",
    arguments::resource_arguments,
    |ctx| Arc::new(ResourceSelectorArgument::resource_selector(ctx, "item")),
    |v, _| args_common::render_id_list(
        &v.downcast_ref::<Vec<ResourceKey>>()
            .expect("keys")
            .iter()
            .map(|k| k.id.to_string())
            .collect::<Vec<_>>()
    ),
);

args_case!(
    resource_selector_mob_effect_matches_vanilla,
    "resource_selector_mob_effect",
    arguments::resource_arguments,
    |ctx| Arc::new(ResourceSelectorArgument::resource_selector(
        ctx,
        "mob_effect"
    )),
    |v, _| args_common::render_id_list(
        &v.downcast_ref::<Vec<ResourceKey>>()
            .expect("keys")
            .iter()
            .map(|k| k.id.to_string())
            .collect::<Vec<_>>()
    ),
);

args_case!(
    resource_selector_structure_matches_vanilla,
    "resource_selector_structure",
    arguments::resource_arguments,
    |ctx| Arc::new(ResourceSelectorArgument::resource_selector(
        ctx,
        "worldgen/structure"
    )),
    |v, _| args_common::render_id_list(
        &v.downcast_ref::<Vec<ResourceKey>>()
            .expect("keys")
            .iter()
            .map(|k| k.id.to_string())
            .collect::<Vec<_>>()
    ),
);

args_case!(
    resource_structure_matches_vanilla,
    "resource_structure",
    arguments::resource_arguments,
    |ctx| Arc::new(ResourceArgument::resource(ctx, "worldgen/structure")),
    |v, _| v.downcast_ref::<ResourceKey>().expect("key").id.to_string(),
);

args_case!(
    rotation_matches_vanilla,
    "rotation",
    arguments,
    |_| Arc::new(coordinates::rotation_argument::RotationArgument::rotation()),
    |v, src| args_common::render_coordinates(
        v.downcast_ref::<coordinates::Coordinates>()
            .expect("coordinates"),
        src
    ),
);

args_case!(
    score_holder_matches_vanilla,
    "score_holder",
    arguments,
    |_ctx| { Arc::new(ScoreHolderArgument::score_holder()) },
    |v, _src| {
        args_common::render_score_holder(
            v.downcast_ref::<scores::ScoreHolderResult>()
                .expect("result"),
            &args_common::render_source(),
        )
    },
    scores::suggest_score_holders(),
);

args_case!(
    score_holders_matches_vanilla,
    "score_holders",
    arguments,
    |_ctx| { Arc::new(ScoreHolderArgument::score_holders()) },
    |v, _src| {
        args_common::render_score_holder(
            v.downcast_ref::<scores::ScoreHolderResult>()
                .expect("result"),
            &args_common::render_source(),
        )
    },
    scores::suggest_score_holders(),
);

args_case!(
    scoreboard_slot_matches_vanilla,
    "scoreboard_slot",
    arguments,
    |_| Arc::new(ScoreboardSlotArgument::display_slot()),
    |v, _src| v
        .downcast_ref::<scores::DisplaySlot>()
        .expect("slot")
        .get_serialized_name()
        .to_owned(),
);

args_case!(
    slot_matches_vanilla,
    "slot",
    arguments,
    |_ctx| { Arc::new(slot_arguments::SlotArgument::slot()) },
    |v, _src| { v.downcast_ref::<i32>().expect("slot").to_string() },
);

args_case!(
    slots_matches_vanilla,
    "slots",
    arguments,
    |_ctx| { Arc::new(slot_arguments::SlotsArgument::slots()) },
    |v, _src| {
        let r = v
            .downcast_ref::<slot_arguments::SlotRange>()
            .expect("range");
        format!(
            "{}|[{}]",
            r.get_serialized_name(),
            r.slots()
                .iter()
                .map(|s| s.to_string())
                .collect::<Vec<_>>()
                .join(", ")
        )
    },
);

args_case!(
    swing_animation_matches_vanilla,
    "swing_animation",
    arguments,
    |_| Arc::new(SwingAnimationArgument::swing_animation_type()),
    |v, _src| v
        .downcast_ref::<SwingAnimationType>()
        .expect("s")
        .get_serialized_name()
        .to_owned(),
);

args_case!(
    team_color_matches_vanilla,
    "team_color",
    arguments,
    |_| Arc::new(TeamColorArgument::team_color()),
    |v, _src| v
        .downcast_ref::<TeamColor>()
        .expect("c")
        .get_serialized_name()
        .to_owned(),
);

args_case!(
    team_matches_vanilla,
    "team",
    arguments,
    |_| Arc::new(TeamArgument::team()),
    |v, _src| v.downcast_ref::<String>().expect("name").clone(),
);

args_case!(
    template_mirror_matches_vanilla,
    "template_mirror",
    arguments,
    |_| Arc::new(TemplateMirrorArgument),
    |v, _src| v
        .downcast_ref::<mc_core::mirror::Mirror>()
        .expect("m")
        .get_serialized_name()
        .to_owned(),
);

args_case!(
    template_rotation_matches_vanilla,
    "template_rotation",
    arguments,
    |_| Arc::new(TemplateRotationArgument),
    |v, _src| v
        .downcast_ref::<mc_core::rotation::Rotation>()
        .expect("r")
        .get_serialized_name()
        .to_owned(),
);

args_case!(
    time_argument_matches_vanilla,
    "time",
    arguments,
    |_| Arc::new(TimeArgument::time()),
    |v, _| v.downcast_ref::<i32>().expect("int").to_string(),
);

args_case!(
    time_min_matches_vanilla,
    "time_min",
    arguments,
    |_| Arc::new(TimeArgument::time_min(20)),
    |v, _src| v.downcast_ref::<i32>().expect("int").to_string(),
);

args_case!(
    uuid_matches_vanilla,
    "uuid",
    arguments,
    |_| Arc::new(UuidArgument::uuid()),
    |v, _src| v.downcast_ref::<uuid::Uuid>().expect("u").to_string(),
);

args_case!(
    vec2_matches_vanilla,
    "vec2",
    arguments,
    |_| Arc::new(Vec2Argument::vec2()),
    |v, src| args_common::render_coordinates(
        v.downcast_ref::<coordinates::Coordinates>()
            .expect("coordinates"),
        src
    ),
);

args_case!(
    vec2_uncentered_matches_vanilla,
    "vec2_uncentered",
    arguments,
    |_| Arc::new(Vec2Argument::new(false)),
    |v, src| args_common::render_coordinates(
        v.downcast_ref::<coordinates::Coordinates>()
            .expect("coordinates"),
        src
    ),
);

args_case!(
    vec3_matches_vanilla,
    "vec3",
    arguments,
    |_| Arc::new(Vec3Argument::vec3()),
    |v, src| args_common::render_coordinates(
        v.downcast_ref::<coordinates::Coordinates>()
            .expect("coordinates"),
        src
    ),
);

args_case!(
    vec3_uncentered_matches_vanilla,
    "vec3_uncentered",
    arguments,
    |_| Arc::new(Vec3Argument::new(false)),
    |v, src| args_common::render_coordinates(
        v.downcast_ref::<coordinates::Coordinates>()
            .expect("coordinates"),
        src
    ),
);

// Cases whose recorded vanilla behaviour is not matched yet carry an `ignore` with the issue.
use mc_game::commands::arguments as a;

macro_rules! case {
    ($(#[$attr:meta])* $test:ident, $name:literal, $make:expr, $render:expr) => {
        #[test]
        $(#[$attr])*
        fn $test() {
            args_common::replay(
                $name,
                &args_common::golden_text($name),
                $make,
                $render,
            );
        }
    };
}

/// `ResourceOrIdArgument` over one registry: every case shares the render.
macro_rules! resource_or_id_case {
    ($(#[$attr:meta])* $test:ident, $name:literal, $make:expr) => {
        case!(
            $(#[$attr])*
            $test,
            $name,
            $make,
            |v, _| args_common::render_resource_or_id(v.downcast_ref::<a::resource_arguments::ResourceOrId>().expect("resource or id"))
        );
    };
}

case!(
    #[ignore = "deviation: codec error text of the component argument differs (#10)"]
    component_matches_vanilla,
    "component",
    |_ctx| Arc::new(a::ComponentArgument::text_component()),
    |v, _src| args_common::canon_json(
        &v.downcast_ref::<mc_text::Component>()
            .expect("component")
            .to_json()
            .expect("json")
    )
);

case!(
    #[ignore = "deviation: codec error text of the item argument prints NBT values as JSON (#10)"]
    item_matches_vanilla,
    "item",
    |ctx| Arc::new(a::item::item_argument::ItemArgument::item(ctx)),
    |v, _src| {
        let i = v
            .downcast_ref::<a::item::item_input::ItemInput>()
            .expect("item");
        let patch = mc_dfu::encode_nbt(
            &mc_protocol::components::DataComponentPatch::codec(),
            &i.components,
        )
        .into_result()
        .map_or("<encode failed>".to_owned(), |t| t.to_string());
        format!("{}|{patch}", i.item.name())
    }
);

case!(
    #[ignore = "deviation: SNBT suggestions miss the word terminals (#9)"]
    nbt_tag_matches_vanilla,
    "nbt_tag",
    |_ctx| Arc::new(a::NbtTagArgument::nbt_tag()),
    |v, _src| v.downcast_ref::<mc_nbt::Tag>().expect("tag").to_string()
);

case!(
    #[ignore = "deviation: slot source suggestions miss the slot ranges and registry elements (#9)"]
    slot_source_matches_vanilla,
    "slot_source",
    |ctx| Arc::new(a::slot_arguments::SlotSourceArgument::slot_source(ctx)),
    |v, _| {
        let r = v
            .downcast_ref::<a::slot_arguments::SlotSourceResult>()
            .expect("slot source");
        format!(
            "{}|{}",
            r.name().unwrap_or_else(|| "<unnamed>".to_owned()),
            if matches!(r, a::slot_arguments::SlotSourceResult::Literal(_)) {
                "literal"
            } else {
                "holder"
            }
        )
    }
);

case!(
    #[ignore = "deviation: codec error text of the style argument differs (#10)"]
    style_matches_vanilla,
    "style",
    |_ctx| Arc::new(a::StyleArgument::style()),
    |v, _src| args_common::canon_json(
        &v.downcast_ref::<mc_text::Style>()
            .expect("style")
            .to_json()
            .expect("json")
    )
);

case!(
    #[ignore = "permanent: vanilla's message contains a lone surrogate (UTF-16 split of an emoji), which a Rust String cannot hold"]
    swizzle_matches_vanilla,
    "swizzle",
    |_| Arc::new(a::coordinates::swizzle_argument::SwizzleArgument::swizzle()),
    |v, _| format!(
        "[{}]",
        v.downcast_ref::<std::collections::BTreeSet<mc_core::direction::Axis>>()
            .expect("axes")
            .iter()
            .map(|a| format!("{a:?}").to_lowercase())
            .collect::<Vec<_>>()
            .join(", ")
    )
);

resource_or_id_case!(
    resource_or_id_dialog_matches_vanilla,
    "resource_or_id_dialog",
    |ctx| Arc::new(a::resource_arguments::DialogArgument::dialog(ctx))
);
resource_or_id_case!(
    #[ignore = "deviation: inline feature definitions are read differently from vanilla (#11)"]
    resource_or_id_feature_matches_vanilla,
    "resource_or_id_feature",
    |ctx| Arc::new(a::resource_arguments::FeatureArgument::feature(ctx))
);
resource_or_id_case!(
    #[ignore = "deviation: inline float provider definitions are read differently from vanilla (#11)"]
    resource_or_id_float_provider_matches_vanilla,
    "resource_or_id_float_provider",
    |ctx| Arc::new(a::resource_arguments::ContextFloatProviderArgument::float_provider(ctx))
);
resource_or_id_case!(
    #[ignore = "deviation: inline int provider definitions are read differently from vanilla (#11)"]
    resource_or_id_int_provider_matches_vanilla,
    "resource_or_id_int_provider",
    |ctx| Arc::new(a::resource_arguments::ContextIntProviderArgument::int_provider(ctx))
);
resource_or_id_case!(
    #[ignore = "deviation: codec error text of the loot modifier argument differs (#10)"]
    resource_or_id_loot_modifier_matches_vanilla,
    "resource_or_id_loot_modifier",
    |ctx| Arc::new(a::resource_arguments::LootModifierArgument::loot_modifier(
        ctx
    ))
);
resource_or_id_case!(
    resource_or_id_loot_predicate_matches_vanilla,
    "resource_or_id_loot_predicate",
    |ctx| Arc::new(a::resource_arguments::LootPredicateArgument::loot_predicate(ctx))
);
