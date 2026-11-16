//! The light a client holds, on the real vanilla server and on `mc-server` with the same seed on a flat world: the light
//! payloads of the chunk packets it receives at login, then of the `light_update` packets after blocks that stop the
//! sky and a light source are placed. A packet batches whatever changed since the previous one, so the packets
//! themselves are not compared one by one: each chunk's light is rebuilt the way the client does (the layers a mask
//! lists replace the held ones, the empty mask clears them) and the final light of the chunks both servers sent is
//! compared exactly.
use std::collections::BTreeMap;
use std::time::Duration;

use mc_protocol::codec::BitSet;
use mc_protocol::packets::play::*;
use mc_testbot::join::BotOptions;
use mc_testbot::raw::{LightPayload, RawClient};

const PROPS: &[&str] = &[
    "level-type=minecraft:flat",
    "generate-structures=false",
    "view-distance=2",
];

/// The light of one chunk as a client holds it: the 2048-byte layer of every light section (bit 0 = the section below
/// the world) that has one; a section without an entry has no light.
#[derive(Default, PartialEq, Eq, Debug)]
struct ChunkLight {
    sky: BTreeMap<usize, Vec<u8>>,
    block: BTreeMap<usize, Vec<u8>>,
}

impl ChunkLight {
    fn apply(&mut self, data: &ClientboundLightUpdatePacketData) {
        fn merge(
            held: &mut BTreeMap<usize, Vec<u8>>,
            mask: &BitSet,
            empty: &BitSet,
            layers: &[Vec<u8>],
        ) {
            let mut layers = layers.iter();
            for bit in 0..mask.length() {
                if mask.get(bit) {
                    let layer = layers.next().expect("a layer per mask bit");
                    // an all-zero layer is the same light as none: whether the engine holds one depends on load order
                    if layer.iter().all(|b| *b == 0) {
                        held.remove(&bit);
                    } else {
                        held.insert(bit, layer.clone());
                    }
                }
            }
            for bit in 0..empty.length() {
                if empty.get(bit) {
                    held.remove(&bit);
                }
            }
        }
        merge(
            &mut self.sky,
            &data.sky_y_mask,
            &data.empty_sky_y_mask,
            &data.sky_updates,
        );
        merge(
            &mut self.block,
            &data.block_y_mask,
            &data.empty_block_y_mask,
            &data.block_updates,
        );
    }
}

/// Joins, loads the world, and returns the light of every chunk after the login and after the edits.
async fn light_after(
    port: u16,
    cmd: impl Fn(&str),
) -> (
    BTreeMap<(i32, i32), ChunkLight>,
    BTreeMap<(i32, i32), ChunkLight>,
) {
    let mut opts = BotOptions::new("127.0.0.1", port, "LightBot");
    opts.timeout = Duration::from_secs(120);
    opts.client_information.view_distance = 2;
    let mut c = RawClient::connect(&opts).await.expect("join");
    c.pump_until("minecraft:set_experience", 1, Duration::from_secs(120))
        .await
        .expect("join");
    c.send(ServerboundPlayPacket::PlayerLoaded(
        ServerboundPlayerLoadedPacket,
    ))
    .await
    .expect("player loaded");
    // the same place on both servers (a new player's spawn point is random)
    cmd("tp LightBot 8 -60 8");
    c.pump(Duration::from_secs(6), |_| None)
        .await
        .expect("chunks");
    let at_login = rebuild(&c.light);
    let seen = c.light.len();
    // the server sends a chunk's light update only to the players for whom the chunk is on the edge of their view (the
    // tracked ring is 3 chunks out), so the edits go into the chunks 1 to 4 chunks east of the player
    for k in 1..=4 {
        let (x, z) = (8 + 16 * k, 8);
        for command in [
            format!("setblock {x} 100 {z} minecraft:stone"),
            format!("setblock {} -60 {} minecraft:glowstone", x + 1, z + 1),
            format!(
                "fill {x} 100 {} {} 103 {} minecraft:oak_planks hollow",
                z - 4,
                x + 5,
                z + 1
            ),
            format!("setblock {x} 100 {z} minecraft:air"),
        ] {
            cmd(&command);
            c.pump(Duration::from_millis(250), |_| None)
                .await
                .expect(&command);
        }
    }
    c.pump(Duration::from_secs(2), |_| None)
        .await
        .expect("light updates");
    assert!(
        c.light[seen..].iter().any(|p| p.update),
        "port {port}: the edits sent no light_update packet"
    );
    (at_login, rebuild(&c.light))
}

fn rebuild(payloads: &[LightPayload]) -> BTreeMap<(i32, i32), ChunkLight> {
    let mut chunks: BTreeMap<(i32, i32), ChunkLight> = BTreeMap::new();
    for p in payloads {
        let held = chunks.entry((p.x, p.z)).or_default();
        if !p.update {
            // a chunk packet replaces whatever the client held for the chunk
            *held = ChunkLight::default();
        }
        held.apply(&p.data);
    }
    chunks
}

/// The chunks both servers sent must hold the same light; at least `min` chunks must be in common.
fn assert_same(
    what: &str,
    vanilla: &BTreeMap<(i32, i32), ChunkLight>,
    ours: &BTreeMap<(i32, i32), ChunkLight>,
    min: usize,
) {
    let common: Vec<_> = vanilla.keys().filter(|k| ours.contains_key(k)).collect();
    assert!(
        common.len() >= min,
        "{what}: only {} chunks in common (vanilla {}, ours {})",
        common.len(),
        vanilla.len(),
        ours.len()
    );
    let differing: Vec<_> = common.iter().filter(|k| vanilla[k] != ours[k]).collect();
    assert!(
        differing.is_empty(),
        "{what}: light differs in {} of {} chunks, first {:?}\nvanilla sky {:?} block {:?}\nours    sky {:?} block {:?}",
        differing.len(),
        common.len(),
        differing[0],
        vanilla[*differing[0]].sky.keys().collect::<Vec<_>>(),
        vanilla[*differing[0]].block.keys().collect::<Vec<_>>(),
        ours[*differing[0]].sky.keys().collect::<Vec<_>>(),
        ours[*differing[0]].block.keys().collect::<Vec<_>>(),
    );
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "oracle: starts the vanilla server and the mc-server binary"]
async fn client_light_matches_vanilla() {
    let vanilla = mc_testbot::harness::vanilla(12345, "survival", PROPS)
        .start()
        .expect("start vanilla");
    let (v_login, v_edited) = light_after(vanilla.port, |c| vanilla.cmd(c)).await;
    vanilla.stop();
    // `LIGHT_PACKETS_BASELINE=1`: the second server is vanilla too, to see what a run of vanilla itself varies in
    let ours = if std::env::var_os("LIGHT_PACKETS_BASELINE").is_some() {
        mc_testbot::harness::vanilla(12345, "survival", PROPS)
    } else {
        crate::common::ours(12345, "survival", PROPS)
    }
    .start()
    .expect("start ours");
    let (o_login, o_edited) = light_after(ours.port, |c| ours.cmd(c)).await;
    ours.kill();
    assert_same("light at login", &v_login, &o_login, 9);
    assert_same("light after the edits", &v_edited, &o_edited, 9);
}
