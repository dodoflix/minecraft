//! Book handlers vs the real vanilla server: `tests/oracle/BooksOracle.java` replays `handleEditBook` (page update, signing,
//! ignored edits) and written-book use (`WrittenBookContent.resolveForItem`) on a real `ServerPlayer`; each resulting stack
//! (`ItemStack.OPTIONAL_CODEC` as SNBT) must equal ours. Regenerate: `scripts/entity-oracle.sh crates/mc-game/tests/oracle/BooksOracle.java > crates/mc-game/tests/fixtures/books_oracle.txt`.
use mc_core::ident::Identifier;
use mc_game::commands::component_utils::{LimitBehavior, ResolutionContext};
use mc_game::server::network::text_filter::FilteredText;
use mc_game::world::container::Container;
use mc_game::world::entity::ServerPlayer;
use mc_game::world::item::{ItemStack, edit_book, resolve_for_item};
use mc_nbt::Tag;
use mc_protocol::codec::GameProfile;
use mc_protocol::packets::play::ServerboundEditBookPacket;
use std::cell::Cell;

fn stack_of(snbt: &str) -> ItemStack {
    let tag = Tag::Compound(mc_nbt::snbt::parse_compound(snbt).expect("fixture SNBT"));
    ItemStack::optional_from_tag(&tag)
        .into_result()
        .expect("fixture stack decodes")
}

fn tag_of(stack: &ItemStack) -> Tag {
    stack
        .optional_to_tag()
        .into_result()
        .expect("stack encodes")
}

#[test]
fn books_match_vanilla() {
    let fixture = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/books_oracle.txt"
    ))
    .unwrap();
    let profile = GameProfile {
        id: uuid::Uuid::nil(),
        name: "Booker".into(),
        properties: vec![],
    };
    let mut player = ServerPlayer::new(
        1,
        Identifier::parse("minecraft:overworld").unwrap(),
        profile,
        Default::default(),
    );
    let context = ResolutionContext {
        source: None,
        default_scoreboard_entity: None,
        depth_limit: 100,
        depth_limit_behavior: LimitBehavior::StopProcessingAndCopyRemaining,
        resolved_component_limit: 65536,
        resolved_component_count: Cell::new(0),
    };
    let (mut checked, mut slot) = (0, 0usize);
    for line in fixture.lines() {
        let Some((op, rest)) = line.split_once(' ') else {
            continue;
        };
        match op {
            "SETUP" => {
                let (s, snbt) = rest.split_once(' ').unwrap();
                slot = s.parse().unwrap();
                player.player.inventory.set_item(slot, stack_of(snbt));
            }
            "EDIT" => {
                let (s, rest) = rest.split_once(' ').unwrap();
                slot = s.parse().unwrap();
                // a title is a JSON string and may contain spaces: the pages array starts where the first JSON value ends
                let (title, pages) = match rest.strip_prefix("- ") {
                    Some(pages) => (None, pages),
                    None => {
                        let mut values =
                            serde_json::Deserializer::from_str(rest).into_iter::<String>();
                        let title = values.next().unwrap().unwrap();
                        (Some(title), rest[values.byte_offset()..].trim_start())
                    }
                };
                let pages: Vec<String> = serde_json::from_str(pages).unwrap();
                edit_book(
                    &mut player,
                    &ServerboundEditBookPacket {
                        slot: slot as i32,
                        pages,
                        title,
                    },
                    |s| FilteredText::pass_through(s),
                );
            }
            "USE" => {
                slot = rest.parse().unwrap();
                let mut stack = player.player.inventory.get_item(slot).clone();
                resolve_for_item(&mut stack, &context);
                player.player.inventory.set_item(slot, stack);
            }
            "ITEM" => {
                let (s, snbt) = rest.split_once(' ').unwrap();
                assert_eq!(s.parse::<usize>().unwrap(), slot);
                let expected = Tag::Compound(mc_nbt::snbt::parse_compound(snbt).unwrap());
                assert_eq!(
                    tag_of(player.player.inventory.get_item(slot)),
                    expected,
                    "slot {slot} after {snbt}"
                );
                checked += 1;
            }
            _ => {}
        }
    }
    assert_eq!(checked, 12);
}
