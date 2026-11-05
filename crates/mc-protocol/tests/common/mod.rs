//! Helpers shared by the play-packet golden tests: load `tests/fixtures/play/<name>.golden`
//! (`label:hex` lines from vanilla's `GameProtocols` codecs, hex = VarInt packet id + body), compare encodings
//! byte for byte and decode the vanilla bytes back.
#![allow(dead_code)]
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::fmt::Debug;

pub mod vanilla;

use mc_protocol::codec::{read_varint, write_varint};
use mc_protocol::Packet;

pub struct Golden {
    name: String,
    lines: HashMap<String, Vec<u8>>,
    used: RefCell<HashSet<String>>,
}

pub fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

impl Golden {
    pub fn load(name: &str) -> Self {
        let path = format!("{}/tests/fixtures/play/{name}.golden", env!("CARGO_MANIFEST_DIR"));
        let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path}: {e} (run fixtures/play/gen.sh)"));
        let lines = text
            .lines()
            .map(|l| {
                let (label, h) = l.split_once(':').expect("label:hex");
                let bytes = (0..h.len()).step_by(2).map(|i| u8::from_str_radix(&h[i..i + 2], 16).unwrap()).collect();
                (label.to_owned(), bytes)
            })
            .collect();
        Self { name: name.into(), lines, used: RefCell::default() }
    }

    pub fn bytes(&self, label: &str) -> &[u8] {
        self.used.borrow_mut().insert(label.to_owned());
        self.lines.get(label).unwrap_or_else(|| panic!("no fixture {}/{label}", self.name))
    }

    /// Our encoding of `p` (VarInt id + body) equals vanilla's, and vanilla's bytes decode back to `p`.
    pub fn check<P: Packet + PartialEq + Debug>(&self, label: &str, p: &P) {
        let expected = self.bytes(label);
        let mut ours = Vec::new();
        write_varint(&mut ours, P::ID);
        p.encode(&mut ours).unwrap();
        assert_eq!(hex(&ours), hex(expected), "encode {label}");
        let mut input = expected;
        let id = read_varint(&mut input).unwrap();
        assert_eq!(id, P::ID, "id {label}");
        let decoded = P::decode(&mut input).unwrap_or_else(|e| panic!("decode {label}: {e}"));
        assert!(input.is_empty(), "decode {label}: {} trailing bytes", input.len());
        assert_eq!(&decoded, p, "decode {label}");
    }

    /// Like [`check`](Self::check) for nested (non-packet) values: bytes without packet id.
    pub fn check_value<T: mc_protocol::Encode + mc_protocol::Decode + PartialEq + Debug>(&self, label: &str, v: &T) {
        let expected = self.bytes(label);
        assert_eq!(hex(&mc_protocol::codec::to_bytes(v).unwrap()), hex(expected), "encode {label}");
        assert_eq!(&mc_protocol::codec::from_bytes::<T>(expected).unwrap_or_else(|e| panic!("decode {label}: {e}")), v, "decode {label}");
    }

    /// Every fixture line was consumed by a check (no orphaned vanilla output).
    pub fn assert_all_used(&self) {
        let used = self.used.borrow();
        let mut unused: Vec<_> = self.lines.keys().filter(|k| !used.contains(*k)).collect();
        unused.sort();
        assert!(unused.is_empty(), "{}: unused fixtures {unused:?}", self.name);
    }
}
