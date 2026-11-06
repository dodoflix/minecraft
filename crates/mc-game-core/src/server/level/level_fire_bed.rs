//! Level-side reads used by fire and beds: `dimensionType().infiniburn()` and `canSleepThroughNights()`.

use super::ServerLevel;
use crate::world::attribute::BedRule;
use crate::world::level::block::BlockStateExt;
use std::sync::Arc;

impl ServerLevel {
    /// `state.is(level.dimensionType().infiniburn())`.
    pub fn is_infiniburn(&self, state: mc_data::block::BlockState) -> bool {
        use crate::registry::typed::HolderSetRef;
        match &self.tick_state.infiniburn {
            HolderSetRef::Tag(tag) => state.is_tag(tag.path()),
            HolderSetRef::Elements(ids) => ids
                .iter()
                .any(|id| id.to_string() == state.block().data().name),
        }
    }

    /// `canSleepThroughNights()`: `playersSleepingPercentage <= 100`.
    pub fn can_sleep_through_nights(&self) -> bool {
        self.players_sleeping_percentage() <= 100
    }

    /// `getBedRule(level, pos)` of a bed block (`straw` selects `STRAW_BED_RULE`).
    pub fn bed_rule_at(&self, straw: bool, pos: mc_core::pos::BlockPos) -> Arc<BedRule> {
        use crate::world::attribute::attributes;
        self.attribute_bed_rule(
            if straw {
                attributes::STRAW_BED_RULE
            } else {
                attributes::BED_RULE
            },
            pos,
        )
    }
}
