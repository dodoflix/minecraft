//! Golden tests for `mc_core::bits` against vanilla `SimpleBitStorage` (fixtures/bits/BitsGolden.java -> bits.txt).
use std::panic::catch_unwind;

use mc_core::bits::{BitStorage, SimpleBitStorage, ZeroBitStorage};

const FIXTURE: &str = include_str!("fixtures/bits/bits.txt");

struct SplitMix(u64);

impl SplitMix {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E3779B97F4A7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D049BB133111EB);
        z ^ (z >> 31)
    }
}

fn mix(h: i64, x: i64) -> i64 {
    h.wrapping_mul(1000003).wrapping_add(x)
}

fn hash_longs(v: &[i64]) -> i64 {
    v.iter().fold(0, |h, &x| mix(h, x))
}

#[test]
fn simple_bit_storage_matches_vanilla() {
    let mut cases = 0;
    for line in FIXTURE.lines().filter(|l| l.starts_with("case ")) {
        let t: Vec<&str> = line.split(' ').collect();
        let bits: i32 = t[1].parse().unwrap();
        let size: usize = t[2].parse().unwrap();
        let (want_acc, want_uh, want_rh, want_fh): (i64, i64, i64, i64) =
            (t[3].parse().unwrap(), t[4].parse().unwrap(), t[5].parse().unwrap(), t[6].parse().unwrap());
        let ctx = format!("bits={bits} size={size}");

        let mut rng = SplitMix(bits as u64 * 1000 + size as u64);
        let mut s = SimpleBitStorage::new(bits, size);
        let mask = (1i64 << bits) - 1;
        let mut acc = 0i64;
        if size > 0 {
            for _ in 0..300 {
                let op = rng.next() as i64 % 8; // signed `%` like Java: negative ops fall into the `set` arm
                let idx = (rng.next() % size as u64) as usize;
                let val = ((rng.next() >> 17) as i64 & mask & 0x7FFF_FFFF) as i32;
                match op {
                    ..=2 => s.set(idx, val),
                    3..=5 => acc = mix(acc, s.get_and_set(idx, val).into()),
                    6 => acc = mix(acc, s.get(idx).into()),
                    _ if rng.next() % 16 == 0 => s.fill(val),
                    _ => acc = mix(acc, s.get(idx).into()),
                }
            }
        }
        assert_eq!(acc, want_acc, "{ctx}: get/get_and_set results");

        let mut un = vec![0i32; size];
        s.unpack(&mut un);
        assert_eq!(un.iter().fold(0, |h, &v| mix(h, v.into())), want_uh, "{ctx}: unpack");
        let mut all = Vec::new();
        s.get_all(&mut |v| all.push(v));
        assert_eq!(all, un, "{ctx}: get_all");
        assert_eq!(hash_longs(s.get_raw()), want_rh, "{ctx}: raw");
        if t[7] != "-" {
            let raw: Vec<i64> = t[7].split(',').map(|x| x.parse().unwrap()).collect();
            assert_eq!(s.get_raw(), &raw[..], "{ctx}: raw longs");
        }
        let from_values = SimpleBitStorage::from_values(bits, size, &un);
        assert_eq!(hash_longs(from_values.get_raw()), want_fh, "{ctx}: values ctor raw");
        assert_eq!(s.copy().get_raw(), s.get_raw(), "{ctx}: copy");
        assert_eq!(s.get_size(), size);
        assert_eq!(s.get_bits(), bits);
        cases += 1;
    }
    assert_eq!(cases, 32 * 12);
}

#[test]
fn data_constructor_validation_matches_vanilla() {
    let mut n = 0;
    for line in FIXTURE.lines().filter(|l| l.starts_with("err ")) {
        let (lhs, msg) = line.split_once(" = ").unwrap();
        let t: Vec<&str> = lhs.split(' ').collect();
        let (bits, size, len): (i32, usize, usize) = (t[1].parse().unwrap(), t[2].parse().unwrap(), t[3].parse().unwrap());
        let err = SimpleBitStorage::with_data(bits, size, vec![0; len]).unwrap_err();
        assert_eq!(err.to_string(), msg);
        // The correct length is accepted and keeps the data.
        let ok = SimpleBitStorage::with_data(bits, size, vec![-1; len - 1]).unwrap();
        assert_eq!(ok.get_raw().len(), len - 1);
        n += 1;
    }
    assert_eq!(n, 32 * 12);
}

fn panic_message(f: impl FnOnce() + std::panic::UnwindSafe) -> String {
    let e = catch_unwind(f).unwrap_err();
    e.downcast_ref::<String>().cloned().unwrap_or_else(|| (*e.downcast_ref::<&str>().unwrap()).to_owned())
}

#[test]
fn validation_messages_match_vanilla() {
    for line in FIXTURE.lines().filter(|l| l.starts_with("bad_bits ")) {
        let (lhs, msg) = line.split_once(" = ").unwrap();
        let bits: i32 = lhs.split(' ').nth(1).unwrap().parse().unwrap();
        assert_eq!(panic_message(move || drop(SimpleBitStorage::new(bits, 4))), msg);
    }
    let expect = |key: &str| FIXTURE.lines().find_map(|l| l.strip_prefix(key)).unwrap().to_owned();
    assert_eq!(panic_message(|| SimpleBitStorage::new(4, 10).set(10, 1)), expect("oob_index = "));
    assert_eq!(panic_message(|| SimpleBitStorage::new(4, 10).set(1, 16)), expect("oob_value = "));
}

#[test]
fn zero_bit_storage() {
    let mut z = ZeroBitStorage::new(5);
    assert_eq!(z.get_size(), 5);
    assert_eq!(z.get_bits(), 0);
    assert!(z.get_raw().is_empty());
    assert_eq!(z.get(4), 0);
    assert_eq!(z.get_and_set(0, 0), 0);
    z.set(1, 0);
    z.fill(0);
    let mut out = vec![7; 6];
    z.unpack(&mut out);
    assert_eq!(out, [0, 0, 0, 0, 0, 7]);
    let mut n = 0;
    z.get_all(&mut |v| {
        assert_eq!(v, 0);
        n += 1;
    });
    assert_eq!(n, 5);
    assert_eq!(panic_message(|| ZeroBitStorage::new(5).set(0, 1)), "The value 1 is not in the specified inclusive range of 0 to 0");
    assert_eq!(panic_message(|| assert_eq!(ZeroBitStorage::new(5).get(5), 0)), "The value 5 is not in the specified inclusive range of 0 to 4");
}
