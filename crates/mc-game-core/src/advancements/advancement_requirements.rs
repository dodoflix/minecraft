//! `net.minecraft.advancements.AdvancementRequirements`: the criteria groups (AND of ORs) an advancement needs.

use mc_core::math::java::java_string_hash;
use mc_dfu::{Codec, STRING};

use crate::stats::hash_order::{DEFAULT_TABLE, object_open_hash_order};

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct AdvancementRequirements {
    pub requirements: Vec<Vec<String>>,
}

/// `new ObjectOpenHashSet<>()` filled with `values` in order: the distinct values in its iteration order.
pub fn object_open_hash_set_order<'a>(values: impl IntoIterator<Item = &'a String>) -> Vec<String> {
    let mut distinct: Vec<&String> = Vec::new();
    for v in values {
        if !distinct.contains(&v) {
            distinct.push(v);
        }
    }
    let hashes: Vec<i32> = distinct.iter().map(|s| java_string_hash(s)).collect();
    object_open_hash_order(&hashes, DEFAULT_TABLE)
        .into_iter()
        .map(|i| distinct[i].clone())
        .collect()
}

impl AdvancementRequirements {
    pub const EMPTY: AdvancementRequirements = AdvancementRequirements {
        requirements: Vec::new(),
    };

    pub fn new(requirements: Vec<Vec<String>>) -> Self {
        AdvancementRequirements { requirements }
    }

    /// `AdvancementRequirements.CODEC`.
    pub fn codec() -> Codec<AdvancementRequirements> {
        STRING.list_of().list_of().xmap(
            AdvancementRequirements::new,
            |r: &AdvancementRequirements| r.requirements.clone(),
        )
    }

    /// `allOf(criteria)`: every criterion is its own group.
    pub fn all_of<'a>(criteria: impl IntoIterator<Item = &'a String>) -> Self {
        AdvancementRequirements {
            requirements: criteria.into_iter().map(|c| vec![c.clone()]).collect(),
        }
    }

    /// `anyOf(criteria)`: one group with every criterion.
    pub fn any_of<'a>(criteria: impl IntoIterator<Item = &'a String>) -> Self {
        AdvancementRequirements {
            requirements: vec![criteria.into_iter().cloned().collect()],
        }
    }

    pub fn size(&self) -> usize {
        self.requirements.len()
    }

    /// `test(predicate)`: false for no groups.
    pub fn test(&self, predicate: impl Fn(&str) -> bool) -> bool {
        if self.requirements.is_empty() {
            return false;
        }
        self.requirements
            .iter()
            .all(|set| set.iter().any(|c| predicate(c)))
    }

    /// `count(predicate)`: the satisfied groups.
    pub fn count(&self, predicate: impl Fn(&str) -> bool) -> usize {
        self.requirements
            .iter()
            .filter(|set| set.iter().any(|c| predicate(c)))
            .count()
    }

    /// `validate(expectedCriteria)`.
    pub fn validate(&self, expected_criteria: &[String]) -> Result<(), String> {
        let mut referenced: Vec<&String> = Vec::new();
        for set in &self.requirements {
            if set.is_empty() && expected_criteria.is_empty() {
                return Err("Requirement entry cannot be empty".to_owned());
            }
            for c in set {
                if !referenced.contains(&c) {
                    referenced.push(c);
                }
            }
        }
        let same = referenced.len() == expected_criteria.len()
            && expected_criteria.iter().all(|c| referenced.contains(&c));
        if same {
            return Ok(());
        }
        let missing: Vec<&String> = expected_criteria
            .iter()
            .filter(|c| !referenced.contains(c))
            .collect();
        let unknown: Vec<String> = object_open_hash_set_order(
            referenced
                .iter()
                .copied()
                .filter(|c| !expected_criteria.contains(c)),
        );
        Err(format!(
            "Advancement completion requirements did not exactly match specified criteria. Missing: [{}]. Unknown: [{}]",
            missing
                .iter()
                .map(|s| s.as_str())
                .collect::<Vec<_>>()
                .join(", "),
            unknown.join(", ")
        ))
    }

    pub fn is_empty(&self) -> bool {
        self.requirements.is_empty()
    }

    /// `names()`: every referenced criterion, in `ObjectOpenHashSet` iteration order.
    pub fn names(&self) -> Vec<String> {
        object_open_hash_set_order(self.requirements.iter().flatten())
    }
}

impl From<&AdvancementRequirements> for mc_protocol::packets::play::AdvancementRequirements {
    fn from(r: &AdvancementRequirements) -> Self {
        mc_protocol::packets::play::AdvancementRequirements {
            requirements: r.requirements.clone(),
        }
    }
}

/// `AdvancementRequirements.Strategy`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Strategy {
    And,
    Or,
}

impl Strategy {
    pub fn create<'a>(
        self,
        criteria: impl IntoIterator<Item = &'a String>,
    ) -> AdvancementRequirements {
        match self {
            Strategy::And => AdvancementRequirements::all_of(criteria),
            Strategy::Or => AdvancementRequirements::any_of(criteria),
        }
    }
}
