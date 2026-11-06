//! `blockEntity.loadWithComponents(nbt)` followed by `saveWithFullMetadata()` for the block entity types that structure
//! templates carry NBT for: what a worldgen chunk ends up holding after `StructureTemplate.placeInWorld` configured a
//! block entity with the template's saved NBT. The data is read with the type's `loadAdditional` rules (typed reads with
//! defaults, unknown fields dropped, items only in their slot range) and written back with its `saveAdditional` rules
//! (`id`/`x`/`y`/`z`, the `components` compound the entity was loaded with, normalized numeric types, defaults omitted
//! where the codec omits them).
//!
//! Types that are not modelled return `None` (callers keep the raw NBT).

use super::block_entity_data::{base_tag, block_entity_type_of};
use crate::world::block_entity::create_block_entity;
use mc_core::pos::BlockPos;
use mc_data::block::BlockState;
use mc_nbt::{CompoundTag, ListTag, Tag};

/// The `loadWithComponents` + `saveWithFullMetadata` result for the block entity of `state` at `pos` loaded from `nbt`.
pub fn load_with_components(
    state: BlockState,
    pos: BlockPos,
    nbt: &CompoundTag,
) -> Option<CompoundTag> {
    let ty = block_entity_type_of(state.block())?;
    if let Some(mut block_entity) = create_block_entity(ty, pos, state) {
        block_entity.load_with_components(nbt);
        return Some(block_entity.save_with_full_metadata());
    }
    let id = ty.name();
    let mut out = base_tag(id, pos);
    if let Some(components) = nbt.get_compound("components") {
        out.put("components", components.clone());
    }
    match id {
        "minecraft:chest" | "minecraft:trapped_chest" | "minecraft:barrel" => {
            randomizable_container(nbt, &mut out, 27)
        }
        "minecraft:dispenser" | "minecraft:dropper" => randomizable_container(nbt, &mut out, 9),
        "minecraft:hopper" => {
            randomizable_container(nbt, &mut out, 5);
            out.put_int("TransferCooldown", nbt.get_int_or("TransferCooldown", -1));
        }
        "minecraft:furnace" | "minecraft:blast_furnace" | "minecraft:smoker" => {
            out.put_int(
                "cooking_time_spent",
                nbt.get_int_or("cooking_time_spent", 0),
            );
            out.put_int(
                "cooking_total_time",
                nbt.get_int_or("cooking_total_time", 0),
            );
            out.put_int(
                "lit_time_remaining",
                nbt.get_int_or("lit_time_remaining", 0),
            );
            out.put_int("lit_total_time", nbt.get_int_or("lit_total_time", 0));
            out.put_float(
                "speed_multiplier",
                nbt.get_float_or("speed_multiplier", 1.0),
            );
            out.put("Items", save_items(&load_items(nbt, 3)));
            let mut recipes = CompoundTag::new();
            for (key, value) in nbt.get_compound_or_empty("RecipesUsed").iter() {
                if let Ok(id) = mc_core::ident::Identifier::parse(key) {
                    recipes.put_int(id.to_string(), value_int(value));
                }
            }
            out.put("RecipesUsed", recipes);
        }
        "minecraft:brewing_stand" => {
            out.put_int("BrewTime", nbt.get_int_or("BrewTime", 0));
            out.put_int("total_brew_time", nbt.get_int_or("total_brew_time", 400));
            out.put("Items", save_items(&load_items(nbt, 5)));
            out.put_int("Fuel", nbt.get_int_or("Fuel", 0));
            out.put_int("total_fuel", nbt.get_int_or("total_fuel", 20));
            out.put_float(
                "speed_multiplier",
                nbt.get_float_or("speed_multiplier", 1.0),
            );
        }
        "minecraft:campfire" | "minecraft:soul_campfire" => {
            out.put("Items", save_items(&load_items(nbt, 4)));
            for key in ["CookingTimes", "CookingTotalTimes"] {
                let mut times = vec![0; 4];
                if let Some(read) = nbt.get_int_array(key) {
                    let n = read.len().min(4);
                    times[..n].copy_from_slice(&read[..n]);
                }
                out.put_int_array(key, times);
            }
        }
        "minecraft:comparator" => out.put_int("OutputSignal", nbt.get_int_or("OutputSignal", 0)),
        "minecraft:banner" => {
            if let Some(patterns) = nbt.get_list("patterns").filter(|p| !p.is_empty()) {
                out.put("patterns", patterns.clone());
            }
            if let Some(name) = nbt.get("CustomName") {
                out.put("CustomName", name.clone());
            }
        }
        "minecraft:decorated_pot" => {
            if let Some(sherds) = nbt.get_compound("sherds") {
                out.put("sherds", sherds.clone());
            }
            if !try_save_loot_table(nbt, &mut out) {
                keep_item(nbt, &mut out);
            }
        }
        "minecraft:brushable_block" => {
            if !try_save_loot_table(nbt, &mut out) {
                keep_item(nbt, &mut out);
            }
        }
        "minecraft:vault" => vault(nbt, &mut out),
        "minecraft:trial_spawner" => trial_spawner(nbt, &mut out),
        "minecraft:sign" | "minecraft:hanging_sign" => sign(nbt, &mut out)?,
        "minecraft:structure_block" => structure_block(nbt, &mut out),
        "minecraft:mob_spawner" => spawner(nbt, &mut out),
        // no saved fields of their own
        "minecraft:bell" | "minecraft:skull" | "minecraft:copper_golem_statue" => {}
        "minecraft:lectern" => {
            if let Some(book) = nbt.get_compound("Book") {
                out.put("Book", book.clone());
                out.put_int("Page", nbt.get_int_or("Page", 0));
            }
        }
        _ => return None,
    }
    Some(out)
}

fn value_int(tag: &Tag) -> i32 {
    let mut holder = CompoundTag::new();
    holder.put("v", tag.clone());
    holder.get_int_or("v", 0)
}

/// `RandomizableContainerBlockEntity`: `tryLoadLootTable` / `tryLoad ContainerHelper.loadAllItems`, then the matching save.
fn randomizable_container(nbt: &CompoundTag, out: &mut CompoundTag, size: usize) {
    if !try_save_loot_table(nbt, out) {
        out.put("Items", save_items(&load_items(nbt, size)));
    }
}

/// `RandomizableContainer.tryLoadLootTable` + `trySaveLootTable`: whether a loot table was saved.
fn try_save_loot_table(nbt: &CompoundTag, out: &mut CompoundTag) -> bool {
    let Some(table) = nbt
        .get_string("LootTable")
        .filter(|t| mc_core::ident::Identifier::parse(t).is_ok())
    else {
        return false;
    };
    out.put_string(
        "LootTable",
        mc_core::ident::Identifier::parse(table)
            .map(|i| i.to_string())
            .unwrap_or_default(),
    );
    let seed = nbt.get_long_or("LootTableSeed", 0);
    if seed != 0 {
        out.put_long("LootTableSeed", seed);
    }
    true
}

fn keep_item(nbt: &CompoundTag, out: &mut CompoundTag) {
    if let Some(item) = nbt
        .get_compound("item")
        .filter(|i| i.get_string("id").is_some_and(|id| id != "minecraft:air"))
    {
        out.put("item", normalize_stack(item));
    }
}

/// `ItemStack.CODEC` round trip of a valid stack: `id`, `count` (always written) and non-empty `components`.
fn normalize_stack(stack: &CompoundTag) -> CompoundTag {
    let mut out = CompoundTag::new();
    out.put_string("id", stack.get_string_or("id", "minecraft:air"));
    out.put_int("count", stack.get_int_or("count", 1));
    if let Some(components) = stack.get_compound("components").filter(|c| !c.is_empty()) {
        out.put("components", components.clone());
    }
    out
}

/// `ContainerHelper.loadAllItems`: `Items` entries with a slot inside the container (a later entry replaces an earlier one).
fn load_items(nbt: &CompoundTag, size: usize) -> Vec<Option<CompoundTag>> {
    let mut items: Vec<Option<CompoundTag>> = vec![None; size];
    for entry in nbt.get_list_or_empty("Items").compounds() {
        let Some(id) = entry.get_string("id") else {
            continue;
        };
        let slot = entry.get_byte_or("Slot", 0) as u8 as usize;
        if slot < size && id != "minecraft:air" {
            items[slot] = Some(entry.clone());
        }
    }
    items
}

/// `ContainerHelper.saveAllItems(output, items, true)`.
fn save_items(items: &[Option<CompoundTag>]) -> ListTag {
    let mut list = ListTag::new();
    for (slot, item) in items.iter().enumerate() {
        if let Some(item) = item {
            let mut tag = normalize_stack(item);
            tag.put_byte("Slot", slot as i8);
            list.push(tag);
        }
    }
    list
}

/// `VaultBlockEntity.saveAdditional` of the loaded config with default server/shared data.
fn vault(nbt: &CompoundTag, out: &mut CompoundTag) {
    let config = nbt.get_compound_or_empty("config");
    let mut saved = CompoundTag::new();
    if let Some(table) = config
        .get_string("loot_table")
        .filter(|t| mc_core::ident::Identifier::parse(t).is_ok())
    {
        let table = mc_core::ident::Identifier::parse(table)
            .map(|i| i.to_string())
            .unwrap_or_default();
        if table != "minecraft:chests/trial_chambers/reward" {
            saved.put_string("loot_table", table);
        }
    }
    let (activation, deactivation) = (
        config.get_double_or("activation_range", 4.0),
        config.get_double_or("deactivation_range", 4.5),
    );
    if activation != 4.0 {
        saved.put_double("activation_range", activation);
    }
    if deactivation != 4.5 {
        saved.put_double("deactivation_range", deactivation);
    }
    match config.get_compound("key_item") {
        Some(key) => {
            saved.put("key_item", normalize_stack(key));
        }
        None => {
            let mut key = CompoundTag::new();
            key.put_string("id", "minecraft:trial_key");
            key.put_int("count", 1);
            saved.put("key_item", key);
        }
    };
    if let Some(table) = config.get_string("override_loot_table_to_display") {
        saved.put_string("override_loot_table_to_display", table);
    }
    out.put("config", saved);
    out.put(
        "shared_data",
        nbt.get_compound("shared_data").cloned().unwrap_or_default(),
    );
    out.put(
        "server_data",
        nbt.get_compound("server_data").cloned().unwrap_or_default(),
    );
}

/// `TrialSpawnerBlockEntity.saveAdditional` (`TrialSpawner.store`): the configs and the non-default settings.
fn trial_spawner(nbt: &CompoundTag, out: &mut CompoundTag) {
    for key in ["normal_config", "ominous_config"] {
        if let Some(config) = nbt.get(key) {
            out.put(key, config.clone());
        }
    }
    if nbt.get_int_or("target_cooldown_length", 36000) != 36000 {
        out.put_int(
            "target_cooldown_length",
            nbt.get_int_or("target_cooldown_length", 36000),
        );
    }
    if nbt.get_int_or("required_player_range", 14) != 14 {
        out.put_int(
            "required_player_range",
            nbt.get_int_or("required_player_range", 14),
        );
    }
}

/// `SignBlockEntity.saveAdditional`: both texts through `SignText.CODEC` (text components re-encoded).
fn sign(nbt: &CompoundTag, out: &mut CompoundTag) -> Option<()> {
    if nbt.get_boolean_or("allow_op_features", false) {
        out.put_boolean("allow_op_features", true);
    }
    for key in ["front_text", "back_text"] {
        out.put(key, sign_text(nbt.get_compound(key))?);
    }
    out.put_boolean("is_waxed", nbt.get_boolean_or("is_waxed", false));
    Some(())
}

fn sign_text(text: Option<&CompoundTag>) -> Option<CompoundTag> {
    let empty = CompoundTag::new();
    let text = text.unwrap_or(&empty);
    let mut messages = ListTag::new();
    let read = text.get_list_or_empty("messages");
    for i in 0..4 {
        let component = match read.get(i) {
            Some(tag) => mc_text::Component::from_nbt(tag).ok()?,
            None => mc_text::Component::empty(),
        };
        messages.push(component.to_nbt().ok()?);
    }
    let mut out = CompoundTag::new();
    out.put("messages", messages);
    out.put_string("color", text.get_string_or("color", "black"));
    out.put_boolean(
        "has_glowing_text",
        text.get_boolean_or("has_glowing_text", false),
    );
    Some(out)
}

/// `StructureBlockEntity.saveAdditional` of the loaded values (offsets/sizes clamped like `loadAdditional`).
fn structure_block(nbt: &CompoundTag, out: &mut CompoundTag) {
    out.put_string("name", nbt.get_string_or("name", ""));
    out.put_string("author", nbt.get_string_or("author", ""));
    out.put_string("metadata", nbt.get_string_or("metadata", ""));
    for key in ["posX", "posY", "posZ"] {
        out.put_int(key, nbt.get_int_or(key, 0).clamp(-48, 48));
    }
    for (key, default) in [("sizeX", 0), ("sizeY", 0), ("sizeZ", 0)] {
        out.put_int(key, nbt.get_int_or(key, default).clamp(0, 48));
    }
    let name_of = |key: &str, valid: &[&str], default: &str| {
        let value = nbt.get_string_or(key, default);
        valid
            .iter()
            .find(|v| **v == value)
            .map_or(default, |v| *v)
            .to_owned()
    };
    out.put_string(
        "rotation",
        name_of(
            "rotation",
            &[
                "NONE",
                "CLOCKWISE_90",
                "CLOCKWISE_180",
                "COUNTERCLOCKWISE_90",
            ],
            "NONE",
        ),
    );
    out.put_string(
        "mirror",
        name_of("mirror", &["NONE", "LEFT_RIGHT", "FRONT_BACK"], "NONE"),
    );
    out.put_string(
        "mode",
        name_of("mode", &["SAVE", "LOAD", "CORNER", "DATA"], "DATA"),
    );
    out.put_boolean("ignoreEntities", nbt.get_boolean_or("ignoreEntities", true));
    out.put_boolean("strict", nbt.get_boolean_or("strict", false));
    out.put_boolean("powered", nbt.get_boolean_or("powered", false));
    out.put_boolean("showair", nbt.get_boolean_or("showair", false));
    out.put_boolean(
        "showboundingbox",
        nbt.get_boolean_or("showboundingbox", true),
    );
    out.put_float("integrity", nbt.get_float_or("integrity", 1.0));
    out.put_long("seed", nbt.get_long_or("seed", 0));
}

/// `SpawnerBlockEntity.saveAdditional` (`BaseSpawner.save`) of a loaded spawner.
fn spawner(nbt: &CompoundTag, out: &mut CompoundTag) {
    out.put_short("Delay", nbt.get_short_or("Delay", 20));
    for (key, default) in [
        ("MinSpawnDelay", 200),
        ("MaxSpawnDelay", 800),
        ("SpawnCount", 4),
        ("MaxNearbyEntities", 6),
        ("RequiredPlayerRange", 16),
        ("SpawnRange", 4),
    ] {
        out.put_short(key, nbt.get_int_or(key, default) as i16);
    }
    if let Some(data) = nbt.get_compound("SpawnData") {
        out.put("SpawnData", data.clone());
    }
    let potentials = nbt.get_list("SpawnPotentials").cloned().unwrap_or_else(|| {
        let mut list = ListTag::new();
        if let Some(data) = nbt.get_compound("SpawnData") {
            let mut entry = CompoundTag::new();
            entry.put("data", data.clone());
            list.push(entry);
        }
        list
    });
    out.put("SpawnPotentials", potentials);
}
