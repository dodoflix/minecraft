//! `ItemStackComponentizationFix`: moves the legacy `tag` of an item stack into data components.

use crate::datafix::DataFix;
use crate::extra_data_fix_utils::dye_color_id_to_name;
use crate::fixes::references::References;
use crate::legacy_component_data_fix_utils::{
    create_translatable_component, is_strictly_valid_json,
};
use crate::schema::Schema;
use crate::ty::ensure_namespaced;
use mc_core::java_collections::hash_map_ordered;
use mc_core::java_string::JavaString;
use mc_core::math::java::{java_double_to_string, java_float_to_string};
use mc_dfu::{Dynamic, JNum, OptionalDynamic};

/// The `Value` and optional `Signature` of every property of a profile, by property name.
type ProfileProperties = Vec<(JavaString, Vec<(JavaString, Option<JavaString>)>)>;

const POTION_HOLDER_IDS: &[&str] = &[
    "minecraft:potion",
    "minecraft:splash_potion",
    "minecraft:lingering_potion",
    "minecraft:tipped_arrow",
];
const BUCKETED_MOB_IDS: &[&str] = &[
    "minecraft:pufferfish_bucket",
    "minecraft:salmon_bucket",
    "minecraft:cod_bucket",
    "minecraft:tropical_fish_bucket",
    "minecraft:axolotl_bucket",
    "minecraft:tadpole_bucket",
];
const BUCKETED_MOB_TAGS: &[&str] = &[
    "NoAI",
    "Silent",
    "NoGravity",
    "Glowing",
    "Invulnerable",
    "Health",
    "Age",
    "Variant",
    "HuntingCooldown",
    "BucketVariantTag",
];
const BOOLEAN_BLOCK_STATE_PROPERTIES: &[&str] = &[
    "attached",
    "bottom",
    "conditional",
    "disarmed",
    "drag",
    "enabled",
    "extended",
    "eye",
    "falling",
    "hanging",
    "has_bottle_0",
    "has_bottle_1",
    "has_bottle_2",
    "has_record",
    "has_book",
    "inverted",
    "in_wall",
    "lit",
    "locked",
    "occupied",
    "open",
    "persistent",
    "powered",
    "short",
    "signal_fire",
    "snowy",
    "triggered",
    "unstable",
    "waterlogged",
    "berries",
    "bloom",
    "shrieking",
    "can_summon",
    "up",
    "down",
    "north",
    "east",
    "south",
    "west",
    "slot_0_occupied",
    "slot_1_occupied",
    "slot_2_occupied",
    "slot_3_occupied",
    "slot_4_occupied",
    "slot_5_occupied",
    "cracked",
    "crafting",
];

/// `new ItemStackComponentizationFix(outputSchema)`.
pub fn new(output_schema: &Schema) -> DataFix {
    DataFix::new("ItemStackComponentizationFix", output_schema, true, |df| {
        df.write_fix_and_read(
            "ItemStack componentization",
            &df.input_schema().get_type(&References::ITEM_STACK),
            &df.output_schema().get_type(&References::ITEM_STACK),
            |dynamic| match ItemStackData::read(&dynamic) {
                Some(mut item_stack) => {
                    let tag = item_stack.tag.clone();
                    fix_item_stack(&mut item_stack, &tag);
                    item_stack.write()
                }
                None => dynamic,
            },
        )
    })
}

/// `String.substring(begin, end)` for byte offsets on char boundaries; throws like Java (UTF-16 offsets in the text).
fn substring(string: &str, begin: usize, end: usize) -> &str {
    if begin > end {
        let units = |s: &str| s.encode_utf16().count();
        panic!(
            "java.lang.StringIndexOutOfBoundsException: Range [{}, {}) out of bounds for length {}",
            units(&string[..begin]),
            units(&string[..end]),
            units(string)
        );
    }
    &string[begin..end]
}

/// `String.trim()`: strips every char up to U+0020.
fn java_trim(string: &str) -> &str {
    string.trim_matches(|c: char| c <= ' ')
}

/// `Number.toString()`.
fn number_to_string(number: &JNum) -> String {
    match number {
        JNum::Byte(v) => v.to_string(),
        JNum::Short(v) => v.to_string(),
        JNum::Int(v) => v.to_string(),
        JNum::Long(v) => v.to_string(),
        JNum::Float(v) => java_float_to_string(*v),
        JNum::Double(v) => java_double_to_string(*v),
        JNum::Lazy(text) => text.clone(),
    }
}

fn fix_item_stack(item_stack: &mut ItemStackData, dynamic: &Dynamic) {
    let hide_flags = item_stack.remove_tag("HideFlags").as_int_or(0);
    item_stack.move_tag_to_component_default("Damage", "minecraft:damage", &dynamic.create_int(0));
    item_stack.move_tag_to_component_default(
        "RepairCost",
        "minecraft:repair_cost",
        &dynamic.create_int(0),
    );
    item_stack.move_tag_to_component("CustomModelData", "minecraft:custom_model_data");
    if let Some(block_state_tag) = item_stack.remove_tag("BlockStateTag").result() {
        item_stack.set_component(
            "minecraft:block_state",
            fix_block_state_tag(&block_state_tag),
        );
    }

    item_stack.move_tag_to_component("EntityTag", "minecraft:entity_data");
    item_stack.fix_sub_tag("BlockEntityTag", false, |item_stack, block_entity_tag| {
        let id = ensure_namespaced(block_entity_tag.get("id").as_string_or("").as_lossy_str());
        let block_entity_tag = fix_block_entity_tag(item_stack, block_entity_tag, &id);
        let without_id = block_entity_tag.remove("id");
        if without_id == block_entity_tag.empty_map() {
            without_id
        } else {
            block_entity_tag
        }
    });
    item_stack.move_tag_to_component("BlockEntityTag", "minecraft:block_entity_data");
    if item_stack.remove_tag("Unbreakable").as_boolean_or(false) {
        let mut component = dynamic.empty_map();
        if (hide_flags & 4) != 0 {
            component = component.set("show_in_tooltip", dynamic.create_boolean(false));
        }

        item_stack.set_component("minecraft:unbreakable", component);
    }

    fix_enchantments(
        item_stack,
        dynamic,
        "Enchantments",
        "minecraft:enchantments",
        (hide_flags & 1) != 0,
    );
    if item_stack.is("minecraft:enchanted_book") {
        fix_enchantments(
            item_stack,
            dynamic,
            "StoredEnchantments",
            "minecraft:stored_enchantments",
            (hide_flags & 32) != 0,
        );
    }

    item_stack.fix_sub_tag("display", false, |item_stack, display| {
        fix_display(item_stack, display, hide_flags)
    });
    fix_adventure_mode_checks(item_stack, dynamic, hide_flags);
    fix_attribute_modifiers(item_stack, dynamic, hide_flags);
    if let Some(trim) = item_stack.remove_tag("Trim").result() {
        let mut fixed_trim = trim;
        if (hide_flags & 128) != 0 {
            fixed_trim = fixed_trim.set("show_in_tooltip", fixed_trim.create_boolean(false));
        }

        item_stack.set_component("minecraft:trim", fixed_trim);
    }

    if (hide_flags & 32) != 0 {
        item_stack.set_component("minecraft:hide_additional_tooltip", dynamic.empty_map());
    }

    if item_stack.is("minecraft:crossbow") {
        item_stack.remove_tag("Charged");
        item_stack.move_tag_to_component_default(
            "ChargedProjectiles",
            "minecraft:charged_projectiles",
            &dynamic.create_list(Vec::new()),
        );
    }

    if item_stack.is("minecraft:bundle") {
        item_stack.move_tag_to_component_default(
            "Items",
            "minecraft:bundle_contents",
            &dynamic.create_list(Vec::new()),
        );
    }

    if item_stack.is("minecraft:filled_map") {
        item_stack.move_tag_to_component("map", "minecraft:map_id");
        let raw: Vec<(Dynamic, Dynamic)> = item_stack
            .remove_tag("Decorations")
            .as_stream()
            .into_iter()
            .map(fix_map_decoration)
            .collect();
        // `Collectors.toMap(.., (first, second) -> first)` into a `HashMap`
        let mut decorations: Vec<(Dynamic, Dynamic)> = Vec::new();
        for (key, value) in raw {
            if !decorations.iter().any(|(existing, _)| *existing == key) {
                decorations.push((key, value));
            }
        }
        if !decorations.is_empty() {
            let ordered = hash_map_ordered(decorations, |(key, _)| key.hash_code(), 16);
            item_stack.set_component("minecraft:map_decorations", dynamic.create_map(ordered));
        }
    }

    if item_stack.is_one_of(POTION_HOLDER_IDS) {
        fix_potion_contents(item_stack, dynamic);
    }

    if item_stack.is("minecraft:writable_book") {
        fix_writable_book(item_stack, dynamic);
    }

    if item_stack.is("minecraft:written_book") {
        fix_written_book(item_stack, dynamic);
    }

    if item_stack.is("minecraft:suspicious_stew") {
        item_stack.move_tag_to_component("effects", "minecraft:suspicious_stew_effects");
    }

    if item_stack.is("minecraft:debug_stick") {
        item_stack.move_tag_to_component("DebugProperty", "minecraft:debug_stick_state");
    }

    if item_stack.is_one_of(BUCKETED_MOB_IDS) {
        fix_bucketed_mob_data(item_stack, dynamic);
    }

    if item_stack.is("minecraft:goat_horn") {
        item_stack.move_tag_to_component("instrument", "minecraft:instrument");
    }

    if item_stack.is("minecraft:knowledge_book") {
        item_stack.move_tag_to_component("Recipes", "minecraft:recipes");
    }

    if item_stack.is("minecraft:compass") {
        fix_lodestone_tracker(item_stack, dynamic);
    }

    if item_stack.is("minecraft:firework_rocket") {
        fix_firework_rocket(item_stack);
    }

    if item_stack.is("minecraft:firework_star") {
        fix_firework_star(item_stack);
    }

    if item_stack.is("minecraft:player_head")
        && let Some(skull_owner) = item_stack.remove_tag("SkullOwner").result()
    {
        item_stack.set_component("minecraft:profile", fix_profile(&skull_owner));
    }
}

fn fix_block_state_tag(block_state_tag: &Dynamic) -> Dynamic {
    let Some(entries) = block_state_tag.as_map_opt().result() else {
        return block_state_tag.clone();
    };
    let mut mapped: Vec<(Dynamic, Dynamic)> = Vec::with_capacity(entries.len());
    for (key_dynamic, value) in entries {
        let key = key_dynamic.as_string_or("");
        let mut new_value = None;
        if BOOLEAN_BLOCK_STATE_PROPERTIES.contains(&key.as_lossy_str())
            && let Some(bool_value) = value.as_boolean().result()
        {
            new_value = Some(value.create_string(bool_value.to_string()));
        }
        let new_value = match new_value {
            Some(v) => v,
            None => match value.as_number().result() {
                Some(number) => value.create_string(number_to_string(&number)),
                None => value,
            },
        };
        // `Collectors.toMap(Pair::getFirst, ..)` has no merge function
        if let Some((_, old)) = mapped.iter().find(|(k, _)| *k == key_dynamic) {
            panic!(
                "java.lang.IllegalStateException: Duplicate key {key_dynamic} (attempted merging values {old} and {new_value})"
            );
        }
        mapped.push((key_dynamic, new_value));
    }
    block_state_tag.create_map(hash_map_ordered(mapped, |(key, _)| key.hash_code(), 16))
}

fn fix_display(item_stack: &mut ItemStackData, mut display: Dynamic, hide_flags: i32) -> Dynamic {
    if let Some(name) = display.get("Name").result().filter(is_strictly_valid_json) {
        item_stack.set_component("minecraft:custom_name", name);
    }

    let lore = display.get("Lore");
    if lore.is_present() {
        item_stack.set_component(
            "minecraft:lore",
            display.create_list(
                display
                    .get("Lore")
                    .as_stream()
                    .into_iter()
                    .filter(is_strictly_valid_json),
            ),
        );
    }

    let color = display
        .get("color")
        .as_number()
        .result()
        .map(|number| number.int_value());
    let hide_dye = (hide_flags & 64) != 0;
    if color.is_some() || hide_dye {
        let mut dyed_color = display
            .empty_map()
            .set("rgb", display.create_int(color.unwrap_or(10511680)));
        if hide_dye {
            dyed_color = dyed_color.set("show_in_tooltip", display.create_boolean(false));
        }

        item_stack.set_component("minecraft:dyed_color", dyed_color);
    }

    if let Some(loc_name) = display.get("LocName").as_string().result() {
        item_stack.set_component(
            "minecraft:item_name",
            create_translatable_component(display.ops(), loc_name),
        );
    }

    if item_stack.is("minecraft:filled_map") {
        item_stack.set_component_opt("minecraft:map_color", display.get("MapColor"));
        display = display.remove("MapColor");
    }

    display
        .remove("Name")
        .remove("Lore")
        .remove("color")
        .remove("LocName")
}

fn fix_block_entity_tag(
    item_stack: &mut ItemStackData,
    mut block_entity: Dynamic,
    id: &str,
) -> Dynamic {
    item_stack.set_component_opt("minecraft:lock", block_entity.get("Lock"));
    block_entity = block_entity.remove("Lock");
    if let Some(loot_table) = block_entity.get("LootTable").result() {
        let mut container_loot = block_entity.empty_map().set("loot_table", loot_table);
        let seed = block_entity.get("LootTableSeed").as_long_or(0);
        if seed != 0 {
            container_loot = container_loot.set("seed", block_entity.create_long(seed));
        }

        item_stack.set_component("minecraft:container_loot", container_loot);
        block_entity = block_entity.remove("LootTable").remove("LootTableSeed");
    }
    match id {
        "minecraft:skull" => {
            item_stack.set_component_opt(
                "minecraft:note_block_sound",
                block_entity.get("note_block_sound"),
            );
            block_entity.remove("note_block_sound")
        }
        "minecraft:decorated_pot" => {
            item_stack.set_component_opt("minecraft:pot_decorations", block_entity.get("sherds"));
            if let Some(item) = block_entity.get("item").result() {
                item_stack.set_component(
                    "minecraft:container",
                    block_entity.create_list([block_entity
                        .empty_map()
                        .set("slot", block_entity.create_int(0))
                        .set("item", item)]),
                );
            }

            block_entity.remove("sherds").remove("item")
        }
        "minecraft:banner" => {
            item_stack.set_component_opt("minecraft:banner_patterns", block_entity.get("patterns"));
            if let Some(base) = block_entity.get("Base").as_number().result() {
                item_stack.set_component(
                    "minecraft:base_color",
                    block_entity.create_string(dye_color_id_to_name(base.int_value())),
                );
            }

            block_entity.remove("patterns").remove("Base")
        }
        "minecraft:shulker_box"
        | "minecraft:chest"
        | "minecraft:trapped_chest"
        | "minecraft:furnace"
        | "minecraft:ender_chest"
        | "minecraft:dispenser"
        | "minecraft:dropper"
        | "minecraft:brewing_stand"
        | "minecraft:hopper"
        | "minecraft:barrel"
        | "minecraft:smoker"
        | "minecraft:blast_furnace"
        | "minecraft:campfire"
        | "minecraft:chiseled_bookshelf"
        | "minecraft:crafter" => {
            let items: Vec<Dynamic> = block_entity.get("Items").as_list(|dynamic| {
                dynamic
                    .empty_map()
                    .set(
                        "slot",
                        dynamic.create_int(i32::from(dynamic.get("Slot").as_byte_or(0) as u8)),
                    )
                    .set("item", dynamic.remove("Slot"))
            });
            if !items.is_empty() {
                item_stack.set_component("minecraft:container", block_entity.create_list(items));
            }

            block_entity.remove("Items")
        }
        "minecraft:beehive" => {
            item_stack.set_component_opt("minecraft:bees", block_entity.get("bees"));
            block_entity.remove("bees")
        }
        _ => block_entity,
    }
}

fn fix_enchantments(
    item_stack: &mut ItemStackData,
    dynamic: &Dynamic,
    key: &str,
    component_type: &str,
    hide_in_tooltip: bool,
) {
    let raw_enchantments = item_stack.remove_tag(key);
    let enchantments: Vec<(String, i32)> = raw_enchantments
        .as_list(|enchantment| enchantment)
        .iter()
        .filter_map(parse_enchantment)
        .filter(|enchantment| enchantment.1 > 0)
        .collect();
    if !enchantments.is_empty() || hide_in_tooltip {
        let mut component = dynamic.empty_map();
        let mut levels = dynamic.empty_map();

        for (name, level) in &enchantments {
            levels = levels.set(name, dynamic.create_int(*level));
        }

        component = component.set("levels", levels);
        if hide_in_tooltip {
            component = component.set("show_in_tooltip", dynamic.create_boolean(false));
        }

        item_stack.set_component(component_type, component);
    }

    if raw_enchantments.is_present() && enchantments.is_empty() {
        item_stack.set_component(
            "minecraft:enchantment_glint_override",
            dynamic.create_boolean(true),
        );
    }
}

fn parse_enchantment(entry: &Dynamic) -> Option<(String, i32)> {
    let id = entry.get("id").as_string_lossy();
    let level = entry.get("lvl").as_number();
    match (id.result(), level.result()) {
        // `Mth.clamp(level, 0, 255)`
        (Some(id), Some(level)) => Some((id, level.int_value().clamp(0, 255))),
        _ => None,
    }
}

fn fix_adventure_mode_checks(item_stack: &mut ItemStackData, dynamic: &Dynamic, hide_flags: i32) {
    fix_block_state_predicates(
        item_stack,
        dynamic,
        "CanDestroy",
        "minecraft:can_break",
        (hide_flags & 8) != 0,
    );
    fix_block_state_predicates(
        item_stack,
        dynamic,
        "CanPlaceOn",
        "minecraft:can_place_on",
        (hide_flags & 16) != 0,
    );
}

fn fix_block_state_predicates(
    item_stack: &mut ItemStackData,
    dynamic: &Dynamic,
    tag: &str,
    component_id: &str,
    hide_in_tooltip: bool,
) {
    if let Some(old_predicate) = item_stack.remove_tag(tag).result() {
        let predicates: Vec<Dynamic> = old_predicate
            .as_stream()
            .into_iter()
            .map(|value| match value.as_string().result() {
                Some(string) => fix_block_state_predicate(&value, string.as_lossy_str()),
                None => value,
            })
            .collect();
        let mut component = dynamic
            .empty_map()
            .set("predicates", dynamic.create_list(predicates));
        if hide_in_tooltip {
            component = component.set("show_in_tooltip", dynamic.create_boolean(false));
        }

        item_stack.set_component(component_id, component);
    }
}

fn fix_block_state_predicate(dynamic: &Dynamic, string: &str) -> Dynamic {
    let start_properties = string.find('[');
    let start_nbt = string.find('{');
    let mut block_name_end = string.len();
    if let Some(start_properties) = start_properties {
        block_name_end = start_properties;
    }

    if let Some(start_nbt) = start_nbt {
        block_name_end = block_name_end.min(start_nbt);
    }

    let block_or_tag_name = &string[..block_name_end];
    let mut predicate = dynamic.empty_map().set(
        "blocks",
        dynamic.create_string(java_trim(block_or_tag_name)),
    );
    let end_properties = string.find(']');
    if let (Some(start_properties), Some(end_properties)) = (start_properties, end_properties) {
        let mut properties = dynamic.empty_map();

        // `Splitter.on(',')`: no trimming, empty parts kept
        for property in substring(string, start_properties + 1, end_properties).split(',') {
            if let Some(assignment) = property.find('=') {
                let key = java_trim(&property[..assignment]);
                let value = java_trim(&property[assignment + 1..]);
                properties = properties.set(key, dynamic.create_string(value));
            }
        }

        predicate = predicate.set("state", properties);
    }

    let end_nbt = string.find('}');
    if let (Some(start_nbt), Some(end_nbt)) = (start_nbt, end_nbt) {
        predicate = predicate.set(
            "nbt",
            dynamic.create_string(substring(string, start_nbt, end_nbt + 1)),
        );
    }

    predicate
}

fn fix_attribute_modifiers(item_stack: &mut ItemStackData, dynamic: &Dynamic, hide_flags: i32) {
    let attribute_modifiers_field = item_stack.remove_tag("AttributeModifiers");
    if attribute_modifiers_field.is_present() {
        let hide_in_tooltip = (hide_flags & 2) != 0;
        let attribute_modifiers = attribute_modifiers_field.as_list(fix_attribute_modifier);
        let mut component = dynamic
            .empty_map()
            .set("modifiers", dynamic.create_list(attribute_modifiers));
        if hide_in_tooltip {
            component = component.set("show_in_tooltip", dynamic.create_boolean(false));
        }

        item_stack.set_component("minecraft:attribute_modifiers", component);
    }
}

fn fix_attribute_modifier(input: Dynamic) -> Dynamic {
    let mut result = input
        .empty_map()
        .set("name", input.create_string(""))
        .set("amount", input.create_double(0.0))
        .set("operation", input.create_string("add_value"));
    result = Dynamic::copy_field(&input, "AttributeName", &result, "type");
    result = Dynamic::copy_field(&input, "Slot", &result, "slot");
    result = Dynamic::copy_field(&input, "UUID", &result, "uuid");
    result = Dynamic::copy_field(&input, "Name", &result, "name");
    result = Dynamic::copy_field(&input, "Amount", &result, "amount");
    Dynamic::copy_and_fix_field(&input, "Operation", &result, "operation", |operation| {
        let name = match operation.as_int_or(0) {
            1 => "add_multiplied_base",
            2 => "add_multiplied_total",
            _ => "add_value",
        };
        operation.create_string(name)
    })
}

fn fix_map_decoration(decoration: Dynamic) -> (Dynamic, Dynamic) {
    let id = decoration
        .get("id")
        .result()
        .unwrap_or_else(|| decoration.create_string(""));
    let value = decoration
        .empty_map()
        .set(
            "type",
            decoration.create_string(fix_map_decoration_type(decoration.get("type").as_int_or(0))),
        )
        .set(
            "x",
            decoration.create_double(decoration.get("x").as_double_or(0.0)),
        )
        .set(
            "z",
            decoration.create_double(decoration.get("z").as_double_or(0.0)),
        )
        .set(
            "rotation",
            decoration.create_float(decoration.get("rot").as_double_or(0.0) as f32),
        );
    (id, value)
}

fn fix_map_decoration_type(id: i32) -> &'static str {
    match id {
        1 => "frame",
        2 => "red_marker",
        3 => "blue_marker",
        4 => "target_x",
        5 => "target_point",
        6 => "player_off_map",
        7 => "player_off_limits",
        8 => "mansion",
        9 => "monument",
        10 => "banner_white",
        11 => "banner_orange",
        12 => "banner_magenta",
        13 => "banner_light_blue",
        14 => "banner_yellow",
        15 => "banner_lime",
        16 => "banner_pink",
        17 => "banner_gray",
        18 => "banner_light_gray",
        19 => "banner_cyan",
        20 => "banner_purple",
        21 => "banner_blue",
        22 => "banner_brown",
        23 => "banner_green",
        24 => "banner_red",
        25 => "banner_black",
        26 => "red_x",
        27 => "village_desert",
        28 => "village_plains",
        29 => "village_savanna",
        30 => "village_snowy",
        31 => "village_taiga",
        32 => "jungle_temple",
        33 => "swamp_hut",
        _ => "player",
    }
}

fn fix_potion_contents(item_stack: &mut ItemStackData, dynamic: &Dynamic) {
    let mut component = dynamic.empty_map();
    let potion = item_stack
        .remove_tag("Potion")
        .as_string()
        .result()
        .filter(|id| id != "minecraft:empty");
    if let Some(potion) = potion {
        component = component.set("potion", dynamic.create_string(potion));
    }

    component = item_stack.move_tag_into("CustomPotionColor", component, "custom_color");
    component = item_stack.move_tag_into("custom_potion_effects", component, "custom_effects");
    if component != dynamic.empty_map() {
        item_stack.set_component("minecraft:potion_contents", component);
    }
}

fn fix_writable_book(item_stack: &mut ItemStackData, dynamic: &Dynamic) {
    if let Some(pages) = fix_book_pages(item_stack, dynamic) {
        item_stack.set_component(
            "minecraft:writable_book_content",
            dynamic.empty_map().set("pages", pages),
        );
    }
}

fn fix_written_book(item_stack: &mut ItemStackData, dynamic: &Dynamic) {
    let pages = fix_book_pages(item_stack, dynamic);
    let title = item_stack.remove_tag("title").as_string_or("");
    let filtered_title = item_stack.remove_tag("filtered_title").as_string().result();
    let mut component = dynamic.empty_map();
    component = component.set(
        "title",
        create_filtered_text(dynamic, &title, filtered_title),
    );
    component = item_stack.move_tag_into("author", component, "author");
    component = item_stack.move_tag_into("resolved", component, "resolved");
    component = item_stack.move_tag_into("generation", component, "generation");
    if let Some(pages) = pages {
        component = component.set("pages", pages);
    }

    item_stack.set_component("minecraft:written_book_content", component);
}

fn fix_book_pages(item_stack: &mut ItemStackData, dynamic: &Dynamic) -> Option<Dynamic> {
    let pages: Vec<JavaString> = item_stack
        .remove_tag("pages")
        .as_list(|page| page.as_string_or(""));
    let filtered_pages: Vec<(JavaString, JavaString)> = item_stack
        .remove_tag("filtered_pages")
        .as_map(|key| key.as_string_or("0"), |page| page.as_string_or(""));
    if pages.is_empty() {
        None
    } else {
        let mut fixed_pages: Vec<Dynamic> = Vec::with_capacity(pages.len());

        for (i, page) in pages.iter().enumerate() {
            let index = i.to_string();
            let filtered_page = filtered_pages
                .iter()
                .find(|(key, _)| *key == index)
                .map(|(_, value)| value.clone());
            fixed_pages.push(create_filtered_text(dynamic, page, filtered_page));
        }

        Some(dynamic.create_list(fixed_pages))
    }
}

fn create_filtered_text(
    dynamic: &Dynamic,
    text: &JavaString,
    filtered: Option<JavaString>,
) -> Dynamic {
    let mut fixed_page = dynamic
        .empty_map()
        .set("raw", dynamic.create_string(text.clone()));
    if let Some(filtered) = filtered {
        fixed_page = fixed_page.set("filtered", dynamic.create_string(filtered));
    }

    fixed_page
}

fn fix_bucketed_mob_data(item_stack: &mut ItemStackData, dynamic: &Dynamic) {
    let mut data = dynamic.empty_map();

    for key in BUCKETED_MOB_TAGS {
        data = item_stack.move_tag_into(key, data, key);
    }

    if data != dynamic.empty_map() {
        item_stack.set_component("minecraft:bucket_entity_data", data);
    }
}

fn fix_lodestone_tracker(item_stack: &mut ItemStackData, dynamic: &Dynamic) {
    let lodestone_pos = item_stack.remove_tag("LodestonePos").result();
    let lodestone_dimension = item_stack.remove_tag("LodestoneDimension").result();
    if lodestone_pos.is_some() || lodestone_dimension.is_some() {
        let lodestone_tracked = item_stack
            .remove_tag("LodestoneTracked")
            .as_boolean_or(true);
        let mut component = dynamic.empty_map();
        if let (Some(pos), Some(dimension)) = (lodestone_pos, lodestone_dimension) {
            component = component.set(
                "target",
                dynamic
                    .empty_map()
                    .set("pos", pos)
                    .set("dimension", dimension),
            );
        }

        if !lodestone_tracked {
            component = component.set("tracked", dynamic.create_boolean(false));
        }

        item_stack.set_component("minecraft:lodestone_tracker", component);
    }
}

fn fix_firework_star(item_stack: &mut ItemStackData) {
    item_stack.fix_sub_tag("Explosion", true, |item_stack, explosion| {
        item_stack.set_component(
            "minecraft:firework_explosion",
            fix_firework_explosion(explosion.clone()),
        );
        explosion
            .remove("Type")
            .remove("Colors")
            .remove("FadeColors")
            .remove("Trail")
            .remove("Flicker")
    });
}

fn fix_firework_rocket(item_stack: &mut ItemStackData) {
    item_stack.fix_sub_tag("Fireworks", true, |item_stack, fireworks| {
        let explosions: Vec<Dynamic> = fireworks
            .get("Explosions")
            .as_stream()
            .into_iter()
            .map(fix_firework_explosion)
            .collect();
        let flight = fireworks.get("Flight").as_int_or(0);
        item_stack.set_component(
            "minecraft:fireworks",
            fireworks
                .empty_map()
                .set("explosions", fireworks.create_list(explosions))
                .set("flight_duration", fireworks.create_byte(flight as i8)),
        );
        fireworks.remove("Explosions").remove("Flight")
    });
}

fn fix_firework_explosion(mut explosion: Dynamic) -> Dynamic {
    let shape = match explosion.get("Type").as_int_or(0) {
        1 => "large_ball",
        2 => "star",
        3 => "creeper",
        4 => "burst",
        _ => "small_ball",
    };
    explosion = explosion
        .set("shape", explosion.create_string(shape))
        .remove("Type");
    explosion = explosion.rename_field("Colors", "colors");
    explosion = explosion.rename_field("FadeColors", "fade_colors");
    explosion = explosion.rename_field("Trail", "has_trail");
    explosion.rename_field("Flicker", "has_twinkle")
}

/// `ItemStackComponentizationFix.fixProfile(dynamic)`.
pub fn fix_profile(dynamic: &Dynamic) -> Dynamic {
    if let Some(simple_name) = dynamic.as_string().result() {
        if is_valid_player_name(simple_name.as_lossy_str()) {
            dynamic
                .empty_map()
                .set("name", dynamic.create_string(simple_name))
        } else {
            dynamic.empty_map()
        }
    } else {
        let name = dynamic.get("Name").as_string_or("");
        let id = dynamic.get("Id").result();
        let properties = fix_profile_properties(&dynamic.get("Properties"));
        let mut profile = dynamic.empty_map();
        if is_valid_player_name(name.as_lossy_str()) {
            profile = profile.set("name", dynamic.create_string(name));
        }

        if let Some(id) = id {
            profile = profile.set("id", id);
        }

        if let Some(properties) = properties {
            profile = profile.set("properties", properties);
        }

        profile
    }
}

/// `name.length() <= 16` and every UTF-16 unit in `33..127`.
fn is_valid_player_name(name: &str) -> bool {
    name.encode_utf16().count() <= 16 && name.encode_utf16().all(|c| (33..127).contains(&c))
}

fn fix_profile_properties(dynamic: &OptionalDynamic) -> Option<Dynamic> {
    let properties: ProfileProperties = dynamic.as_map(
        |key| key.as_string_or(""),
        |list| {
            list.as_list(|property| {
                let value = property.get("Value").as_string_or("");
                let signature = property.get("Signature").as_string().result();
                (value, signature)
            })
        },
    );
    if properties.is_empty() {
        None
    } else {
        let mut list: Vec<Dynamic> = Vec::new();
        for (key, entries) in &properties {
            for (value, signature) in entries {
                let property = dynamic
                    .empty_map()
                    .set("name", dynamic.create_string(key.clone()))
                    .set("value", dynamic.create_string(value.clone()));
                list.push(match signature {
                    Some(signature) => {
                        property.set("signature", dynamic.create_string(signature.clone()))
                    }
                    None => property,
                });
            }
        }
        Some(dynamic.create_list(list))
    }
}

/// `ItemStackComponentizationFix.ItemStackData`.
struct ItemStackData {
    item: String,
    count: i32,
    components: Dynamic,
    remainder: Dynamic,
    tag: Dynamic,
}

impl ItemStackData {
    fn new(item: &str, count: i32, remainder: &Dynamic) -> ItemStackData {
        ItemStackData {
            item: ensure_namespaced(item),
            count,
            components: remainder.empty_map(),
            tag: remainder.get("tag").or_else_empty_map(),
            remainder: remainder.remove("tag"),
        }
    }

    fn read(dynamic: &Dynamic) -> Option<ItemStackData> {
        let item = dynamic.get("id").as_string();
        let count = dynamic.get("Count").as_number();
        match (item.result(), count.result()) {
            (Some(item), Some(count)) => Some(ItemStackData::new(
                item.as_lossy_str(),
                count.int_value(),
                &dynamic.remove("id").remove("Count"),
            )),
            _ => None,
        }
    }

    fn remove_tag(&mut self, key: &str) -> OptionalDynamic {
        let value = self.tag.get(key);
        self.tag = self.tag.remove(key);
        value
    }

    fn set_component(&mut self, ty: &str, value: Dynamic) {
        self.components = self.components.set(ty, value);
    }

    /// The `setComponent(String, OptionalDynamic)` overload.
    fn set_component_opt(&mut self, ty: &str, optional_value: OptionalDynamic) {
        if let Some(value) = optional_value.result() {
            self.components = self.components.set(ty, value);
        }
    }

    fn move_tag_into(&mut self, from_key: &str, target: Dynamic, to_key: &str) -> Dynamic {
        match self.remove_tag(from_key).result() {
            Some(value) => target.set(to_key, value),
            None => target,
        }
    }

    /// The `moveTagToComponent(key, type, defaultValue)` overload.
    fn move_tag_to_component_default(&mut self, key: &str, ty: &str, default_value: &Dynamic) {
        if let Some(value) = self.remove_tag(key).result()
            && value != *default_value
        {
            self.set_component(ty, value);
        }
    }

    fn move_tag_to_component(&mut self, key: &str, ty: &str) {
        if let Some(value) = self.remove_tag(key).result() {
            self.set_component(ty, value);
        }
    }

    fn fix_sub_tag(
        &mut self,
        key: &str,
        dont_fix_when_field_is_missing: bool,
        function: impl FnOnce(&mut ItemStackData, Dynamic) -> Dynamic,
    ) {
        let value = self.tag.get(key);
        if !dont_fix_when_field_is_missing || value.is_present() {
            let map = value.or_else_empty_map();
            let map = function(self, map);
            if map == map.empty_map() {
                self.tag = self.tag.remove(key);
            } else {
                self.tag = self.tag.set(key, map);
            }
        }
    }

    fn write(mut self) -> Dynamic {
        let mut result = self
            .tag
            .empty_map()
            .set("id", self.tag.create_string(self.item.as_str()))
            .set("count", self.tag.create_int(self.count));
        if self.tag != self.tag.empty_map() {
            self.components = self
                .components
                .set("minecraft:custom_data", self.tag.clone());
        }

        if self.components != self.tag.empty_map() {
            result = result.set("components", self.components.clone());
        }

        merge_remainder(result, &self.remainder)
    }

    fn is(&self, id: &str) -> bool {
        self.item == id
    }

    /// The `is(Set<String>)` overload.
    fn is_one_of(&self, ids: &[&str]) -> bool {
        ids.contains(&self.item.as_str())
    }
}

/// `ItemStackData.mergeRemainder(itemStack, remainder)`: the remainder's entries, overridden by the item stack's.
fn merge_remainder(item_stack: Dynamic, remainder: &Dynamic) -> Dynamic {
    let ops = item_stack.ops();
    let merged = ops
        .get_map(item_stack.value())
        .flat_map(|item_stack_map| {
            ops.merge_to_map(remainder.convert(&ops).value(), item_stack_map.entries())
        })
        .map(|merged| Dynamic::new(ops, merged))
        .result();
    merged.unwrap_or(item_stack)
}
