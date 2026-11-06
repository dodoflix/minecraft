//! `net.minecraft.advancements.AdvancementProgress` + `CriterionProgress`.

use std::cmp::Ordering;

use chrono::{DateTime, Local, TimeZone};
use mc_dfu::{BOOL, Codec, DataResult, STRING};
use mc_protocol::codec::Instant;
use mc_text::{Component, Num, TranslatableArg};

use super::advancement_requirements::AdvancementRequirements;
use super::java_hash::JavaHashMap;

/// `CriterionProgress`: the epoch milliseconds the criterion was obtained at.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct CriterionProgress {
    pub obtained: Option<i64>,
}

impl CriterionProgress {
    pub fn is_done(&self) -> bool {
        self.obtained.is_some()
    }

    /// `grant()`: obtained now.
    pub fn grant(&mut self) {
        self.obtained = Some(Local::now().timestamp_millis());
    }

    pub fn revoke(&mut self) {
        self.obtained = None;
    }
}

const OBTAINED_TIME_FORMAT: &str = "%Y-%m-%d %H:%M:%S %z";

/// `OBTAINED_TIME_CODEC`: `yyyy-MM-dd HH:mm:ss Z` in the system zone; sub-second precision is dropped by the format.
fn obtained_time_codec() -> Codec<i64> {
    STRING.comap_flat_map(
        |s: String| match DateTime::parse_from_str(&s, OBTAINED_TIME_FORMAT) {
            Ok(t) => DataResult::success(t.timestamp() * 1000),
            Err(_) => {
                DataResult::error(move || format!("Text '{s}' could not be parsed at index 0"))
            }
        },
        |millis: &i64| {
            Local
                .timestamp_millis_opt(*millis)
                .single()
                .map(|t| t.format(OBTAINED_TIME_FORMAT).to_string())
                .unwrap_or_default()
        },
    )
}

/// The serialized shape (`criteria` map of obtained times, `done`).
#[derive(Clone, PartialEq)]
struct Raw {
    criteria: Vec<(String, i64)>,
    done: bool,
}

#[derive(Clone, Default)]
pub struct AdvancementProgress {
    /// `HashMap<String, CriterionProgress>` in Java iteration order.
    criteria: JavaHashMap<String, CriterionProgress>,
    requirements: AdvancementRequirements,
}

impl AdvancementProgress {
    pub fn new() -> Self {
        Self::default()
    }

    /// `AdvancementProgress.CODEC`: only done criteria are written; `done` is always written and ignored on read.
    pub fn codec() -> Codec<AdvancementProgress> {
        static C: std::sync::LazyLock<Codec<AdvancementProgress>> =
            std::sync::LazyLock::new(|| {
                let criteria = STRING.unbounded_map(&obtained_time_codec());
                mc_dfu::record!(Raw {
                    criteria: criteria.optional_field_of_default("criteria", Vec::new()),
                    done: BOOL
                        .optional_field_of_default("done", true)
                        .xmap(|d| d, |d: &bool| *d),
                })
                .codec()
                .xmap(
                    |raw: Raw| {
                        // `Util.mapValues` (Collectors.toMap -> HashMap) then `new HashMap<>(criteria)`
                        let mut staged: JavaHashMap<String, CriterionProgress> = JavaHashMap::new();
                        for (k, t) in raw.criteria {
                            staged.insert(k, CriterionProgress { obtained: Some(t) });
                        }
                        let size = staged.len();
                        let entries: Vec<(String, CriterionProgress)> =
                            staged.iter().map(|(k, v)| (k.clone(), *v)).collect();
                        AdvancementProgress {
                            criteria: JavaHashMap::copy_of(entries, size),
                            requirements: AdvancementRequirements::EMPTY,
                        }
                    },
                    |p: &AdvancementProgress| {
                        // `Collectors.toMap` over the done entries into a fresh HashMap
                        let mut out: JavaHashMap<String, i64> = JavaHashMap::new();
                        for (k, v) in p.criteria.iter() {
                            if let Some(t) = v.obtained {
                                out.insert(k.clone(), t);
                            }
                        }
                        Raw {
                            criteria: out.iter().map(|(k, v)| (k.clone(), *v)).collect(),
                            done: p.is_done(),
                        }
                    },
                )
            });
        C.clone()
    }

    /// `update(requirements)`.
    pub fn update(&mut self, requirements: &AdvancementRequirements) {
        let names = requirements.names();
        let stale: Vec<String> = self
            .criteria
            .keys()
            .filter(|k| !names.contains(k))
            .cloned()
            .collect();
        for k in stale {
            self.criteria.remove(&k);
        }
        for name in names {
            if !self.criteria.contains_key(&name) {
                self.criteria.insert(name, CriterionProgress::default());
            }
        }
        self.requirements = requirements.clone();
    }

    pub fn is_done(&self) -> bool {
        self.requirements.test(|c| self.is_criterion_done(c))
    }

    pub fn has_progress(&self) -> bool {
        self.criteria.values().any(CriterionProgress::is_done)
    }

    /// `grantProgress(name)`.
    pub fn grant_progress(&mut self, name: &str) -> bool {
        match self.criteria.get_mut(&name.to_owned()) {
            Some(p) if !p.is_done() => {
                p.grant();
                true
            }
            _ => false,
        }
    }

    /// `revokeProgress(name)`.
    pub fn revoke_progress(&mut self, name: &str) -> bool {
        match self.criteria.get_mut(&name.to_owned()) {
            Some(p) if p.is_done() => {
                p.revoke();
                true
            }
            _ => false,
        }
    }

    pub fn get_criterion(&self, id: &str) -> Option<&CriterionProgress> {
        self.criteria.get(&id.to_owned())
    }

    fn is_criterion_done(&self, criterion: &str) -> bool {
        self.get_criterion(criterion)
            .is_some_and(CriterionProgress::is_done)
    }

    /// `getPercent()`.
    pub fn get_percent(&self) -> f32 {
        if self.criteria.is_empty() {
            return 0.0;
        }
        let total = self.requirements.size() as f32;
        let complete = self.count_completed_requirements() as f32;
        complete / total
    }

    /// `getProgressText()`.
    pub fn get_progress_text(&self) -> Option<Component> {
        if self.criteria.is_empty() {
            return None;
        }
        let total = self.requirements.size();
        if total <= 1 {
            return None;
        }
        let complete = self.count_completed_requirements();
        Some(Component::translatable(
            "advancements.progress",
            vec![
                TranslatableArg::Number(Num::Int(complete as i32)),
                TranslatableArg::Number(Num::Int(total as i32)),
            ],
        ))
    }

    fn count_completed_requirements(&self) -> usize {
        self.requirements.count(|c| self.is_criterion_done(c))
    }

    /// `getRemainingCriteria()`.
    pub fn get_remaining_criteria(&self) -> Vec<String> {
        self.criteria
            .iter()
            .filter(|(_, v)| !v.is_done())
            .map(|(k, _)| k.clone())
            .collect()
    }

    /// `getCompletedCriteria()`.
    pub fn get_completed_criteria(&self) -> Vec<String> {
        self.criteria
            .iter()
            .filter(|(_, v)| v.is_done())
            .map(|(k, _)| k.clone())
            .collect()
    }

    /// `getFirstProgressDate()`.
    pub fn get_first_progress_date(&self) -> Option<i64> {
        self.criteria.values().filter_map(|c| c.obtained).min()
    }

    /// `compareTo`: earlier first progress sorts first, no progress last.
    pub fn compare_to(&self, other: &AdvancementProgress) -> Ordering {
        match (
            self.get_first_progress_date(),
            other.get_first_progress_date(),
        ) {
            (None, Some(_)) => Ordering::Greater,
            (Some(_), None) => Ordering::Less,
            (None, None) => Ordering::Equal,
            (Some(a), Some(b)) => a.cmp(&b),
        }
    }

    /// `STREAM_CODEC`: the criteria in Java `HashMap` order.
    pub fn to_network(&self) -> mc_protocol::packets::play::AdvancementProgress {
        mc_protocol::packets::play::AdvancementProgress {
            criteria: self
                .criteria
                .iter()
                .map(|(k, v)| {
                    (
                        k.clone(),
                        mc_protocol::packets::play::CriterionProgress {
                            obtained: v.obtained.map(Instant),
                        },
                    )
                })
                .collect(),
        }
    }
}
