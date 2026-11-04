//! Differential test against the real vanilla `DistanceManager` trackers + `TicketStorage` + fastutil hash iteration
//! order (`tests/fixtures/chunkmap/DistanceGolden.java`).

use mc_core::pos::{ChunkPos, SectionPos};
use mc_game::server::level::chunk_level;
use mc_game::server::level::distance_manager::{DistanceHost, DistanceManager};
use mc_game::server::level::fastutil::{LongOpenHashMap, LongOpenHashSet};
use mc_game::server::level::{Ticket, TicketType};
use mc_game::world::level::ticket_storage::TicketStorage;
use std::collections::HashMap;

struct Host {
    levels: HashMap<i64, i32>,
    to_drop: LongOpenHashSet,
    log: Vec<String>,
}

impl DistanceHost for Host {
    fn is_chunk_to_remove(&self, node: i64) -> bool {
        self.to_drop.contains(node)
    }
    fn holder_ticket_level(&self, node: i64) -> Option<i32> {
        self.levels.get(&node).copied()
    }
    fn update_chunk_scheduling(&mut self, node: i64, level: i32, old_level: i32) -> bool {
        if !chunk_level::is_loaded(old_level) && !chunk_level::is_loaded(level) {
            return self.levels.contains_key(&node);
        }
        let exists = self.levels.contains_key(&node);
        if exists {
            self.levels.insert(node, level);
            if !chunk_level::is_loaded(level) {
                self.to_drop.add(node);
            } else {
                self.to_drop.remove(node);
            }
        }
        let mut result = exists;
        if chunk_level::is_loaded(level) && !exists {
            self.levels.insert(node, level);
            result = true;
        }
        let pos = ChunkPos::unpack(node);
        self.log.push(format!("L set {},{} {}", pos.x, pos.z, level));
        result
    }
}

fn parse_key_list(line: &str) -> Vec<String> {
    line.split(' ').skip(1).map(str::to_string).collect()
}

#[test]
fn distance_manager_matches_vanilla() {
    let golden = include_str!("fixtures/chunkmap/distance_golden.txt");
    let types = [
        TicketType::PLAYER_SPAWN,
        TicketType::DRAGON,
        TicketType::PLAYER_LOADING,
        TicketType::PLAYER_SIMULATION,
        TicketType::FORCED,
        TicketType::PORTAL,
        TicketType::ENDER_PEARL,
        TicketType::UNKNOWN,
    ];
    let mut tickets = TicketStorage::new();
    let mut dm = DistanceManager::new();
    let mut host = Host { levels: HashMap::new(), to_drop: LongOpenHashSet::new(), log: Vec::new() };
    let mut lines = golden.lines().peekable();
    let mut updates = 0;
    let mut map: LongOpenHashMap<u8> = LongOpenHashMap::new();
    let mut set = LongOpenHashSet::new();
    while let Some(line) = lines.next() {
        let parts: Vec<&str> = line.split(' ').collect();
        match parts[0] {
            "A" => {
                let (t, level, x, z): (usize, i32, i32, i32) = (parts[1].parse().unwrap(), parts[2].parse().unwrap(), parts[3].parse().unwrap(), parts[4].parse().unwrap());
                tickets.add_ticket(Ticket::new(types[t], level), ChunkPos::new(x, z));
            }
            "R" => {
                let (t, level, x, z): (usize, i32, i32, i32) = (parts[1].parse().unwrap(), parts[2].parse().unwrap(), parts[3].parse().unwrap(), parts[4].parse().unwrap());
                tickets.remove_ticket(&Ticket::new(types[t], level), ChunkPos::new(x, z));
            }
            "P" => {
                let (id, x, z): (i32, i32, i32) = (parts[1].parse().unwrap(), parts[2].parse().unwrap(), parts[3].parse().unwrap());
                dm.add_player(SectionPos::of(x, 0, z), id, &mut tickets);
            }
            "Q" => {
                let (id, x, z): (i32, i32, i32) = (parts[1].parse().unwrap(), parts[2].parse().unwrap(), parts[3].parse().unwrap());
                dm.remove_player(SectionPos::of(x, 0, z), id, &mut tickets);
            }
            "U" => {
                updates += 1;
                host.log.clear();
                dm.run_updates(&mut host, &mut tickets);
                dm.chunks_to_update_futures.clear();
                let mut expected_log = Vec::new();
                while lines.peek().is_some_and(|l| l.starts_with("L ")) {
                    expected_log.push(lines.next().unwrap().to_string());
                }
                assert_eq!(host.log, expected_log, "loading setLevel sequence at update {updates}");
                let t_line = lines.next().unwrap();
                let s_line = lines.next().unwrap();
                for z in -22..=22 {
                    let g_line = lines.next().unwrap();
                    let expected: Vec<&str> = g_line.split(' ').skip(1).collect();
                    let got: Vec<String> = (-22..=22)
                        .map(|x| {
                            let k = ChunkPos::pack_xz(x, z);
                            format!("{}/{}", dm.get_chunk_level(&mut host, k, false), dm.get_chunk_level(&mut host, k, true))
                        })
                        .collect();
                    assert_eq!(got, expected, "levels row z={z} at update {updates}");
                }
                let mut ticking = Vec::new();
                dm.for_each_entity_ticking_chunk(|k| {
                    let p = ChunkPos::unpack(k);
                    ticking.push(format!("{},{}", p.x, p.z));
                });
                assert_eq!(ticking, parse_key_list(t_line), "entity ticking order at update {updates}");
                let spawn: Vec<String> = dm
                    .get_spawn_candidate_chunks()
                    .into_iter()
                    .map(|k| {
                        let p = ChunkPos::unpack(k);
                        format!("{},{}", p.x, p.z)
                    })
                    .collect();
                assert_eq!(spawn, parse_key_list(s_line), "spawn candidate order at update {updates}");
                // `ChunkMap.processUnloads`
                let mut dropped = Vec::new();
                host.to_drop.drain_filter_in_order(|k| {
                    dropped.push(k);
                    true
                });
                for k in dropped {
                    host.levels.remove(&k);
                }
                let n_line = lines.next().unwrap();
                let n: Vec<&str> = n_line.split(' ').collect();
                assert_eq!(dm.get_natural_spawn_chunk_count().to_string(), n[1], "natural spawn count at update {updates}");
                assert_eq!(tickets.get_force_loaded_chunks().len().to_string(), n[2]);
            }
            "fp" => {
                let k: i64 = parts[1].parse().unwrap();
                map.insert(k, 1);
                set.add(k);
            }
            "fr" => {
                let k: i64 = parts[1].parse().unwrap();
                map.remove(k);
                set.remove(k);
            }
            "FM" => {
                let got: Vec<String> = map.keys().iter().map(i64::to_string).collect();
                let expected: Vec<String> = parts[1..].iter().map(|e| e.split(':').next().unwrap().to_string()).collect();
                assert_eq!(got, expected, "Long2ByteOpenHashMap iteration order");
            }
            "FS" => {
                let got: Vec<String> = set.iter().map(|k| k.to_string()).collect();
                assert_eq!(got, parts[1..].iter().map(|s| s.to_string()).collect::<Vec<_>>(), "LongOpenHashSet iteration order");
            }
            "FR" => {
                set.drain_filter_in_order(|k| ((k >> 3) & 1) == 0);
                let got: Vec<String> = set.iter().map(|k| k.to_string()).collect();
                assert_eq!(got, parts[1..].iter().map(|s| s.to_string()).collect::<Vec<_>>(), "iteration order after remove()");
            }
            "E" => {
                map = LongOpenHashMap::new();
                set = LongOpenHashSet::new();
            }
            other => panic!("unexpected golden line {other}"),
        }
    }
    assert!(updates >= 60);
}
