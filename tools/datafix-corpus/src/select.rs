//! The corpus of one `DataFixTypes` constant: duplicates dropped, then a greedy set cover over the structural features
//! (every shape of the candidates is in at least one sample), then a seeded random fill up to the count and size
//! budget. Every kept sample is expanded into one update request per target version.

use crate::candidate::{Candidate, era};
use crate::features::fnv1a;
use base64::{Engine, engine::general_purpose::STANDARD};
use std::cmp::Reverse;
use std::collections::{BTreeMap, BinaryHeap, HashSet};

pub struct Params {
    /// Most samples kept.
    pub count: usize,
    /// Most gzip bytes of inputs kept.
    pub budget: u64,
    /// A sample above this gzip size is kept only as one of the `OVERSIZE_PER_ERA` richest of its era.
    pub max_gz: u64,
    /// The size no sample may exceed.
    pub max_gz_oversize: u64,
    pub seed: u64,
}

/// Samples per era that may exceed `Params::max_gz`.
const OVERSIZE_PER_ERA: usize = 3;

struct Sized {
    c: Candidate,
    gz: u64,
}

/// Samples of one type with the same version and the same features exercise the same fixes: the smallest of them stays.
fn dedup_sized(cands: Vec<Candidate>) -> Vec<Sized> {
    let mut best: BTreeMap<(String, i32, Vec<u64>), Sized> = BTreeMap::new();
    for c in cands {
        let s = Sized { gz: c.gz_size(), c };
        let key = (s.c.ty.clone(), s.c.from, s.c.features.clone());
        match best.get(&key) {
            Some(old) if (old.gz, &old.c.id) <= (s.gz, &s.c.id) => {}
            _ => {
                best.insert(key, s);
            }
        }
    }
    best.into_values().collect()
}

/// `dedup_sized` without the sizes, sorted by id.
pub fn dedup(cands: Vec<Candidate>) -> Vec<Candidate> {
    let mut out: Vec<Candidate> = dedup_sized(cands).into_iter().map(|s| s.c).collect();
    out.sort_by(|a, b| a.id.cmp(&b.id));
    out
}

/// The kept candidates, sorted by id.
pub fn select(cands: Vec<Candidate>, p: &Params) -> Vec<Candidate> {
    let all = dedup_sized(cands);
    let mut picked: Vec<usize> = Vec::new();
    let mut used = 0u64;
    let mut covered: HashSet<u64> = HashSet::new();
    let take = |i: usize, picked: &mut Vec<usize>, used: &mut u64, covered: &mut HashSet<u64>| {
        picked.push(i);
        *used += all[i].gz;
        covered.extend(all[i].c.features.iter().copied());
    };
    // the richest of each era that exceed the usual size
    let mut by_era: BTreeMap<u8, Vec<usize>> = BTreeMap::new();
    for (i, s) in all.iter().enumerate() {
        if s.gz > p.max_gz && s.gz <= p.max_gz_oversize {
            by_era.entry(era(s.c.from)).or_default().push(i);
        }
    }
    for list in by_era.values_mut() {
        list.sort_by_key(|&i| (Reverse(all[i].c.features.len()), &all[i].c.id));
        for &i in list.iter().take(OVERSIZE_PER_ERA) {
            if picked.len() < p.count && used + all[i].gz <= p.budget {
                take(i, &mut picked, &mut used, &mut covered);
            }
        }
    }
    let fits = |s: &Sized| s.gz <= p.max_gz;
    // lazy greedy cover: a stale gain only ever overestimates
    let mut heap: BinaryHeap<(usize, Reverse<u64>, Reverse<&str>, usize)> = all
        .iter()
        .enumerate()
        .filter(|(i, s)| fits(s) && !picked.contains(i))
        .map(|(i, s)| {
            (
                s.c.features.len(),
                Reverse(s.gz),
                Reverse(s.c.id.as_str()),
                i,
            )
        })
        .collect();
    while picked.len() < p.count {
        let Some((stale, gz, id, i)) = heap.pop() else {
            break;
        };
        let gain = all[i]
            .c
            .features
            .iter()
            .filter(|f| !covered.contains(f))
            .count();
        if gain == 0 {
            continue;
        }
        if gain < stale {
            heap.push((gain, gz, id, i));
            continue;
        }
        if used + all[i].gz <= p.budget {
            take(i, &mut picked, &mut used, &mut covered);
        }
    }
    // seeded fill
    let chosen: HashSet<usize> = picked.iter().copied().collect();
    let mut rest: Vec<usize> = (0..all.len())
        .filter(|i| fits(&all[*i]) && !chosen.contains(i))
        .collect();
    rest.sort_by_key(|&i| {
        (
            fnv1a(format!("{}:{}", p.seed, all[i].c.id).as_bytes()),
            &all[i].c.id,
        )
    });
    for i in rest {
        if picked.len() >= p.count {
            break;
        }
        if used + all[i].gz <= p.budget {
            take(i, &mut picked, &mut used, &mut covered);
        }
    }
    let mut out: Vec<Candidate> = picked.into_iter().map(|i| all[i].c.clone()).collect();
    out.sort_by(|a, b| a.id.cmp(&b.id));
    out
}

/// The batch input of the oracle (`ID TYPE FROM TO BASE64`): per sample one line for every target above its version, the
/// newest target (`current`) included; a sample at or above `current` has nothing to update and gets none.
pub fn requests(samples: &[Candidate], targets: &[i32], current: i32) -> Vec<String> {
    let mut lines = Vec::new();
    for c in samples {
        let b64 = STANDARD.encode(&c.bytes);
        let mut to: Vec<i32> = targets
            .iter()
            .copied()
            .filter(|t| *t > c.from && *t < current)
            .collect();
        if c.from < current {
            to.push(current);
        }
        for t in to {
            lines.push(format!("{}\t{}\t{}\t{t}\t{b64}", c.id, c.ty, c.from));
        }
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cand(id: &str, from: i32, features: &[u64], len: usize) -> Candidate {
        // incompressible bytes of a given length
        let bytes = (0..len)
            .map(|i| (fnv1a(format!("{id}{i}").as_bytes()) >> 7) as u8)
            .collect();
        Candidate {
            id: id.into(),
            ty: "CHUNK".into(),
            from,
            bytes,
            features: features.to_vec(),
        }
    }

    fn params(count: usize) -> Params {
        Params {
            count,
            budget: u64::MAX,
            max_gz: 1000,
            max_gz_oversize: 100_000,
            seed: 1,
        }
    }

    #[test]
    fn duplicates_collapse_to_the_smallest() {
        let kept = select(
            vec![
                cand("b", 5, &[1, 2], 100),
                cand("a", 5, &[1, 2], 200),
                cand("c", 6, &[1, 2], 50),
            ],
            &params(10),
        );
        let ids: Vec<&str> = kept.iter().map(|c| c.id.as_str()).collect();
        assert_eq!(ids, ["b", "c"]);
    }

    #[test]
    fn the_cover_comes_before_the_fill() {
        let cands = vec![
            cand("a", 1, &[1, 2, 3], 50),
            cand("b", 2, &[3, 4], 50),
            cand("c", 3, &[1], 50),
            cand("d", 4, &[2], 50),
        ];
        let ids: Vec<String> = select(cands, &params(2))
            .into_iter()
            .map(|c| c.id)
            .collect();
        // `a` has the most features, then `b` the only other one with a feature `a` lacks
        assert_eq!(ids, ["a", "b"]);
    }

    #[test]
    fn the_budget_and_the_count_bound_the_corpus() {
        let cands: Vec<Candidate> = (0..20)
            .map(|i| cand(&format!("s{i:02}"), i, &[100 + i as u64], 400))
            .collect();
        assert_eq!(select(cands.clone(), &params(7)).len(), 7);
        let one = cands[0].gz_size();
        let kept = select(
            cands,
            &Params {
                budget: one * 3 + 1,
                ..params(20)
            },
        );
        assert!(kept.len() <= 3 && !kept.is_empty());
    }

    #[test]
    fn only_the_richest_oversize_samples_of_an_era_stay() {
        let mut cands: Vec<Candidate> = (0..6)
            .map(|i| {
                cand(
                    &format!("big{i}"),
                    2600,
                    &(0..=i as u64).collect::<Vec<_>>(),
                    5000,
                )
            })
            .collect();
        cands.push(cand("small", 2600, &[99], 100));
        let ids: Vec<String> = select(cands, &params(10))
            .into_iter()
            .map(|c| c.id)
            .collect();
        assert_eq!(ids, ["big3", "big4", "big5", "small"]);
    }

    #[test]
    fn the_selection_is_a_function_of_its_input() {
        let cands: Vec<Candidate> = (0..50)
            .map(|i| cand(&format!("s{i:02}"), i % 4, &[i as u64 % 3], 100))
            .collect();
        let a = select(cands.clone(), &params(8));
        let mut reversed = cands;
        reversed.reverse();
        assert_eq!(a, select(reversed, &params(8)));
    }

    #[test]
    fn every_target_above_the_version_gets_a_request() {
        let c = cand("x", 2000, &[1], 10);
        let lines = requests(&[c], &[1343, 2199, 3299, 5023], 5023);
        let to: Vec<&str> = lines
            .iter()
            .map(|l| l.split('\t').nth(3).unwrap())
            .collect();
        assert_eq!(to, ["2199", "3299", "5023"]);
        assert!(requests(&[cand("y", 5023, &[1], 10)], &[1343, 5023], 5023).is_empty());
    }
}
