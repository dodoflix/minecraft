//! A test instance block action is carried out inline, right after its packet: a `save` followed at once by a
//! `set_test_block` of a block inside the box saves the test block as it was before the second packet, even when both
//! packets arrive in the same tick. The same script runs against the vanilla server and ours.
use std::path::Path;
use std::time::{Duration, Instant};

use crate::common::*;
use mc_core::ident::Identifier;
use mc_core::pos::block_pos::BlockPos;
use mc_core::pos::vec3i::Vec3i;
use mc_core::rotation::Rotation;
use mc_protocol::packets::play::{
    ServerboundPlayPacket, ServerboundSetTestBlockPacket, ServerboundTestInstanceBlockActionPacket,
    ServerboundTestInstanceBlockActionPacketAction, TestBlockMode, TestInstanceBlockEntityData,
    TestInstanceBlockEntityStatus,
};
use mc_testbot::Bot;

const PROPS: &[&str] = &[
    "level-type=minecraft:flat",
    "generate-structures=false",
    "spawn-monsters=false",
    "spawn-animals=false",
];
const ROUNDS: usize = 10;
const INSTANCE: BlockPos = BlockPos::new(10, -60, 10);
const TEST_BLOCK: BlockPos = BlockPos::new(11, -58, 12);

fn set_test_block(mode: TestBlockMode) -> ServerboundPlayPacket {
    ServerboundPlayPacket::SetTestBlock(ServerboundSetTestBlockPacket {
        position: TEST_BLOCK,
        mode,
        message: String::new(),
    })
}

fn save() -> ServerboundPlayPacket {
    ServerboundPlayPacket::TestInstanceBlockAction(ServerboundTestInstanceBlockActionPacket {
        pos: INSTANCE,
        action: ServerboundTestInstanceBlockActionPacketAction::Save,
        data: TestInstanceBlockEntityData {
            test: Some(Identifier::read("foo:b").expect("id")),
            size: Vec3i::new(3, 3, 3),
            rotation: Rotation::None,
            ignore_entities: false,
            status: TestInstanceBlockEntityStatus::Cleared,
            error_message: None,
        },
    })
}

/// The `mode` of the test block in the saved structure, once the file is complete.
async fn saved_mode(file: &Path) -> String {
    let deadline = Instant::now() + T;
    loop {
        if let Ok(bytes) = std::fs::read(file)
            && let Ok(root) = mc_nbt::compress::read_compressed(
                &bytes,
                &mut mc_nbt::NbtAccounter::unlimited_heap(),
            )
        {
            let blocks = root.get_list("blocks").expect("blocks");
            return (0..)
                .map_while(|i| blocks.get_compound(i))
                .filter_map(|b| b.get_compound("nbt"))
                .find_map(|nbt| nbt.get_string("mode"))
                .expect("a test block in the structure")
                .to_string();
        }
        assert!(
            Instant::now() < deadline,
            "{} was never written",
            file.display()
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

async fn scenario(server: &mc_testkit::Server, bot: &Bot) {
    server.cmd("setblock 10 -60 10 minecraft:test_instance_block");
    server.cmd("setblock 11 -58 12 minecraft:test_block");
    let file = server.dir().join("world/generated/foo/structure/b.nbt");
    for round in 0..ROUNDS {
        bot.send(set_test_block(TestBlockMode::Start));
        tokio::time::sleep(Duration::from_millis(300)).await;
        let _ = std::fs::remove_file(&file);
        // back to back: the save has to see the test block before the second packet changed it
        bot.send(save());
        bot.send(set_test_block(TestBlockMode::Fail));
        assert_eq!(
            saved_mode(&file).await,
            "start",
            "round {round}: the save ran after the next packet"
        );
    }
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "oracle: starts the vanilla server"]
async fn test_action_order_vanilla() {
    let server = mc_testbot::harness::vanilla(12345, "creative", PROPS)
        .start()
        .expect("start vanilla");
    let bot = join("127.0.0.1", server.port, "Bot").await;
    server.cmd("op Bot");
    server.wait_log("Made Bot a server operator", 30);
    scenario(&server, &bot).await;
    bot.disconnect().await;
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "oracle: starts the mc-server binary"]
async fn test_action_order_ours() {
    let server = ours(12345, "creative", PROPS)
        .start()
        .expect("start our server");
    let bot = join("127.0.0.1", server.port, "Bot").await;
    server.cmd("op Bot");
    server.wait_log("Made Bot a server operator", 30);
    scenario(&server, &bot).await;
    bot.disconnect().await;
}
