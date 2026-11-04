//! Golden tests for the `inventory` play packets against vanilla's `GameProtocols` (generator: fixtures/play/GenInventory.java).
mod common;
use common::*;
use std::fmt::Debug;

use mc_core::ident::Identifier;
use mc_core::pos::block_pos::BlockPos;
use mc_data::{DataComponentType, Item, MapDecorationType, MenuType, RecipeBookCategory};
use mc_protocol::codec::{from_bytes, Holder, HolderSet};
use mc_protocol::components::{
    DataComponentExactPredicate, DataComponentPatch, HashedPatchMap, HashedStack, ItemStack, ItemStackTemplate, TypedDataComponent, Value, DAMAGE, MAX_STACK_SIZE, UNBREAKABLE,
};
use mc_protocol::codec::read_varint;
use mc_protocol::packets::play::*;
use mc_protocol::{Decode, Packet};
use mc_text::{ChatFormatting, Component, TranslatableArg};

fn id(s: &str) -> Identifier {
    Identifier::parse(s).unwrap()
}

/// `bad_<label>` fails to decode with exactly the message vanilla's codec throws (`err_<label>`).
fn check_bad<P: Packet + Debug>(g: &Golden, label: &str) {
    let want = String::from_utf8(g.bytes(&format!("err_{label}")).to_vec()).unwrap();
    let mut input = g.bytes(&format!("bad_{label}"));
    assert_eq!(read_varint(&mut input).unwrap(), P::ID, "id {label}");
    let err = P::decode(&mut input).expect_err(label);
    assert_eq!(err.to_string(), want, "{label}");
}

/// `bad_<label>` decodes leniently and re-encodes to the bytes vanilla re-encodes (`norm_<label>`).
fn check_lenient<P: Packet + Debug>(g: &Golden, label: &str) {
    let mut input = g.bytes(&format!("bad_{label}"));
    assert_eq!(read_varint(&mut input).unwrap(), P::ID, "id {label}");
    let p = P::decode(&mut input).unwrap_or_else(|e| panic!("{label}: {e}"));
    assert!(input.is_empty(), "{label}: trailing bytes");
    let mut out = Vec::new();
    mc_protocol::codec::write_varint(&mut out, P::ID);
    p.encode(&mut out);
    assert_eq!(hex(&out), hex(&g.bytes(&format!("norm_{label}"))[..]), "{label}");
}

fn simple(g: &Golden) {
    for c in [0, 1, 127, 128, 255, 300, i32::MAX, -1, i32::MIN] {
        g.check(&format!("cb_container_close_{c}"), &ClientboundContainerClosePacket { container_id: c });
        g.check(&format!("sb_container_close_{c}"), &ServerboundContainerClosePacket { container_id: c });
    }
    let set_data = |c, id, value| ClientboundContainerSetDataPacket { container_id: c, id, value };
    g.check("cb_set_data_basic", &set_data(3, 0, 200));
    g.check("cb_set_data_negative", &set_data(-1, -32768, 32767));
    g.check("cb_set_data_big_container", &set_data(1000, 2, -1));
    let cd = |g_: &str, duration| ClientboundCooldownPacket { cooldown_group: id(g_), duration };
    g.check("cb_cooldown_basic", &cd("minecraft:ender_pearl", 20));
    g.check("cb_cooldown_zero", &cd("minecraft:shield", 0));
    g.check("cb_cooldown_custom_ns", &cd("example:some/group", i32::MAX));
    g.check("cb_cooldown_neg", &cd("minecraft:x", -5));
    g.check("cb_open_book_main", &ClientboundOpenBookPacket { hand: InteractionHand::MainHand });
    g.check("cb_open_book_off", &ClientboundOpenBookPacket { hand: InteractionHand::OffHand });
    for m in MenuType::iter() {
        let n = m.name().strip_prefix("minecraft:").unwrap();
        let title = Component::translatable(format!("container.{n}"), vec![]);
        g.check(&format!("cb_open_screen_{n}"), &ClientboundOpenScreenPacket { container_id: 7, menu_type: m, title });
    }
    let screen = |container_id, menu_type, title| ClientboundOpenScreenPacket { container_id, menu_type, title };
    g.check("cb_open_screen_literal", &screen(1, MenuType::GENERIC_9X3, Component::literal("My \u{1F600} chest")));
    g.check(
        "cb_open_screen_styled",
        &screen(
            255,
            MenuType::ANVIL,
            Component::literal("Repair")
                .with_formats(&[ChatFormatting::Red, ChatFormatting::Bold])
                .append(Component::literal(" & Name").with_color(0x12ab34)),
        ),
    );
    g.check("cb_open_screen_empty", &screen(0, MenuType::HOPPER, Component::empty()));
    g.check(
        "cb_open_screen_translatable_args",
        &screen(
            2,
            MenuType::MERCHANT,
            Component::translatable(
                "merchant.title",
                vec![
                    TranslatableArg::String("Villager".into()),
                    TranslatableArg::String("Master".into()),
                    TranslatableArg::Number(mc_text::Num::Int(5)),
                    TranslatableArg::Component(Component::literal("x").with_format(ChatFormatting::Red)),
                ],
            ),
        ),
    );
    for (name, slot) in [("BACK", SignTextSlot::Back), ("FRONT", SignTextSlot::Front)] {
        g.check(&format!("cb_open_sign_{name}"), &ClientboundOpenSignEditorPacket { pos: BlockPos::new(100, 64, -200), slot });
    }
    g.check(
        "cb_open_sign_extreme",
        &ClientboundOpenSignEditorPacket { pos: BlockPos::new(-33554432, -2048, 33554431), slot: SignTextSlot::Front },
    );
    for s in [0, 1, 8, 40, 127, 128, -1] {
        g.check(&format!("cb_set_held_slot_{s}"), &ClientboundSetHeldSlotPacket { slot: s });
    }
    use RecipeBookType::*;
    for (name, t) in [("CRAFTING", Crafting), ("FURNACE", Furnace), ("BLAST_FURNACE", BlastFurnace), ("SMOKER", Smoker)] {
        for (suffix, (is_open, is_filtering)) in [("00", (false, false)), ("10", (true, false)), ("01", (false, true)), ("11", (true, true))] {
            g.check(
                &format!("sb_recipe_book_change_{name}_{suffix}"),
                &ServerboundRecipeBookChangeSettingsPacket { book_type: t, is_open, is_filtering },
            );
        }
    }
    let ts = |open, filtering| RecipeBookSettingsTypeSettings { open, filtering };
    let mut settings = RecipeBookSettings::default();
    g.check("cb_recipe_book_settings_default", &ClientboundRecipeBookSettingsPacket { book_settings: settings });
    settings = RecipeBookSettings { crafting: ts(true, false), furnace: ts(false, true), blast_furnace: ts(true, true), smoker: ts(false, true) };
    g.check("cb_recipe_book_settings_mixed", &ClientboundRecipeBookSettingsPacket { book_settings: settings });
    settings = RecipeBookSettings { crafting: ts(true, true), furnace: ts(true, true), blast_furnace: ts(true, true), smoker: ts(true, true) };
    g.check("cb_recipe_book_settings_all", &ClientboundRecipeBookSettingsPacket { book_settings: settings });
    let rid = |index| RecipeDisplayId { index };
    g.check("cb_recipe_book_remove_empty", &ClientboundRecipeBookRemovePacket { recipes: vec![] });
    g.check("cb_recipe_book_remove_one", &ClientboundRecipeBookRemovePacket { recipes: vec![rid(0)] });
    g.check(
        "cb_recipe_book_remove_many",
        &ClientboundRecipeBookRemovePacket { recipes: [1, 127, 128, 16384, -1, i32::MAX].map(rid).to_vec() },
    );
    g.check("cb_recipe_book_remove_300", &ClientboundRecipeBookRemovePacket { recipes: (0..300).map(|i| rid(i * 7)).collect() });

    g.check("sb_bundle_selected_basic", &ServerboundSelectBundleItemPacket { slot_id: 36, selected_item_index: 2 });
    g.check("sb_bundle_selected_none", &ServerboundSelectBundleItemPacket { slot_id: 0, selected_item_index: -1 });
    g.check("sb_bundle_selected_big", &ServerboundSelectBundleItemPacket { slot_id: -1, selected_item_index: i32::MAX });
    g.check("sb_button_click", &ServerboundContainerButtonClickPacket { container_id: 3, button_id: 1 });
    g.check("sb_button_click_neg", &ServerboundContainerButtonClickPacket { container_id: 200, button_id: -1 });
    g.check("sb_slot_state_true", &ServerboundContainerSlotStateChangedPacket { slot_id: 5, container_id: 2, new_state: true });
    g.check("sb_slot_state_false", &ServerboundContainerSlotStateChangedPacket { slot_id: -1, container_id: 300, new_state: false });
    let place = |container_id, recipe, use_max_items| ServerboundPlaceRecipePacket { container_id, recipe: rid(recipe), use_max_items };
    g.check("sb_place_recipe_max", &place(1, 42, true));
    g.check("sb_place_recipe_one", &place(0, 0, false));
    g.check("sb_place_recipe_big", &place(255, 100000, true));
    g.check("sb_seen_recipe_0", &ServerboundRecipeBookSeenRecipePacket { recipe: rid(0) });
    g.check("sb_seen_recipe_big", &ServerboundRecipeBookSeenRecipePacket { recipe: rid(123456) });
    let rename = |s: &str| ServerboundRenameItemPacket { name: s.into() };
    g.check("sb_rename_empty", &rename(""));
    g.check("sb_rename_basic", &rename("Excalibur"));
    g.check("sb_rename_non_bmp", &rename("Sw\u{f6}rd \u{1F5E1}\u{FE0F} \u{4e2d}\u{6587} \u{1F600}"));
    g.check("sb_rename_long", &rename(&"x".repeat(50)));
    for (label, item) in [("0", 0), ("7", 7), ("neg", -1)] {
        g.check(&format!("sb_select_trade_{label}"), &ServerboundSelectTradePacket { item });
    }
    for s in [0, 8, 255, 32767, -1, -32768] {
        g.check(&format!("sb_set_carried_{s}"), &ServerboundSetCarriedItemPacket { slot: s });
    }
    let book = |slot, pages: &[&str], title: Option<&str>| ServerboundEditBookPacket {
        slot,
        pages: pages.iter().map(|p| p.to_string()).collect(),
        title: title.map(str::to_string),
    };
    g.check("sb_edit_book_no_pages", &book(0, &[], None));
    g.check("sb_edit_book_one_page", &book(36, &["Hello"], None));
    g.check("sb_edit_book_sign", &book(40, &["Page 1", "Page \u{1F600} 2", ""], Some("My Title")));
    g.check("sb_edit_book_max_title", &book(-1, &["a"], Some(&"t".repeat(32))));
    g.check("sb_edit_book_max_page", &book(1, &[&"p".repeat(1024)], Some("")));
    let pages: Vec<String> = (0..100).map(|i| format!("page {i}")).collect();
    let refs: Vec<&str> = pages.iter().map(String::as_str).collect();
    g.check("sb_edit_book_100_pages", &book(2, &refs, Some(&"\u{1F600}".repeat(16))));
}

fn maps(g: &Golden) {
    let map = |id, scale, locked, decorations, color_patch| ClientboundMapItemDataPacket {
        map_id: MapId { id },
        scale,
        locked,
        decorations,
        color_patch,
    };
    g.check("cb_map_minimal", &map(0, 0, false, None, None));
    g.check("cb_map_locked_scale", &map(1234567, 4, true, None, None));
    g.check("cb_map_neg_scale", &map(-1, -128, true, Some(vec![]), None));
    let decos: Vec<MapDecoration> = MapDecorationType::iter()
        .enumerate()
        .map(|(i, t)| {
            let i = i as i32;
            MapDecoration {
                decoration_type: t,
                x: (i * 5 - 100) as i8,
                y: (100 - i * 4) as i8,
                rot: (i as i8) & 15,
                name: (i % 3 == 0).then(|| Component::literal(format!("n{i}"))),
            }
        })
        .collect();
    g.check("cb_map_all_decorations", &map(5, 2, false, Some(decos.clone()), None));
    let deco = |decoration_type, x, y, rot, name| MapDecoration { decoration_type, x, y, rot, name };
    g.check(
        "cb_map_decoration_rot_masked",
        &map(
            5,
            2,
            false,
            Some(vec![deco(
                MapDecorationType::PLAYER,
                127,
                -128,
                15,
                Some(Component::translatable("filled_map.id", vec![TranslatableArg::Number(mc_text::Num::Int(3))]).with_format(ChatFormatting::Gold)),
            )]),
            None,
        ),
    );
    g.check("cb_map_decoration_neg_rot", &map(5, 2, false, Some(vec![deco(MapDecorationType::FRAME, 0, 0, 15, None)]), None));
    let patch = |start_x, start_y, width, height, map_colors| MapItemSavedDataMapPatch { start_x, start_y, width, height, map_colors };
    let colors: Vec<u8> = (0..128 * 128).map(|k: i32| (k * 31) as u8).collect();
    g.check("cb_map_full_patch", &map(9, 1, false, None, Some(patch(0, 0, 128, 128, colors))));
    g.check("cb_map_small_patch", &map(9, 1, true, None, Some(patch(17, 99, 3, 2, vec![1, 2, 3, 252, 251, 250]))));
    g.check("cb_map_patch_extreme", &map(9, 1, true, Some(vec![]), Some(patch(255, 254, 255, 1, vec![0; 255]))));
    g.check("cb_map_everything", &map(77, 3, true, Some(decos), Some(patch(1, 2, 4, 4, vec![0; 16]))));
}

fn limits(g: &Golden) {
    for label in ["edit_book_101_pages", "edit_book_page_1025", "edit_book_page_1024_utf16_over", "edit_book_title_33", "edit_book_neg_pages"] {
        check_bad::<ServerboundEditBookPacket>(g, label);
    }
    check_lenient::<ServerboundEditBookPacket>(g, "edit_book_max");
    check_bad::<ServerboundRenameItemPacket>(g, "rename_32768");
    check_lenient::<ServerboundRenameItemPacket>(g, "rename_32767");
    check_bad::<ServerboundSelectBundleItemPacket>(g, "bundle_selected_minus2");
    check_bad::<ServerboundSelectBundleItemPacket>(g, "bundle_selected_min");
    for v in [4, 5, 100, -1, i32::MIN] {
        check_bad::<ServerboundRecipeBookChangeSettingsPacket>(g, &format!("recipe_book_type_{v}"));
    }
    let menus = MenuType::iter().len() as i32;
    for v in [menus, menus + 1, 1000, -1] {
        check_bad::<ClientboundOpenScreenPacket>(g, &format!("open_screen_menu_{v}"));
    }
    for v in [2, 3, -1, 1000] {
        check_lenient::<ClientboundOpenSignEditorPacket>(g, &format!("open_sign_slot_{v}"));
    }
    check_bad::<ClientboundMapItemDataPacket>(g, &format!("map_decoration_type_{}", MapDecorationType::iter().len()));
    for rot in [16, 31, 0x7f, -1, -128] {
        check_lenient::<ClientboundMapItemDataPacket>(g, &format!("map_decoration_rot_{rot}"));
    }
    check_lenient::<ClientboundMapItemDataPacket>(g, "map_patch_zero_width");
    check_bad::<ClientboundMapItemDataPacket>(g, "map_patch_array_too_long");
}

#[test]
fn inventory_packets() {
    let g = Golden::load("inventory");
    simple(&g);
    maps(&g);
    limits(&g);
    items(&g);
    recipes(&g);
    merchant(&g);
    item_limits(&g);
    g.assert_all_used();
}

fn stack(item: Item, count: i32, patch: DataComponentPatch) -> ItemStack {
    ItemStack::with_patch(item, count, &patch)
}

/// Mirrors `GenInventory.stacks()`.
fn stacks() -> Vec<(&'static str, ItemStack)> {
    let b = DataComponentPatch::builder;
    let plain = |i, n| stack(i, n, DataComponentPatch::empty());
    vec![
        ("empty", ItemStack::empty()),
        ("diamond", ItemStack::new(Item::DIAMOND)),
        ("dirt64", plain(Item::DIRT, 64)),
        ("cobble99", plain(Item::COBBLESTONE, 99)),
        ("named", stack(Item::DIAMOND_SWORD, 1, b().set(UNBREAKABLE, ()).build())),
        ("damaged", stack(Item::IRON_PICKAXE, 1, b().set(DAMAGE, 17).build())),
        ("removed", stack(Item::DIAMOND_PICKAXE, 1, b().remove(DataComponentType::MAX_DAMAGE).build())),
        (
            "set_and_removed",
            stack(Item::NETHERITE_AXE, 1, b().set(DAMAGE, 3).remove(DataComponentType::MAX_DAMAGE).set(UNBREAKABLE, ()).build()),
        ),
        ("max_stack_override", stack(Item::STICK, 5, b().set(MAX_STACK_SIZE, 16).build())),
    ]
}

fn items(g: &Golden) {
    let st = stacks();
    let get = |n: &str| st.iter().find(|(k, _)| *k == n).unwrap().1.clone();
    for (name, s) in &st {
        g.check(&format!("cb_set_slot_{name}"), &ClientboundContainerSetSlotPacket { container_id: 2, state_id: 17, slot: 5, item_stack: s.clone() });
        g.check(&format!("cb_set_cursor_{name}"), &ClientboundSetCursorItemPacket { contents: s.clone() });
        g.check(&format!("cb_set_player_inventory_{name}"), &ClientboundSetPlayerInventoryPacket { slot: 40, contents: s.clone() });
        g.check(&format!("sb_creative_slot_{name}"), &ServerboundSetCreativeModeSlotPacket { slot_num: 36, item_stack: s.clone() });
    }
    let slot = |container_id, state_id, slot, item_stack| ClientboundContainerSetSlotPacket { container_id, state_id, slot, item_stack };
    g.check("cb_set_slot_extremes", &slot(-1, -1, -1, get("diamond")));
    g.check("cb_set_slot_big", &slot(300, 70000, 32767, get("named")));
    g.check("cb_set_slot_min", &slot(0, 0, -32768, get("empty")));
    g.check("cb_set_player_inventory_neg", &ClientboundSetPlayerInventoryPacket { slot: -1, contents: get("dirt64") });
    g.check("sb_creative_slot_neg", &ServerboundSetCreativeModeSlotPacket { slot_num: -1, item_stack: get("dirt64") });
    g.check("sb_creative_slot_max", &ServerboundSetCreativeModeSlotPacket { slot_num: 32767, item_stack: get("empty") });
    let content = |container_id, state_id, items, carried_item| ClientboundContainerSetContentPacket { container_id, state_id, items, carried_item };
    g.check("cb_set_content_empty_list", &content(0, 0, vec![], ItemStack::empty()));
    g.check("cb_set_content_all", &content(5, 123456, st.iter().map(|(_, s)| s.clone()).collect(), get("named")));
    let inv = (0..46).map(|i| if i % 3 == 0 { ItemStack::empty() } else { ItemStack::with_count(Item::DIRT, 1 + i) }).collect();
    g.check("cb_set_content_46", &content(0, 1, inv, ItemStack::with_count(Item::APPLE, 3)));
    let big = (0..300).map(|i| if i % 2 == 0 { ItemStack::empty() } else { ItemStack::new(Item::STONE) }).collect();
    g.check("cb_set_content_300", &content(255, -5, big, ItemStack::empty()));
    g.check("cb_cooldown_unused", &ClientboundCooldownPacket { cooldown_group: id("minecraft:diamond"), duration: 1 });

    // container click
    let no_patch = HashedPatchMap::default();
    let actual = |item, count, components| HashedStack::ActualItem { item, count, components };
    let patched = actual(
        Item::NETHERITE_SWORD,
        99,
        HashedPatchMap {
            added_components: vec![(DataComponentType::DAMAGE, 0x12345678), (DataComponentType::CUSTOM_NAME, -1)],
            removed_components: vec![DataComponentType::MAX_STACK_SIZE, DataComponentType::MAX_DAMAGE],
        },
    );
    let only_removed = actual(
        Item::STICK,
        2,
        HashedPatchMap { added_components: vec![], removed_components: vec![DataComponentType::MAX_STACK_SIZE, DataComponentType::MAX_DAMAGE] },
    );
    let hs = [
        ("empty", HashedStack::Empty),
        ("diamond", actual(Item::DIAMOND, 1, no_patch.clone())),
        ("dirt64", actual(Item::DIRT, 64, no_patch.clone())),
        ("patched", patched.clone()),
        ("only_removed", only_removed.clone()),
        ("neg_count", actual(Item::STICK, -3, no_patch.clone())),
    ];
    let h = |n: &str| hs.iter().find(|(k, _)| *k == n).unwrap().1.clone();
    let click = |container_id, state_id, slot_num, button_num, container_input, changed_slots, carried_item| ServerboundContainerClickPacket {
        container_id,
        state_id,
        slot_num,
        button_num,
        container_input,
        changed_slots,
        carried_item,
    };
    use ContainerInput::*;
    for (name, input) in [("PICKUP", Pickup), ("QUICK_MOVE", QuickMove), ("SWAP", Swap), ("CLONE", Clone), ("THROW", Throw), ("QUICK_CRAFT", QuickCraft), ("PICKUP_ALL", PickupAll)] {
        let mut slots = vec![];
        g.check(&format!("sb_click_{name}_no_changes"), &click(1, 2, 3, 0, input, slots.clone(), HashedStack::Empty));
        slots.push((7, h("dirt64")));
        g.check(&format!("sb_click_{name}_one_change"), &click(0, 99, -999, 1, input, slots.clone(), h("diamond")));
        slots.extend([(-1, HashedStack::Empty), (32767, patched.clone()), (-32768, only_removed.clone())]);
        g.check(&format!("sb_click_{name}_many_changes"), &click(255, -1, 32767, -128, input, slots, patched.clone()));
    }
    for (name, v) in &hs {
        g.check(&format!("sb_click_hashed_{name}"), &click(300, 70000, 1, 127, Pickup, vec![(1, v.clone())], v.clone()));
    }
    let max = (0..128).map(|i| (i * 3 - 100, h(if i % 2 == 0 { "diamond" } else { "empty" }))).collect();
    g.check("sb_click_128_changes", &click(1, 1, 0, 0, QuickCraft, max, HashedStack::Empty));
}

fn item(i: Item) -> SlotDisplay {
    SlotDisplay::Item { item: i }
}
fn boxed(d: SlotDisplay) -> Box<SlotDisplay> {
    Box::new(d)
}
fn direct(items: &[Item]) -> HolderSet {
    HolderSet::Direct(items.iter().map(|i| i.id() as i32).collect())
}
fn tag(name: &str) -> HolderSet {
    HolderSet::Named(id(name))
}
fn ingredient(items: &[Item]) -> Ingredient {
    Ingredient { values: direct(items) }
}

fn slot_displays(g: &Golden) -> Vec<(&'static str, SlotDisplay)> {
    let trim_id = from_bytes::<mc_protocol::codec::VarInt>(g.bytes("trim_sentry_id")).unwrap().0;
    let template = |item, count, patch| SlotDisplay::ItemStack { stack: ItemStackTemplate::new(item, count, patch).unwrap() };
    let mut m = vec![
        ("empty", SlotDisplay::Empty),
        ("any_fuel", SlotDisplay::AnyFuel),
        ("item", item(Item::DIAMOND)),
        ("item_air", item(Item::AIR)),
        ("item_stack", SlotDisplay::ItemStack { stack: ItemStackTemplate::of(Item::APPLE) }),
        ("item_stack_count", template(Item::STICK, 12, DataComponentPatch::empty())),
        ("item_stack_patch", template(Item::DIAMOND_SWORD, 3, DataComponentPatch::builder().set(DAMAGE, 9).remove(DataComponentType::MAX_DAMAGE).build())),
        ("tag_named", SlotDisplay::Tag { tag: tag("minecraft:planks") }),
        ("tag_direct", SlotDisplay::Tag { tag: direct(&[Item::OAK_PLANKS, Item::STONE, Item::DIAMOND]) }),
        ("tag_direct_empty", SlotDisplay::Tag { tag: direct(&[]) }),
        ("any_potion", SlotDisplay::WithAnyPotion { display: boxed(item(Item::POTION)) }),
        ("only_with_component", SlotDisplay::OnlyWithComponent { source: boxed(item(Item::APPLE)), component: DataComponentType::FOOD }),
        ("dyed", SlotDisplay::Dyed { dye: boxed(item(Item::RED_DYE)), target: boxed(item(Item::LEATHER_CHESTPLATE)) }),
        (
            "smithing_trim_ref",
            SlotDisplay::SmithingTrim { base: boxed(item(Item::IRON_CHESTPLATE)), material: boxed(item(Item::GOLD_INGOT)), pattern: Holder::Reference(trim_id) },
        ),
        (
            "smithing_trim_direct",
            SlotDisplay::SmithingTrim {
                base: boxed(item(Item::IRON_CHESTPLATE)),
                material: boxed(item(Item::GOLD_INGOT)),
                pattern: Holder::Direct(TrimPattern {
                    asset_id: id("custom:pattern"),
                    description: Component::literal("Custom \u{1F600}").with_format(ChatFormatting::Green),
                    decal: true,
                }),
            },
        ),
        (
            "smithing_trim_direct_nodecal",
            SlotDisplay::SmithingTrim {
                base: boxed(SlotDisplay::Empty),
                material: boxed(SlotDisplay::AnyFuel),
                pattern: Holder::Direct(TrimPattern { asset_id: id("a:b"), description: Component::empty(), decal: false }),
            },
        ),
        ("with_remainder", SlotDisplay::WithRemainder { input: boxed(item(Item::LAVA_BUCKET)), remainder: boxed(item(Item::BUCKET)) }),
        ("composite_empty", SlotDisplay::Composite { contents: vec![] }),
        (
            "composite",
            SlotDisplay::Composite { contents: vec![item(Item::STONE), SlotDisplay::Empty, SlotDisplay::Tag { tag: tag("minecraft:coals") }] },
        ),
    ];
    let mut deep = item(Item::DIAMOND);
    for i in 0..6 {
        deep = match i % 3 {
            0 => SlotDisplay::Composite {
                contents: vec![deep.clone(), SlotDisplay::WithRemainder { input: boxed(deep), remainder: boxed(SlotDisplay::Empty) }],
            },
            1 => SlotDisplay::WithAnyPotion { display: boxed(SlotDisplay::Dyed { dye: boxed(deep), target: boxed(SlotDisplay::AnyFuel) }) },
            _ => SlotDisplay::OnlyWithComponent {
                source: boxed(SlotDisplay::SmithingTrim { base: boxed(deep), material: boxed(item(Item::EMERALD)), pattern: Holder::Reference(trim_id) }),
                component: DataComponentType::DAMAGE,
            },
        };
    }
    m.push(("deep_nested", deep));
    m
}

fn recipes(g: &Golden) {
    let sd = slot_displays(g);
    let s = |n: &str| sd.iter().find(|(k, _)| *k == n).unwrap().1.clone();
    let all: Vec<SlotDisplay> = sd.iter().map(|(_, d)| d.clone()).collect();
    for (name, d) in &sd {
        g.check_value(&format!("slot_{name}"), d);
    }
    let station = item(Item::CRAFTING_TABLE);
    let e = || SlotDisplay::Empty;
    let shapeless = |ingredients, result, crafting_station| RecipeDisplay::CraftingShapeless(ShapelessCraftingRecipeDisplay { ingredients, result, crafting_station });
    let shaped = |width, height, ingredients, result, crafting_station| {
        RecipeDisplay::CraftingShaped(ShapedCraftingRecipeDisplay { width, height, ingredients, result, crafting_station })
    };
    let furnace = |ingredient, fuel, result, crafting_station, duration, experience| {
        RecipeDisplay::Furnace(FurnaceRecipeDisplay { ingredient, fuel, result, crafting_station, duration, experience })
    };
    let displays: Vec<(&str, RecipeDisplay)> = vec![
        ("shapeless_empty", shapeless(vec![], e(), e())),
        ("shapeless", shapeless(vec![item(Item::OAK_PLANKS), SlotDisplay::Tag { tag: tag("minecraft:coals") }], item(Item::STICK), station.clone())),
        (
            "shapeless_all_slot_displays",
            shapeless(all.clone(), SlotDisplay::ItemStack { stack: ItemStackTemplate::new(Item::STICK, 4, DataComponentPatch::empty()).unwrap() }, station.clone()),
        ),
        ("shaped_1x1", shaped(1, 1, vec![item(Item::STONE)], item(Item::STONE_BRICKS), station.clone())),
        (
            "shaped_3x3",
            shaped(
                3,
                3,
                vec![
                    item(Item::STONE), item(Item::STONE), item(Item::STONE), e(), item(Item::STICK), e(), e(), item(Item::STICK), e(),
                ],
                item(Item::STONE_PICKAXE),
                station.clone(),
            ),
        ),
        ("shaped_0x0", shaped(0, 0, vec![], e(), e())),
        ("shaped_2x3_nested", shaped(2, 3, all[..6].to_vec(), s("deep_nested"), s("with_remainder"))),
        ("furnace", furnace(item(Item::IRON_ORE), SlotDisplay::AnyFuel, item(Item::IRON_INGOT), item(Item::FURNACE), 200, 0.7)),
        ("furnace_neg", furnace(s("composite"), s("dyed"), s("any_potion"), s("only_with_component"), -1, -0.5)),
        ("furnace_big", furnace(e(), e(), e(), e(), i32::MAX, 3.4028235E38)),
        (
            "stonecutter",
            RecipeDisplay::Stonecutter(StonecutterRecipeDisplay { input: item(Item::STONE), result: item(Item::STONE_SLAB), crafting_station: item(Item::STONECUTTER) }),
        ),
        (
            "smithing",
            RecipeDisplay::Smithing(SmithingRecipeDisplay {
                template: item(Item::NETHERITE_UPGRADE_SMITHING_TEMPLATE),
                base: item(Item::DIAMOND_SWORD),
                addition: item(Item::NETHERITE_INGOT),
                result: s("item_stack_patch"),
                crafting_station: item(Item::SMITHING_TABLE),
            }),
        ),
        (
            "smithing_trim",
            RecipeDisplay::Smithing(SmithingRecipeDisplay {
                template: s("composite"),
                base: s("smithing_trim_ref"),
                addition: s("smithing_trim_direct"),
                result: s("with_remainder"),
                crafting_station: e(),
            }),
        ),
    ];
    for (name, d) in &displays {
        g.check_value(&format!("display_{name}"), d);
        g.check(&format!("cb_place_ghost_{name}"), &ClientboundPlaceGhostRecipePacket { container_id: 3, recipe_display: d.clone() });
    }
    let d = |n: &str| displays.iter().find(|(k, _)| *k == n).unwrap().1.clone();

    // recipe book add
    let mut entries = vec![];
    for (n, cat) in RecipeBookCategory::iter().enumerate() {
        for display in [d("shapeless"), d("furnace")] {
            let n = entries.len() as i32 + 0 * n as i32;
            let crafting_requirements = match n % 3 {
                0 => None,
                1 => Some(vec![]),
                _ => Some(vec![
                    ingredient(&[Item::STONE]),
                    Ingredient { values: tag("minecraft:planks") },
                    ingredient(&[Item::OAK_PLANKS, Item::BIRCH_PLANKS]),
                ]),
            };
            let contents = RecipeDisplayEntry {
                id: RecipeDisplayId { index: n },
                display,
                group: (n % 2 != 0).then_some(n * 100),
                category: cat,
                crafting_requirements,
            };
            g.check_value(&format!("entry_{n}"), &contents);
            entries.push(ClientboundRecipeBookAddEntry { contents, flags: (n % 4) as i8 });
        }
    }
    let add = |entries, replace| ClientboundRecipeBookAddPacket { entries, replace };
    g.check("cb_recipe_book_add_empty", &add(vec![], false));
    g.check("cb_recipe_book_add_empty_replace", &add(vec![], true));
    g.check("cb_recipe_book_add_one", &add(entries[..1].to_vec(), false));
    g.check("cb_recipe_book_add_all", &add(entries, true));
    let entry = |index, display, group, category| RecipeDisplayEntry { id: RecipeDisplayId { index }, display, group, category, crafting_requirements: None };
    for (k, (_, display)) in displays.iter().enumerate() {
        let e = entry(1000 + k as i32, display.clone(), Some(i32::MAX), RecipeBookCategory::CRAFTING_MISC);
        g.check(&format!("cb_recipe_book_add_display_{k}"), &add(vec![ClientboundRecipeBookAddEntry { contents: e, flags: -1 }], false));
    }
    for flags in [0i8, 1, 2, 3, 4, 127, -128, -1] {
        let e = entry(5, d("stonecutter"), Some(-2), RecipeBookCategory::STONECUTTER);
        g.check(&format!("cb_recipe_book_add_flags_{flags}"), &add(vec![ClientboundRecipeBookAddEntry { contents: e, flags }], false));
    }
    let n = ClientboundRecipeBookAddEntry::new(entry(0, d("stonecutter"), None, RecipeBookCategory::STONECUTTER), true, false);
    assert_eq!((n.flags, n.notification(), n.highlight()), (1, true, false));
    let n = ClientboundRecipeBookAddEntry::new(n.contents, false, true);
    assert_eq!((n.flags, n.notification(), n.highlight()), (2, false, true));

    // update recipes
    let key = |k: &str| id(&format!("minecraft:{k}"));
    let set = |items: &[Item]| RecipePropertySet { items: items.to_vec() };
    let update = |item_sets, entries| ClientboundUpdateRecipesPacket { item_sets, stonecutter_recipes: SelectableRecipeSingleInputSet { entries } };
    g.check("cb_update_recipes_empty", &update(vec![], vec![]));
    let sets = vec![
        (key("smithing_base"), set(&[Item::DIAMOND_SWORD])),
        (key("furnace_input"), set(&[])),
        (key("brewing_reagent"), set(&[Item::NETHER_WART])),
        (id("custom:set"), set(&[])),
    ];
    g.check("cb_update_recipes_sets", &update(sets.clone(), vec![]));
    // vanilla writes a Set in hash order (random per JVM): compare the decoded items as a set
    let mut bytes = g.bytes("cb_update_recipes_multi_item_set");
    read_varint(&mut bytes).unwrap();
    let vanilla = ClientboundUpdateRecipesPacket::decode(&mut bytes).unwrap();
    assert_eq!(vanilla.item_sets.len(), 1);
    let mut got = vanilla.item_sets[0].1.items.clone();
    got.sort();
    let mut want = vec![Item::BEEF, Item::PORKCHOP, Item::CHICKEN, Item::COD, Item::SALMON, Item::POTATO];
    want.sort();
    assert_eq!((&vanilla.item_sets[0].0, got), (&key("smoker_input"), want));
    let entry = |input, option_display| SelectableRecipeSingleInputEntry { input, recipe: SelectableRecipe { option_display } };
    let cutter = vec![
        entry(ingredient(&[Item::STONE]), item(Item::STONE_SLAB)),
        entry(ingredient(&[Item::STONE]), item(Item::STONE_STAIRS)),
        entry(Ingredient { values: tag("minecraft:planks") }, s("deep_nested")),
        entry(ingredient(&[Item::COBBLESTONE, Item::STONE]), SlotDisplay::Empty),
    ];
    g.check("cb_update_recipes_stonecutter", &update(vec![], cutter.clone()));
    g.check("cb_update_recipes_both", &update(sets, cutter));
    g.check("cb_update_recipes_all_displays", &update(vec![], all.iter().map(|d| entry(ingredient(&[Item::STONE]), d.clone())).collect()));
}

fn item_limits(g: &Golden) {
    for label in [
        "click_129_changes", "click_neg_changes", "click_hashed_unknown_item", "click_hashed_patch_too_many",
        "click_hashed_removed_too_many", "click_hashed_unknown_component",
    ] {
        check_bad::<ServerboundContainerClickPacket>(g, label);
    }
    for v in [7, 8, 100, -1, i32::MIN] {
        check_lenient::<ServerboundContainerClickPacket>(g, &format!("click_input_{v}"));
    }
    for v in [5, 6, 1000, -1] {
        check_bad::<ClientboundPlaceGhostRecipePacket>(g, &format!("display_type_{v}"));
    }
    for v in [11, 12, 1000, -1] {
        check_bad::<ClientboundPlaceGhostRecipePacket>(g, &format!("slot_display_type_{v}"));
    }
    for label in [
        "slot_display_nested_bad", "shaped_wrong_size", "shaped_neg_size", "slot_item_unknown", "slot_item_stack_air",
        "slot_item_stack_zero_count", "slot_only_with_component_unknown",
    ] {
        check_bad::<ClientboundPlaceGhostRecipePacket>(g, label);
    }
    check_lenient::<ClientboundPlaceGhostRecipePacket>(g, "shaped_overflow_size");
    for label in ["ingredient_empty", "ingredient_air", "ingredient_unknown_item", "ingredient_neg_count", "category_unknown"] {
        check_bad::<ClientboundRecipeBookAddPacket>(g, label);
    }
    check_bad::<ClientboundMerchantOffersPacket>(g, "merchant_result_empty");
    check_bad::<ClientboundUpdateRecipesPacket>(g, "update_unknown_item_in_set");
    check_bad::<ClientboundUpdateRecipesPacket>(g, "update_bad_key");
    check_lenient::<ClientboundUpdateRecipesPacket>(g, "update_dup_items_in_set");
}

fn merchant(g: &Golden) {
    let st = stacks();
    let get = |n: &str| st.iter().find(|(k, _)| *k == n).unwrap().1.clone();
    let cost = |item, count, components| ItemCost { item, count, components };
    let none = DataComponentExactPredicate::empty();
    let plain = cost(Item::EMERALD, 5, none.clone());
    let comp = cost(
        Item::PAPER,
        2,
        DataComponentExactPredicate::from_components(vec![
            TypedDataComponent { id: DataComponentType::DAMAGE, value: Value::new(4i32) },
            TypedDataComponent { id: DataComponentType::MAX_STACK_SIZE, value: Value::new(8i32) },
        ]),
    );
    let offer = |a, b, result, uses, max_uses, xp, special_price_diff, price_multiplier, demand| MerchantOffer {
        base_cost_a: a,
        cost_b: b,
        result,
        uses,
        max_uses,
        xp,
        special_price_diff,
        price_multiplier,
        demand,
    };
    let packet = |container_id, offers: &Vec<MerchantOffer>, villager_level, villager_xp, show_progress, can_restock| {
        ClientboundMerchantOffersPacket { container_id, offers: offers.clone(), villager_level, villager_xp, show_progress, can_restock }
    };
    let mut offers = vec![];
    g.check("cb_merchant_none", &packet(1, &offers, 1, 0, true, true));
    offers.push(offer(plain.clone(), None, get("diamond"), 0, 12, 2, 0, 0.05, 0));
    g.check("cb_merchant_one", &packet(2, &offers, 2, 10, true, false));
    offers.push(offer(comp.clone(), Some(plain.clone()), get("named"), 3, 16, 7, -4, 0.2, 11));
    offers.push(offer(cost(Item::STICK, 64, none.clone()), None, get("dirt64"), 0, 0, 0, 0, 0.0, -9));
    offers.push(offer(plain.clone(), Some(cost(Item::DIAMOND, 1, none.clone())), get("set_and_removed"), 7, 7, 30, i32::MIN, -1.5, i32::MAX));
    g.check("cb_merchant_four", &packet(3, &offers, 5, 250, false, false));
    g.check("cb_merchant_levels", &packet(300, &offers, 200, 100000, true, true));
    g.check("cb_merchant_neg", &packet(-1, &offers, -1, -1, false, true));
    let many: Vec<MerchantOffer> = (0..20)
        .map(|i| offer(cost(Item::EMERALD, 1 + i, none.clone()), None, ItemStack::with_count(Item::DIAMOND, 1 + i), 0, i + 1, i, 0, 0.05, 0))
        .collect();
    g.check("cb_merchant_20", &packet(9, &many, 3, 60, true, true));
    g.check_value("item_cost_plain", &plain);
    g.check_value("item_cost_components", &comp);
    g.check_value("item_cost_optional_none", &None::<ItemCost>);
    g.check_value("item_cost_optional_some", &Some(comp));
}
