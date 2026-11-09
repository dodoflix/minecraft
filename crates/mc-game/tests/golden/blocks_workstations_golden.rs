//! Campfire cooking, enchanting table and the anvil/grindstone menu hooks: scenarios
//! `tests/oracle/scenarios/blk_{fbc_campfire,campfire_cook,enchanting_table_nbt,workstation_menus}.txt` replayed on the
//! real vanilla level (`scripts/block-oracle.sh NAME`) vs our ticking level (fixtures `tests/fixtures/entity_oracle/`).

use crate::common;

#[test]
fn campfire_block_states_match_vanilla() {
    common::block_scenario::check("blk_fbc_campfire");
}

#[test]
fn campfire_cooking_matches_vanilla() {
    common::block_scenario::check("blk_campfire_cook");
}

#[test]
fn enchanting_table_block_entity_matches_vanilla() {
    common::block_scenario::check("blk_enchanting_table_nbt");
}

#[test]
fn workstation_menus_open_like_vanilla() {
    common::block_scenario::check("blk_workstation_menus");
}

/// `ServerPlayerGameMode.useItemOn` for a spectator: a block without a menu that is a `Portal` answers with the
/// `spectator.cannot_teleport` overlay when it has no destination (a nether portal without an exit portal; the test
/// server has no nether level, vanilla's has no exit portal) and consumes the use; anything else passes. The packet is the
/// real server's (`system_chat`: red translatable `spectator.cannot_teleport`, overlay), recorded from a spectator
/// right-clicking a nether portal block.
#[test]
fn spectator_right_click_on_a_portal_matches_vanilla() {
    use mc_core::ident::Identifier;
    use mc_core::pos::BlockPos;
    use mc_data::Block;
    use mc_data::enums::GameType;
    use mc_game::server::level::ServerLevel;
    use mc_game::server::network::game_packets::PlayCtx;
    use mc_game::server::network::game_packets::hooks::{ITEM_SYSTEMS, ServerboundBlockHitResult};
    use mc_game::server::network::{ConnectionSender, Outbound};
    use mc_game::world::entity::ServerPlayer;
    use mc_game::world::item::{InteractionHand, InteractionResult};
    use mc_game::world::level::{BlockGetter, LevelAccessor};

    const CANNOT_TELEPORT: &str = "7c0a080005636f6c6f7200037265640800097472616e736c6174650019737065637461746f722e63616e6e6f745f74656c65706f72740001";
    let (mut server, _guard) = crate::command_common::test_server();
    let mut level = ServerLevel::detached(Identifier::parse("minecraft:overworld").unwrap());
    let (portal, stone) = (BlockPos::new(3, 64, 0), BlockPos::new(5, 64, 0));
    level.set_block_and_update(portal, Block::NETHER_PORTAL.default_state());
    level.set_block_and_update(stone, Block::STONE.default_state());
    assert_eq!(level.get_block_state(portal).block(), Block::NETHER_PORTAL);
    let mut right_click = |mode: GameType, pos: BlockPos| {
        let mut player = ServerPlayer::new(
            1,
            Identifier::parse("minecraft:overworld").unwrap(),
            mc_game::server::network::login_listener::create_offline_profile("Bob"),
            Default::default(),
        );
        player.set_game_mode_for_player(mode, None);
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        player.connection = Some(ConnectionSender::new(tx));
        let hit = ServerboundBlockHitResult {
            block_pos: pos,
            direction: mc_core::direction::Direction::Up,
            click_x: 0.5,
            click_y: 1.0,
            click_z: 0.5,
            inside: false,
            world_border_hit: false,
        };
        let result = ITEM_SYSTEMS.first().expect("item systems").use_item_on(
            &mut PlayCtx {
                server: &mut server,
                level: &mut level,
                player: &mut player,
            },
            InteractionHand::MainHand,
            &hit,
        );
        let mut sent = Vec::new();
        while let Ok(Outbound::Payload(p) | Outbound::Write(p)) = rx.try_recv() {
            sent.push(p.iter().map(|b| format!("{b:02x}")).collect::<String>());
        }
        (result, sent)
    };
    assert_eq!(
        right_click(GameType::Spectator, portal),
        (InteractionResult::CONSUME, vec![CANNOT_TELEPORT.to_owned()])
    );
    assert_eq!(
        right_click(GameType::Spectator, stone),
        (InteractionResult::Pass, vec![])
    );
    assert_eq!(
        right_click(GameType::Survival, portal).1,
        Vec::<String>::new()
    );
}
