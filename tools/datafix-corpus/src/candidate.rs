//! A candidate sample: the `NbtIo` bytes of one input of a `DataFixTypes` update, as read from a world (or generated).

use crate::features::features;
use base64::{Engine, engine::general_purpose::STANDARD};
use flate2::{Compression, write::GzEncoder};
use mc_nbt::{CompoundTag, NbtAccounter, Tag};
use std::io::Write;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Candidate {
    /// Unique and stable: the world, the file and the position the input came from.
    pub id: String,
    /// A `DataFixTypes` constant name.
    pub ty: String,
    /// The data version the load path of vanilla would update from.
    pub from: i32,
    /// `NbtIo.write` bytes of the named root compound.
    pub bytes: Vec<u8>,
    /// Sorted feature hashes (`features.rs`).
    pub features: Vec<u64>,
}

/// True when a fix would draw a random number for this input: a zombie that `IsVillager` but names no profession
/// (`EntityZombieVillagerTypeFix` uses a thread-local random source), so vanilla's answer is not reproducible.
fn draws_random_profession(tag: &Tag) -> bool {
    match tag {
        Tag::Compound(c) => {
            let villager = c
                .get("IsVillager")
                .and_then(Tag::as_number)
                .is_some_and(|n| n.int_value() != 0);
            let profession = c
                .get("VillagerProfession")
                .and_then(Tag::as_number)
                .map(|n| n.int_value());
            (villager && !c.contains("ZombieType") && !matches!(profession, Some(0..6)))
                || c.values().any(draws_random_profession)
        }
        Tag::List(l) => l.iter().any(draws_random_profession),
        _ => false,
    }
}

impl Candidate {
    /// The candidate of the bytes, or why there is none (unreadable, or its update is not reproducible).
    pub fn from_bytes(
        id: String,
        ty: &str,
        from: i32,
        bytes: Vec<u8>,
    ) -> Result<Candidate, String> {
        let root: CompoundTag =
            mc_nbt::read_named(&mut bytes.as_slice(), &mut NbtAccounter::unlimited_heap())
                .map_err(|e| format!("{id}: {e}"))?;
        if draws_random_profession(&Tag::Compound(root.clone())) {
            return Err(format!(
                "{id}: a zombie villager without a profession draws a random one"
            ));
        }
        Ok(Candidate {
            features: features(&root, from),
            id,
            ty: ty.to_string(),
            from,
            bytes,
        })
    }

    /// Size of the input as a fixture stores it (gzip).
    pub fn gz_size(&self) -> u64 {
        let mut enc = GzEncoder::new(Vec::new(), Compression::best());
        enc.write_all(&self.bytes).expect("a Vec takes any write");
        enc.finish().expect("a Vec takes any write").len() as u64
    }

    /// `ID TYPE FROM FEATURES BASE64`, tab separated (features as hex, comma separated).
    pub fn to_line(&self) -> String {
        let features: Vec<String> = self.features.iter().map(|f| format!("{f:x}")).collect();
        format!(
            "{}\t{}\t{}\t{}\t{}",
            self.id,
            self.ty,
            self.from,
            features.join(","),
            STANDARD.encode(&self.bytes)
        )
    }

    pub fn parse_line(line: &str) -> Result<Candidate, String> {
        let f: Vec<&str> = line.split('\t').collect();
        let [id, ty, from, features, bytes] = f[..] else {
            return Err(format!("a candidate line has 5 fields: {line:.80}"));
        };
        Ok(Candidate {
            id: id.to_string(),
            ty: ty.to_string(),
            from: from.parse().map_err(|e| format!("{id}: from: {e}"))?,
            bytes: STANDARD
                .decode(bytes)
                .map_err(|e| format!("{id}: base64: {e}"))?,
            features: features
                .split(',')
                .filter(|s| !s.is_empty())
                .map(|s| u64::from_str_radix(s, 16).map_err(|e| format!("{id}: feature: {e}")))
                .collect::<Result<_, _>>()?,
        })
    }
}

/// The release era of a data version: 1 = 1.7 and 1.8 (no version), 2 = 1.9 to 1.12, 3 = 1.13 to 1.15, 4 = 1.16 to
/// 1.20.4, 5 = 1.20.5 and newer.
pub fn era(from: i32) -> u8 {
    match from {
        ..0 => 1,
        0..1500 => 2,
        1500..2400 => 3,
        2400..3800 => 4,
        _ => 5,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mc_nbt::ListTag;

    fn villager_zombie(fields: &[(&str, i8)]) -> Vec<u8> {
        let mut zombie = CompoundTag::new();
        for (k, v) in fields {
            zombie.put_byte(*k, *v);
        }
        let mut list = ListTag::new();
        list.push(Tag::Compound(zombie));
        let mut root = CompoundTag::new();
        root.put("Entities", Tag::List(list));
        mc_nbt::nbt_io::to_bytes(&root)
    }

    #[test]
    fn only_a_zombie_villager_without_a_profession_is_not_reproducible() {
        let make = |fields: &[(&str, i8)]| {
            Candidate::from_bytes("x".into(), "CHUNK", 1, villager_zombie(fields))
        };
        assert!(make(&[("IsVillager", 1)]).is_err());
        assert!(make(&[("IsVillager", 1), ("VillagerProfession", 9)]).is_err());
        assert!(make(&[("IsVillager", 1), ("VillagerProfession", 3)]).is_ok());
        assert!(make(&[("IsVillager", 1), ("ZombieType", 2)]).is_ok());
        assert!(make(&[("IsVillager", 0)]).is_ok());
        assert!(make(&[]).is_ok());
    }

    #[test]
    fn a_candidate_survives_its_line() {
        let c = Candidate {
            id: "w-chunk-c1.2".into(),
            ty: "CHUNK".into(),
            from: -1,
            bytes: vec![10, 0, 0, 0],
            features: vec![1, 0xdead_beef],
        };
        assert_eq!(Candidate::parse_line(&c.to_line()), Ok(c));
    }

    #[test]
    fn eras_split_at_the_release_boundaries() {
        let eras: Vec<u8> = [-1, 184, 1343, 1519, 2230, 2586, 3700, 3839, 4903]
            .map(era)
            .into();
        assert_eq!(eras, [1, 2, 2, 3, 3, 4, 4, 5, 5]);
    }
}
