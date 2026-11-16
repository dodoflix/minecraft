//! `net.minecraft.world.level.dimension.end.DragonRespawnStage`.

use super::ender_dragon_fight as fight;
use crate::server::level::ServerLevel;
use crate::world::entity::EntityId;
use crate::world::entity::boss::end_crystal::EndCrystal;
use crate::world::level::LevelAccessor as _;
use mc_core::pos::BlockPos;

/// `DragonRespawnStage`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DragonRespawnStage {
    Start,
    PreparingToSummonPillars,
    SummoningPillars,
    SummoningDragon,
    End,
}

/// The block every crystal of the ritual beams at first (`new BlockPos(0, 128, 0)`).
const BEAM_POS: BlockPos = BlockPos::new(0, 128, 0);

impl DragonRespawnStage {
    const ALL: [DragonRespawnStage; 5] = [
        Self::Start,
        Self::PreparingToSummonPillars,
        Self::SummoningPillars,
        Self::SummoningDragon,
        Self::End,
    ];

    /// `getSerializedName()`.
    pub fn get_serialized_name(self) -> &'static str {
        match self {
            Self::Start => "start",
            Self::PreparingToSummonPillars => "preparing_to_summon_pillars",
            Self::SummoningPillars => "summoning_pillars",
            Self::SummoningDragon => "summoning_dragon",
            Self::End => "end",
        }
    }

    /// `CODEC` decode.
    pub fn by_name(name: &str) -> Option<DragonRespawnStage> {
        Self::ALL
            .into_iter()
            .find(|s| s.get_serialized_name() == name)
    }

    /// `tick(level, fight, crystals, time)`.
    pub(super) fn tick(self, level: &mut ServerLevel, crystals: &[EntityId], time: i32) {
        match self {
            Self::Start => {
                set_beam_target(level, crystals, Some(BEAM_POS));
                fight::set_respawn_stage(level, Self::PreparingToSummonPillars);
            }
            Self::PreparingToSummonPillars => {
                if time < 100 {
                    if time == 0 || time == 50 || time == 51 || time == 52 || time >= 95 {
                        level.level_event(None, 3001, BEAM_POS, 0);
                    }
                } else {
                    fight::set_respawn_stage(level, Self::SummoningPillars);
                }
            }
            // Not implemented: the stages after the preparation (the pillar beams and explosions, the dragon summon, the end of
            // the ritual): a fight saved in one of them stays in it.
            Self::SummoningPillars | Self::SummoningDragon | Self::End => {}
        }
    }
}

/// `crystal.setBeamTarget(target)` for each of the ritual's crystals.
fn set_beam_target(level: &mut ServerLevel, crystals: &[EntityId], target: Option<BlockPos>) {
    for &id in crystals {
        level.with_entity(id, |_, crystal| {
            if let Some(crystal) = crystal.as_any_mut().downcast_mut::<EndCrystal>() {
                crystal.set_beam_target(target);
            }
        });
    }
}
