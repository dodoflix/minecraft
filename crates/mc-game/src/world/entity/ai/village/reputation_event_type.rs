//! `ReputationEventType`: the events a `ReputationEventHandler` reacts to.

/// `ReputationEventType` (identity by name).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct ReputationEventType(&'static str);

impl ReputationEventType {
    pub const ZOMBIE_VILLAGER_CURED: ReputationEventType = ReputationEventType("zombie_villager_cured");
    pub const GOLEM_KILLED: ReputationEventType = ReputationEventType("golem_killed");
    pub const VILLAGER_HURT: ReputationEventType = ReputationEventType("villager_hurt");
    pub const VILLAGER_KILLED: ReputationEventType = ReputationEventType("villager_killed");
    pub const TRADE: ReputationEventType = ReputationEventType("trade");

    /// `toString()`.
    pub fn name(self) -> &'static str {
        self.0
    }
}

impl std::fmt::Display for ReputationEventType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.0)
    }
}
