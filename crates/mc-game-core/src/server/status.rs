//! Server-list status: `MinecraftServer.buildServerStatus`/`buildPlayerStatus`, `loadStatusIcon` and `util.PngInfo`.

use mc_core::math::mth;
use mc_core::random::RandomSource;
use mc_protocol::packets::status::{Players, ServerStatus, Version};
use mc_text::Component;
use mc_world::level_storage::WORLD_VERSION_NAME;
use std::path::{Path, PathBuf};
use uuid::Uuid;

/// `MinecraftServer.MAX_STATUS_PLAYER_SAMPLE`.
const MAX_STATUS_PLAYER_SAMPLE: usize = 12;

/// `MinecraftServer.ANONYMOUS_PLAYER_PROFILE`.
pub fn anonymous_player_profile() -> (Uuid, String) {
    (Uuid::nil(), "Anonymous Player".to_owned())
}

/// What `buildPlayerStatus` needs from a `ServerPlayer`.
#[derive(Debug, Clone)]
pub struct StatusPlayer {
    pub name_and_id: (Uuid, String),
    pub allows_listing: bool,
}

/// `Util.shuffle`.
fn shuffle<T>(list: &mut [T], random: &mut dyn RandomSource) {
    for i in (2..=list.len()).rev() {
        let swap_to = random.next_int_bounded(i as i32) as usize;
        list.swap(i - 1, swap_to);
    }
}

/// `MinecraftServer.buildPlayerStatus`.
pub fn build_player_status(players: &[StatusPlayer], max_players: i32, hides_online_players: bool, random: &mut dyn RandomSource) -> Players {
    let online = players.len() as i32;
    if hides_online_players {
        return Players { max: max_players, online, sample: Vec::new() };
    }
    let sample_size = players.len().min(MAX_STATUS_PLAYER_SAMPLE);
    let offset = mth::next_int(random, 0, (players.len() - sample_size) as i32) as usize;
    let mut sample: Vec<(Uuid, String)> = players[offset..offset + sample_size]
        .iter()
        .map(|p| if p.allows_listing { p.name_and_id.clone() } else { anonymous_player_profile() })
        .collect();
    shuffle(&mut sample, random);
    Players { max: max_players, online, sample }
}

/// `MinecraftServer.buildServerStatus`.
pub fn build_server_status(motd: &str, players: Players, favicon: Option<&[u8]>, enforces_secure_chat: bool) -> ServerStatus {
    ServerStatus {
        description: Component::literal(motd),
        players: Some(players),
        version: Some(Version { name: WORLD_VERSION_NAME.to_owned(), protocol: mc_protocol::PROTOCOL_VERSION }),
        favicon: favicon.map(<[u8]>::to_vec),
        enforces_secure_chat,
    }
}

/// `util.PngInfo` (the IHDR fields).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PngInfo {
    pub width: i32,
    pub height: i32,
    pub bit_depth: i8,
    pub color_type: i8,
}

impl PngInfo {
    /// `PngInfo.fromBytes`; the error is the message of the `IOException` vanilla throws.
    pub fn from_bytes(bytes: &[u8]) -> Result<PngInfo, String> {
        const EOF: &str = "java.io.EOFException";
        let take = |at: usize, len: usize| bytes.get(at..at + len).ok_or_else(|| EOF.to_owned());
        let be_i32 = |at: usize| take(at, 4).map(|b| i32::from_be_bytes([b[0], b[1], b[2], b[3]]));
        let magic = take(0, 8)?;
        if magic != [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A] {
            let value = u64::from_be_bytes(magic.try_into().expect("8 bytes"));
            return Err(format!("Bad PNG Signature: 0x{value:016X}"));
        }
        let header_size = be_i32(8)?;
        if header_size != 13 {
            return Err(format!("Bad length for IHDR chunk: {header_size}"));
        }
        let header_type = be_i32(12)?;
        if header_type != 0x4948_4452 {
            return Err(format!("Bad type for IHDR chunk: 0x{:08X}", header_type as u32));
        }
        let width = be_i32(16)?;
        let height = be_i32(20)?;
        let bit_depth = take(24, 1)?[0] as i8;
        if !matches!(bit_depth, 1 | 2 | 4 | 8 | 16) {
            return Err(format!("Invalid bit depth: {bit_depth}"));
        }
        let color_type = take(25, 1)?[0] as i8;
        if !matches!(color_type, 0 | 2 | 3 | 4 | 6) {
            return Err(format!("Invalid color type: {color_type}"));
        }
        if bit_depth == 16 {
            let buffer_estimate = 4i64.wrapping_mul(width as i64).wrapping_mul(height as i64).wrapping_mul(2);
            if buffer_estimate != buffer_estimate as i32 as i64 {
                return Err(format!("Bounds check failed, {width}x{height}@{bit_depth}, type: {color_type}"));
            }
        }
        Ok(PngInfo { width, height, bit_depth, color_type })
    }
}

/// `MinecraftServer.loadStatusIcon`: `server-icon.png` in the server directory, else the world's `icon.png`; must be a
/// 64x64 PNG. Failures are logged like vanilla and yield `None`.
pub fn load_status_icon(server_icon: &Path, world_icon: Option<PathBuf>) -> Option<Vec<u8>> {
    let path = if server_icon.is_file() { server_icon.to_path_buf() } else { world_icon.filter(|p| p.is_file())? };
    let result = std::fs::read(&path).map_err(|e| format!("java.nio.file.NoSuchFileException: {e}")).and_then(|contents| {
        let info = PngInfo::from_bytes(&contents).map_err(|e| format!("java.io.IOException: {e}"))?;
        if info.width == 64 && info.height == 64 {
            Ok(contents)
        } else {
            Err(format!("java.lang.IllegalArgumentException: Invalid world icon size [{}, {}], but expected [64, 64]", info.width, info.height))
        }
    });
    match result {
        Ok(contents) => Some(contents),
        Err(e) => {
            log::error!("Couldn't load server icon\n{e}");
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mc_core::random::RandomSource;

    fn png_header(w: i32, h: i32, depth: u8, color: u8) -> Vec<u8> {
        let mut b = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0, 0, 0, 13, b'I', b'H', b'D', b'R'];
        b.extend(w.to_be_bytes());
        b.extend(h.to_be_bytes());
        b.extend([depth, color]);
        b
    }

    #[test]
    fn png_info() {
        assert_eq!(PngInfo::from_bytes(&png_header(64, 64, 8, 6)), Ok(PngInfo { width: 64, height: 64, bit_depth: 8, color_type: 6 }));
        assert_eq!(PngInfo::from_bytes(&png_header(64, 64, 3, 6)), Err("Invalid bit depth: 3".into()));
        assert_eq!(PngInfo::from_bytes(&png_header(64, 64, 8, 5)), Err("Invalid color type: 5".into()));
        assert_eq!(PngInfo::from_bytes(&png_header(40000, 40000, 16, 6)), Err("Bounds check failed, 40000x40000@16, type: 6".into()));
        assert_eq!(PngInfo::from_bytes(b"GIF89a\0\0"), Err("Bad PNG Signature: 0x4749463839610000".into()));
        assert_eq!(PngInfo::from_bytes(&png_header(1, 1, 8, 2)[..20]), Err("java.io.EOFException".into()));
    }

    #[test]
    fn icon_must_be_64x64() {
        let dir = tempfile::tempdir().unwrap();
        let icon = dir.path().join("server-icon.png");
        assert_eq!(load_status_icon(&icon, None), None);
        std::fs::write(&icon, png_header(32, 64, 8, 6)).unwrap();
        assert_eq!(load_status_icon(&icon, None), None);
        std::fs::write(&icon, png_header(64, 64, 8, 6)).unwrap();
        assert_eq!(load_status_icon(&icon, None), Some(png_header(64, 64, 8, 6)));
        // world icon is the fallback
        let world_icon = dir.path().join("icon.png");
        std::fs::write(&world_icon, png_header(64, 64, 8, 2)).unwrap();
        assert_eq!(load_status_icon(&dir.path().join("none.png"), Some(world_icon)), Some(png_header(64, 64, 8, 2)));
    }

    /// `tests/fixtures/lifecycle/StatusGolden.java`: sample selection + shuffle with the real `Mth`/`Util`/`RandomSource`.
    #[test]
    fn status_sample_matches_vanilla() {
        let mut lines = 0;
        let mut random = mc_core::random::create(0);
        let mut current_seed = 0i64;
        for line in include_str!("../../tests/fixtures/lifecycle/status_sample.txt").lines() {
            let mut parts = line.splitn(3, ' ');
            let seed: i64 = parts.next().unwrap().parse().unwrap();
            let n: usize = parts.next().unwrap().parse().unwrap();
            let expected = parts.next().unwrap_or("");
            if seed != current_seed || lines == 0 {
                current_seed = seed;
                random = mc_core::random::create(seed);
            }
            let players: Vec<_> = (0..n)
                .map(|i| StatusPlayer { name_and_id: (Uuid::from_u128(i as u128 + 1), format!("p{i}")), allows_listing: i % 4 != 3 })
                .collect();
            let status = build_player_status(&players, 20, false, &mut random);
            let names: Vec<_> = status.sample.iter().map(|(_, name)| name.as_str()).collect();
            assert_eq!(names.join(","), expected, "seed {seed} players {n}");
            assert_eq!((status.max, status.online), (20, n as i32));
            lines += 1;
        }
        assert_eq!(lines, 32);
    }

    #[test]
    fn hidden_players_have_no_sample_and_consume_no_randomness() {
        let mut random = mc_core::random::create(1);
        let players: Vec<_> = (0..3).map(|i| StatusPlayer { name_and_id: (Uuid::from_u128(i + 1), format!("p{i}")), allows_listing: true }).collect();
        let hidden = build_player_status(&players, 20, true, &mut random);
        assert_eq!((hidden.max, hidden.online, hidden.sample.len()), (20, 3, 0));
        assert_eq!(random.next_int(), mc_core::random::create(1).next_int());
    }
}
