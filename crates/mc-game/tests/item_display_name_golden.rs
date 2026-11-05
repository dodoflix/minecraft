//! `ItemStack.getDisplayName()` against vanilla (`fixtures/ItemDisplayNameGolden.java`): every item plus custom name,
//! rarity override and written book title variants.

use mc_data::Item;
use mc_game::world::item::{ItemStack, ItemStackExt};
use mc_protocol::components::{CUSTOM_NAME, Filterable, RARITY, Rarity, WRITTEN_BOOK_CONTENT, WrittenBookContent};
use mc_text::Component;

fn book(title: &str) -> ItemStack {
    let mut s = ItemStack::new(Item::WRITTEN_BOOK);
    s.set(WRITTEN_BOOK_CONTENT, WrittenBookContent { title: Filterable::pass_through(title.to_owned()), author: "me".into(), generation: 0, pages: Vec::new(), resolved: true });
    s
}

#[test]
fn display_name_golden() {
    mc_protocol::components::install_text_hooks();
    let text = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/item_display_name_golden.txt")).expect("fixture");
    let mut failures = Vec::new();
    for line in text.lines() {
        let (label, json) = line.split_once('\t').unwrap();
        let stack = match label {
            "custom_name" => {
                let mut s = ItemStack::with_count(Item::DIAMOND_SWORD, 3);
                s.set(CUSTOM_NAME, Component::literal("Hello"));
                s
            }
            "epic" => {
                let mut s = ItemStack::new(Item::DIRT);
                s.set(RARITY, Rarity::Epic);
                s
            }
            "book" => book("Title"),
            "book_blank" => book("  "),
            id => ItemStack::new(Item::by_name(id).unwrap()),
        };
        let expected: serde_json::Value = serde_json::from_str(json).unwrap();
        if stack.get_display_name().to_json().unwrap() != expected {
            failures.push(label);
        }
    }
    assert!(failures.is_empty(), "{failures:?}");
}
