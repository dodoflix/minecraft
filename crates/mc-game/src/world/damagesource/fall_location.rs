//! `net.minecraft.world.damagesource.FallLocation` and `CombatEntry`.

use mc_data::block::Block;

use crate::server::level::ServerLevel;
use crate::world::entity::Entity;
use crate::world::level::level_access::BlockGetter as _;

/// `FallLocation(id)`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FallLocation(pub &'static str);

impl FallLocation {
    pub const GENERIC: FallLocation = FallLocation("generic");
    pub const LADDER: FallLocation = FallLocation("ladder");
    pub const VINES: FallLocation = FallLocation("vines");
    pub const WEEPING_VINES: FallLocation = FallLocation("weeping_vines");
    pub const TWISTING_VINES: FallLocation = FallLocation("twisting_vines");
    pub const SCAFFOLDING: FallLocation = FallLocation("scaffolding");
    pub const OTHER_CLIMBABLE: FallLocation = FallLocation("other_climbable");
    pub const WATER: FallLocation = FallLocation("water");

    /// `blockToFallLocation(blockState)`.
    pub fn block_to_fall_location(block: Block) -> FallLocation {
        if block == Block::LADDER || block.is_in_tag("trapdoors") {
            FallLocation::LADDER
        } else if block == Block::VINE {
            FallLocation::VINES
        } else if block == Block::WEEPING_VINES || block == Block::WEEPING_VINES_PLANT {
            FallLocation::WEEPING_VINES
        } else if block == Block::TWISTING_VINES || block == Block::TWISTING_VINES_PLANT {
            FallLocation::TWISTING_VINES
        } else if block == Block::SCAFFOLDING {
            FallLocation::SCAFFOLDING
        } else {
            FallLocation::OTHER_CLIMBABLE
        }
    }

    /// `getCurrentFallLocation(mob)`.
    pub fn get_current_fall_location(level: &ServerLevel, mob: &dyn Entity) -> Option<FallLocation> {
        match mob.living().and_then(|l| l.last_climbable_pos) {
            Some(pos) => Some(FallLocation::block_to_fall_location(level.get_block_state(pos).block())),
            None => mob.base().was_touching_water.then_some(FallLocation::WATER),
        }
    }

    /// `languageKey()`.
    pub fn language_key(&self) -> String {
        format!("death.fell.accident.{}", self.0)
    }
}
