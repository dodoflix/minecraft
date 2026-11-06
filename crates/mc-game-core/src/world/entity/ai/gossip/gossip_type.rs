//! `GossipType`.

/// `GossipType`: `(id, weight, max, decayPerDay, decayPerTransfer)`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, PartialOrd, Ord)]
pub enum GossipType {
    MajorNegative,
    MinorNegative,
    MinorPositive,
    MajorPositive,
    Trading,
}

/// `GossipType.REPUTATION_CHANGE_PER_EVENT`.
pub const REPUTATION_CHANGE_PER_EVENT: i32 = 25;
/// `GossipType.REPUTATION_CHANGE_PER_EVERLASTING_MEMORY`.
pub const REPUTATION_CHANGE_PER_EVERLASTING_MEMORY: i32 = 20;
/// `GossipType.REPUTATION_CHANGE_PER_TRADE`.
pub const REPUTATION_CHANGE_PER_TRADE: i32 = 2;

impl GossipType {
    /// `GossipType.values()` (ordinal order).
    pub const ALL: [GossipType; 5] = [
        GossipType::MajorNegative,
        GossipType::MinorNegative,
        GossipType::MinorPositive,
        GossipType::MajorPositive,
        GossipType::Trading,
    ];

    /// `getSerializedName()` (`id`).
    pub const fn id(self) -> &'static str {
        match self {
            GossipType::MajorNegative => "major_negative",
            GossipType::MinorNegative => "minor_negative",
            GossipType::MinorPositive => "minor_positive",
            GossipType::MajorPositive => "major_positive",
            GossipType::Trading => "trading",
        }
    }

    pub const fn weight(self) -> i32 {
        match self {
            GossipType::MajorNegative => -5,
            GossipType::MinorNegative => -1,
            GossipType::MinorPositive => 1,
            GossipType::MajorPositive => 5,
            GossipType::Trading => 1,
        }
    }

    pub const fn max(self) -> i32 {
        match self {
            GossipType::MajorNegative => 100,
            GossipType::MinorNegative => 200,
            GossipType::MinorPositive => 25,
            GossipType::MajorPositive => 20,
            GossipType::Trading => 25,
        }
    }

    pub const fn decay_per_day(self) -> i32 {
        match self {
            GossipType::MajorNegative => 10,
            GossipType::MinorNegative => 20,
            GossipType::MinorPositive => 1,
            GossipType::MajorPositive => 0,
            GossipType::Trading => 2,
        }
    }

    pub const fn decay_per_transfer(self) -> i32 {
        match self {
            GossipType::MajorNegative => 10,
            GossipType::MinorNegative => 20,
            GossipType::MinorPositive => 5,
            GossipType::MajorPositive => 20,
            GossipType::Trading => 20,
        }
    }

    pub const fn ordinal(self) -> usize {
        self as usize
    }

    /// `CODEC` decode: by serialized name.
    pub fn by_id(id: &str) -> Option<GossipType> {
        Self::ALL.into_iter().find(|t| t.id() == id)
    }
}
