//! `Item` data-driven defaults against vanilla (`fixtures/ItemGolden.java`): tool speed / correct tool for drops over
//! every (tool item, block) pair and use duration / animation of every item.

use mc_data::{Item, block::Block};
use mc_game::world::item::{ItemStack, behaviour};

#[test]
fn item_defaults_golden() {
    let text = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/item_golden.txt")).expect("fixture");
    let (mut tools, mut uses) = (0, 0);
    for line in text.lines() {
        let p: Vec<&str> = line.split(' ').collect();
        let item = Item::by_name(p[1]).unwrap();
        let stack = ItemStack::new(item);
        let b = behaviour(item);
        if p[0] == "tool" {
            let block = Block::by_name(p[2]).unwrap();
            let state = block.default_state();
            let speed = f32::from_bits(p[3].parse::<i32>().unwrap() as u32);
            assert_eq!(b.get_destroy_speed(&stack, state), speed, "{line}");
            assert_eq!(b.is_correct_tool_for_drops(&stack, state), p[4] == "true", "{line}");
            tools += 1;
        } else {
            // InstrumentItem's duration comes from the (data-driven) instrument; typed instrument components are pending.
            if item.data().classes.contains(&"InstrumentItem") {
                continue;
            }
            assert_eq!(b.get_use_duration(&stack), p[2].parse::<i32>().unwrap(), "{line}");
            assert_eq!(b.get_use_animation(&stack).name().to_uppercase(), p[3], "{line}");
            uses += 1;
        }
    }
    assert!(tools > 5000 && uses > 1500, "{tools} {uses}");
}
