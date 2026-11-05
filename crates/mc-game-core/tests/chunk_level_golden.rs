//! Differential tests against vanilla for `ChunkLevel`, both `ChunkPyramid`s, the `TicketStorage` codec and
//! `ChunkTrackingView` (`tests/fixtures/chunkmap/ChunkLevelGolden.java`).

extern crate mc_game_core as mc_game;
use mc_core::pos::ChunkPos;
use mc_game::server::level::chunk_level;
use mc_game::server::level::{ChunkTrackingView, FullChunkStatus};
use mc_game::world::level::chunk::status::{GENERATION_PYRAMID, LOADING_PYRAMID, max_chunk_coordinate_value};
use mc_game::world::level::ticket_storage::TicketStorage;
use mc_nbt::snbt;
use mc_world::status::ChunkStatus;

const GOLDEN: &str = include_str!("../../mc-game/tests/fixtures/chunkmap/chunk_level_golden.txt");

fn status_name(s: ChunkStatus) -> String {
    format!("minecraft:{}", s.path())
}

#[test]
fn chunk_level_and_pyramids() {
    let mut pyramid = None;
    for line in GOLDEN.lines() {
        let parts: Vec<&str> = line.split(' ').collect();
        match parts[0] {
            "MAX_LEVEL" => {
                assert_eq!(chunk_level::max_level().to_string(), parts[1]);
                assert_eq!(chunk_level::RADIUS_AROUND_FULL_CHUNK.to_string(), parts[3]);
            }
            "BYSTATUS" => {
                let status = ChunkStatus::ALL.iter().find(|s| status_name(**s) == parts[1]).unwrap();
                assert_eq!(chunk_level::by_status(*status).to_string(), parts[2], "byStatus {}", parts[1]);
            }
            "GENSTATUS" => {
                let level: i32 = parts[1].parse().unwrap();
                let got = chunk_level::generation_status(level).map_or("none".to_string(), status_name);
                assert_eq!(got, parts[2], "generationStatus({level})");
                let full = match chunk_level::full_status(level) {
                    FullChunkStatus::Inaccessible => "INACCESSIBLE",
                    FullChunkStatus::Full => "FULL",
                    FullChunkStatus::BlockTicking => "BLOCK_TICKING",
                    FullChunkStatus::EntityTicking => "ENTITY_TICKING",
                };
                assert_eq!(full, parts[3], "fullStatus({level})");
            }
            "PYRAMID" => pyramid = Some(if parts[1] == "gen" { &*GENERATION_PYRAMID } else { &*LOADING_PYRAMID }),
            "STEP" => {
                let step = pyramid.unwrap().steps().iter().find(|s| status_name(s.target_status) == parts[1]).unwrap();
                let list = |deps: &mc_game::world::level::chunk::status::ChunkDependencies| {
                    format!("[{}]", deps.as_list().iter().map(|s| status_name(*s)).collect::<Vec<_>>().join(", "))
                };
                let line_rest = line.split_once(" direct=").unwrap().1;
                let (direct, rest) = line_rest.split_once(" acc=").unwrap();
                let (acc, write) = rest.split_once(" write=").unwrap();
                assert_eq!(list(&step.direct_dependencies), direct, "{} direct", parts[1]);
                assert_eq!(list(&step.accumulated_dependencies), acc, "{} accumulated", parts[1]);
                assert_eq!(step.block_state_write_radius.to_string(), write, "{} write radius", parts[1]);
            }
            "MAXCOORD" => assert_eq!(max_chunk_coordinate_value().to_string(), parts[1]),
            _ => {}
        }
    }
}

#[test]
fn ticket_storage_codec_matches_vanilla() {
    let mut ts = TicketStorage::new();
    use mc_game::server::level::{Ticket, TicketType};
    ts.add_ticket(Ticket::new(TicketType::FORCED, 31), ChunkPos::new(3, -4));
    ts.add_ticket(Ticket::new(TicketType::PORTAL, 30), ChunkPos::new(0, 0));
    ts.add_ticket(Ticket::new(TicketType::PLAYER_LOADING, 31), ChunkPos::new(5, 5));
    ts.add_ticket(Ticket::new(TicketType::UNKNOWN, 20), ChunkPos::new(-7, 9));
    for i in 0..6 {
        ts.add_ticket(Ticket::new(TicketType::FORCED, 31), ChunkPos::new(i * 11 - 20, 100 - i * 7));
    }
    let inputs = [
        "{tickets:[{chunk_pos:[I;1,2],type:\"minecraft:portal\",level:30,ticks_left:5L}]}",
        "{tickets:[{chunk_pos:[I;1,2],type:\"minecraft:portal\",level:30}]}",
        "{}",
        "{tickets:[]}",
        "{tickets:[{chunk_pos:[I;1,2,3],type:\"minecraft:portal\",level:30}]}",
        "{tickets:[{chunk_pos:[I;1,2],type:\"minecraft:nope\",level:30}]}",
        "{tickets:[{chunk_pos:[I;1,2],type:\"minecraft:portal\",level:-1}]}",
        "{tickets:[{chunk_pos:[I;1,2],type:\"minecraft:portal\",level:3.9d,ticks_left:7b}]}",
    ];
    let mut dec = 0;
    for line in GOLDEN.lines() {
        if let Some(expected) = line.strip_prefix("ENC ") {
            assert_eq!(ts.to_tag(), snbt::parse(expected).unwrap(), "encode");
        } else if let Some(rest) = line.strip_prefix("DEC ") {
            let parsed = TicketStorage::from_tag(&mc_nbt::Tag::Compound(snbt::parse_compound(inputs[dec]).unwrap()));
            if let Some(expected) = rest.strip_prefix("ok ") {
                let storage = parsed.unwrap_or_else(|e| panic!("input {dec} should decode: {e}"));
                assert_eq!(storage.to_tag(), snbt::parse(expected).unwrap(), "decode+encode of input {dec}");
            } else {
                assert!(parsed.is_err(), "input {dec} should fail");
            }
            dec += 1;
        }
    }
    assert_eq!(dec, inputs.len());
}

fn parse_view(s: &str) -> ChunkTrackingView {
    if s == "E" {
        return ChunkTrackingView::Empty;
    }
    let p: Vec<i32> = s.split(',').map(|n| n.parse().unwrap()).collect();
    ChunkTrackingView::of(ChunkPos::new(p[0], p[1]), p[2])
}

#[test]
fn chunk_tracking_view_difference_matches_vanilla() {
    let mut n = 0;
    for line in GOLDEN.lines().filter(|l| l.starts_with("DIFF ")) {
        let (head, rest) = line.split_once(" | ").unwrap();
        let (enter, leave) = rest.split_once(" | ").unwrap_or((rest, ""));
        let h: Vec<&str> = head.split(' ').collect();
        let (from, to) = (parse_view(h[1]), parse_view(h[2]));
        let (mut got_enter, mut got_leave) = (Vec::new(), Vec::new());
        ChunkTrackingView::difference(&from, &to, |p| got_enter.push(format!("{},{}", p.x, p.z)), |p| got_leave.push(format!("{},{}", p.x, p.z)));
        assert_eq!(got_enter.join(" "), enter.trim(), "enter of {line}");
        assert_eq!(got_leave.join(" "), leave.trim(), "leave of {line}");
        n += 1;
    }
    assert_eq!(n, 60);
}
