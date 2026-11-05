//! `PlayerDetector` (with `EntitySelector.SELECT_FROM_LEVEL`): which players a trial spawner / vault reacts to.
//! shortcut: the debug-only `SHEEP` detector (`SharedConstants.DEBUG_*_DETECTS_SHEEP_AS_PLAYERS`, off in release) and the
//! test-only `EntitySelector.onlySelectPlayer(s)` are not modelled.

use crate::server::level::ServerLevel;
use crate::world::entity::ServerPlayer;
use crate::world::level::collision::{BlockGetterExt, ClipBlock, ClipContext, ClipFluid, CollisionContext};
use mc_core::phys::Vec3;
use mc_core::pos::BlockPos;
use uuid::Uuid;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlayerDetector {
    /// `NO_CREATIVE_PLAYERS`.
    NoCreativePlayers,
    /// `INCLUDING_CREATIVE_PLAYERS`.
    IncludingCreativePlayers,
}

/// `inLineOfSight(level, origin, dest)`.
pub fn in_line_of_sight(level: &ServerLevel, origin: Vec3, dest: Vec3) -> bool {
    let hit = level.clip(&ClipContext::new(dest, origin, ClipBlock::Visual, ClipFluid::None, CollisionContext::empty()));
    hit.block_pos == BlockPos::containing(origin.x, origin.y, origin.z) || hit.miss
}

impl PlayerDetector {
    /// `detect(level, selector, spawnerPos, requiredPlayerRange, requireLineOfSight)` with `SELECT_FROM_LEVEL`.
    pub fn detect(self, level: &ServerLevel, spawner_pos: BlockPos, required_player_range: f64, require_line_of_sight: bool) -> Vec<Uuid> {
        let origin = Vec3::at_center_of(&spawner_pos);
        let mut found = Vec::new();
        for &id in level.players() {
            let Some(entity) = level.entity_manager.get_any(id) else { continue };
            let creative = entity.as_any().downcast_ref::<ServerPlayer>().is_some_and(ServerPlayer::is_creative);
            let eligible = entity.block_position().closer_than(&spawner_pos, required_player_range)
                && (self == PlayerDetector::IncludingCreativePlayers || !creative)
                && !entity.is_spectator();
            if eligible && (!require_line_of_sight || in_line_of_sight(level, origin, entity.base().get_eye_position())) {
                found.push(entity.uuid());
            }
        }
        found
    }
}
