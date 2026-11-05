//! `PlayerChunkSender.collectChunksToSend` ordering vs the real fastutil/Guava classes
//! (`tests/fixtures/play_net/ChunkSenderGolden.java` -> `chunk_sender.golden`).
extern crate mc_game_core as mc_game;
use mc_core::pos::ChunkPos;
use mc_game::server::network::player_chunk_sender::least;
use mc_game::util::long_open_hash_set::LongOpenHashSet;

fn parse_list(s: &str) -> Vec<i64> {
    s.split(',')
        .filter(|e| !e.is_empty())
        .map(|e| {
            let (x, z) = e.split_once(':').unwrap();
            ChunkPos::pack_xz(x.parse().unwrap(), z.parse().unwrap())
        })
        .collect()
}

#[test]
fn set_iteration_and_selection_order_match_vanilla() {
    let golden = include_str!("../../mc-game/tests/fixtures/play_net/chunk_sender.golden");
    let mut cases = 0;
    for line in golden.lines() {
        let f: Vec<&str> = line.split(';').collect();
        let mut set = LongOpenHashSet::default();
        for op in f[0].split(',') {
            let mut p = op.split(':');
            let kind = p.next().unwrap();
            let x: i32 = p.next().unwrap().parse().unwrap();
            let z: i32 = p.next().unwrap().parse().unwrap();
            if kind == "a" { set.add(ChunkPos::pack_xz(x, z)); } else { set.remove(ChunkPos::pack_xz(x, z)); }
        }
        let (cx, cz) = f[1].split_once(',').unwrap();
        let player = ChunkPos::new(cx.parse().unwrap(), cz.parse().unwrap());
        let k: usize = f[2].parse().unwrap();
        assert_eq!(set.len(), f[3].parse::<usize>().unwrap(), "size, case {line}");
        assert_eq!(set.iter().collect::<Vec<_>>(), parse_list(f[4]), "iteration order, case {line}");
        let dist = |p: i64| player.distance_squared_long(p);
        let got = least(k, set.iter(), |a, b| dist(*a).cmp(&dist(*b)));
        assert_eq!(got, parse_list(f[5]), "least({k}), case {line}");
        let mut sorted: Vec<i64> = set.iter().collect();
        sorted.sort_by_key(|p| dist(*p));
        assert_eq!(sorted, parse_list(f[6]), "sorted, case {line}");
        cases += 1;
    }
    assert_eq!(cases, 400);
}
