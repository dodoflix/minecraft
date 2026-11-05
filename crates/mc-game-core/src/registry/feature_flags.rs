//! `net.minecraft.world.flag` helpers on top of `mc_world`'s `FeatureFlagSet` (which only covers what level data needs):
//! `subtract`, `intersects`, `is_empty`, `FeatureFlags.printMissingFlags` and the JSON codec used by `pack.mcmeta`.

use mc_nbt::{ListTag, Tag};
use mc_world::level_storage::FeatureFlagSet;
use serde_json::Value;

const SINGLES: [FeatureFlagSet; 4] =
    [FeatureFlagSet::VANILLA, FeatureFlagSet::TRADE_REBALANCE, FeatureFlagSet::REDSTONE_EXPERIMENTS, FeatureFlagSet::MINECART_IMPROVEMENTS];

fn bits(set: FeatureFlagSet) -> u8 {
    SINGLES.iter().enumerate().fold(0, |acc, (i, f)| if set.contains_all(*f) { acc | 1 << i } else { acc })
}

fn from_bits(bits: u8) -> FeatureFlagSet {
    SINGLES.iter().enumerate().fold(FeatureFlagSet::EMPTY, |acc, (i, f)| if bits & (1 << i) != 0 { acc.join(*f) } else { acc })
}

/// The `FeatureFlagSet` methods `mc_world` does not expose.
pub trait FeatureFlagSetExt: Sized {
    fn is_empty(self) -> bool;
    fn subtract(self, other: Self) -> Self;
    fn intersects(self, other: Self) -> bool;
}

impl FeatureFlagSetExt for FeatureFlagSet {
    fn is_empty(self) -> bool {
        bits(self) == 0
    }

    fn subtract(self, other: Self) -> Self {
        from_bits(bits(self) & !bits(other))
    }

    fn intersects(self, other: Self) -> bool {
        bits(self) & bits(other) != 0
    }
}

/// `FeatureFlags.printMissingFlags(allowed, requested)`: ids requested but not allowed, joined by `", "`.
pub fn print_missing_flags(allowed: FeatureFlagSet, requested: FeatureFlagSet) -> String {
    requested.subtract(allowed).names().iter().map(|n| n.to_string()).collect::<Vec<_>>().join(", ")
}

/// `FeatureFlags.CODEC` decoded from JSON (`pack.mcmeta` `features.enabled`).
pub fn decode_json(value: &Value) -> Result<FeatureFlagSet, String> {
    let list = value.as_array().ok_or_else(|| format!("Not a list: {value}"))?;
    let mut tag = ListTag::new();
    for v in list {
        tag.push(Tag::String(v.as_str().ok_or_else(|| format!("Not a string: {v}"))?.to_owned()));
    }
    FeatureFlagSet::decode(&Tag::List(tag))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_algebra() {
        let all = FeatureFlagSet::ALL;
        assert!(FeatureFlagSet::EMPTY.is_empty());
        assert!(!FeatureFlagSet::VANILLA.is_empty());
        assert_eq!(all.subtract(FeatureFlagSet::VANILLA).is_subset_of(all), true);
        assert!(!all.subtract(FeatureFlagSet::VANILLA).contains_all(FeatureFlagSet::VANILLA));
        assert!(FeatureFlagSet::TRADE_REBALANCE.join(FeatureFlagSet::VANILLA).intersects(FeatureFlagSet::VANILLA));
        assert!(!FeatureFlagSet::TRADE_REBALANCE.intersects(FeatureFlagSet::VANILLA));
        assert_eq!(print_missing_flags(FeatureFlagSet::VANILLA, FeatureFlagSet::TRADE_REBALANCE.join(FeatureFlagSet::VANILLA)), "minecraft:trade_rebalance");
    }

    #[test]
    fn json_codec() {
        assert_eq!(decode_json(&serde_json::json!(["minecraft:vanilla"])).unwrap(), FeatureFlagSet::VANILLA);
        assert!(decode_json(&serde_json::json!(["minecraft:nope"])).is_err());
    }
}
