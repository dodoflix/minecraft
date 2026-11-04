//! Golden tests for the `inventory` play packets against vanilla's `GameProtocols` (generator: fixtures/play/GenInventory.java).
mod common;
use common::*;
use std::fmt::Debug;

use mc_core::ident::Identifier;
use mc_core::pos::block_pos::BlockPos;
use mc_data::{MapDecorationType, MenuType};
use mc_protocol::codec::read_varint;
use mc_protocol::packets::play::*;
use mc_protocol::Packet;
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
    // g.assert_all_used() comes with the item-carrying packets
}
