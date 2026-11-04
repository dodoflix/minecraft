//! `net.minecraft.world.flag.FeatureFlags` + `FeatureFlagSet` restricted to what level data needs: the four
//! built-in `main`-universe flags, name conversion and the `FeatureFlags.CODEC` list encoding.

use mc_core::ident::Identifier;
use mc_nbt::{ListTag, Tag};

use super::dynamic::{java_hash_set_order, string_list};

/// Registry order == flag id (bit index) in vanilla's `FeatureFlags` static initializer.
const NAMES: [&str; 4] = ["vanilla", "trade_rebalance", "redstone_experiments", "minecart_improvements"];

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct FeatureFlagSet(u64);

impl FeatureFlagSet {
    pub const EMPTY: FeatureFlagSet = FeatureFlagSet(0);
    pub const VANILLA: FeatureFlagSet = FeatureFlagSet(1 << 0);
    pub const TRADE_REBALANCE: FeatureFlagSet = FeatureFlagSet(1 << 1);
    pub const REDSTONE_EXPERIMENTS: FeatureFlagSet = FeatureFlagSet(1 << 2);
    pub const MINECART_IMPROVEMENTS: FeatureFlagSet = FeatureFlagSet(1 << 3);
    /// `FeatureFlags.VANILLA_SET`.
    pub const VANILLA_SET: FeatureFlagSet = FeatureFlagSet::VANILLA;
    /// `FeatureFlags.DEFAULT_FLAGS`.
    pub const DEFAULT_FLAGS: FeatureFlagSet = FeatureFlagSet::VANILLA;
    /// `FeatureFlags.REGISTRY.allFlags()`.
    pub const ALL: FeatureFlagSet = FeatureFlagSet(0b1111);

    pub const fn join(self, other: FeatureFlagSet) -> FeatureFlagSet {
        FeatureFlagSet(self.0 | other.0)
    }

    pub const fn contains_all(self, other: FeatureFlagSet) -> bool {
        other.0 & !self.0 == 0
    }

    /// `FeatureFlagSet.isSubsetOf`.
    pub const fn is_subset_of(self, other: FeatureFlagSet) -> bool {
        other.contains_all(self)
    }

    /// `FeatureFlags.isExperimental`.
    pub const fn is_experimental(self) -> bool {
        !self.is_subset_of(FeatureFlagSet::VANILLA_SET)
    }

    /// `FeatureFlagRegistry.fromNames(ids, unknown)`: unknown ids are reported through `unknown`.
    pub fn from_names<'a>(ids: impl IntoIterator<Item = &'a Identifier>, mut unknown: impl FnMut(&Identifier)) -> Self {
        let mut bits = 0;
        for id in ids {
            match (id.namespace() == "minecraft").then(|| NAMES.iter().position(|n| *n == id.path())).flatten() {
                Some(bit) => bits |= 1 << bit,
                None => unknown(id),
            }
        }
        FeatureFlagSet(bits)
    }

    /// `FeatureFlagRegistry.toNames(set)` rendered in the order of the `HashSet<Identifier>` vanilla builds
    /// (and then copies into the encoded list).
    pub fn names(self) -> Vec<Identifier> {
        let items = NAMES
            .iter()
            .enumerate()
            .filter(|(bit, _)| self.0 & (1 << bit) != 0)
            .map(|(_, n)| {
                let id = Identifier::with_default_namespace(n).expect("valid built-in flag name");
                (id.java_hash_code(), id)
            })
            .collect();
        java_hash_set_order(items)
    }

    /// `FeatureFlags.CODEC` decode: a list of identifiers, unknown/invalid ids make it fail (`Err` message).
    pub fn decode(tag: &Tag) -> Result<FeatureFlagSet, String> {
        let strings = string_list(tag).ok_or_else(|| "Not a list of strings".to_owned())?;
        let ids = strings.iter().map(|s| Identifier::read(s)).collect::<Result<Vec<_>, _>>()?;
        let mut unknown = Vec::new();
        let set = FeatureFlagSet::from_names(&ids, |id| unknown.push(id.to_string()));
        if unknown.is_empty() { Ok(set) } else { Err(format!("Unknown feature ids: {unknown:?}")) }
    }

    /// `FeatureFlags.CODEC` encode.
    pub fn encode(self) -> Tag {
        let mut list = ListTag::new();
        for id in self.names() {
            list.push(Tag::String(id.to_string()));
        }
        Tag::List(list)
    }
}
