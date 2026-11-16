//! `net.minecraft.world.level.dimension.end.DragonRespawnStage`.

use super::ender_dragon_fight as fight;
use crate::server::level::ServerLevel;
use crate::server::level::level_explode::Explode;
use crate::world::entity::boss::end_crystal::EndCrystal;
use crate::world::entity::{Entity, EntityId, RemovalReason};
use crate::world::level::LevelAccessor as _;
use crate::world::level::explosion::ExplosionInteraction;
use crate::world::level::levelgen::feature::end_spike::{EndSpike, get_spikes_for_level};
use crate::world::level::levelgen::feature::end_spike_feature::EndSpikeFeature;
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
            Self::SummoningPillars => {
                let start_of_beam = time % 40 == 0;
                let end_of_beam = time % 40 == 39;
                if start_of_beam || end_of_beam {
                    let spikes = get_spikes_for_level(level);
                    match spikes.get((time / 40) as usize) {
                        Some(spike) if start_of_beam => set_beam_target(
                            level,
                            crystals,
                            Some(BlockPos::new(
                                spike.get_center_x(),
                                spike.get_height() + 1,
                                spike.get_center_z(),
                            )),
                        ),
                        Some(spike) => rebuild_pillar(level, spike),
                        None if start_of_beam => {
                            fight::set_respawn_stage(level, Self::SummoningDragon);
                        }
                        None => {}
                    }
                }
            }
            Self::SummoningDragon => {
                if time >= 100 {
                    fight::set_respawn_stage(level, Self::End);
                    fight::reset_spike_crystals(level);
                    for &id in crystals {
                        explode_crystal(level, id);
                    }
                } else if time >= 80 {
                    level.level_event(None, 3001, BEAM_POS, 0);
                } else if time == 0 {
                    set_beam_target(level, crystals, Some(BEAM_POS));
                } else if time < 5 {
                    level.level_event(None, 3001, BEAM_POS, 0);
                }
            }
            Self::End => {}
        }
    }
}

/// Clears the 21x21x21 box around a pillar, explodes it and builds it again with an invulnerable crystal beaming at the
/// podium.
fn rebuild_pillar(level: &mut ServerLevel, spike: &EndSpike) {
    let (x, y, z) = (
        spike.get_center_x(),
        spike.get_height(),
        spike.get_center_z(),
    );
    for pos in BlockPos::between_closed(
        &BlockPos::new(x - 10, y - 10, z - 10),
        &BlockPos::new(x + 10, y + 10, z + 10),
    ) {
        level.remove_block(pos, false);
    }
    level.explode(Explode::new(
        None,
        f64::from(x) + 0.5,
        f64::from(y),
        f64::from(z) + 0.5,
        5.0,
        ExplosionInteraction::Block,
    ));
    let feature = EndSpikeFeature {
        spikes: vec![spike.clone()],
        crystal_invulnerable: true,
        crystal_beam_target: Some(BEAM_POS),
    };
    level.place_feature_value_with_unique_seed(&feature, BlockPos::new(x, 45, z));
}

/// `crystal.setBeamTarget(null); level.explode(crystal, x, y, z, 6.0F, NONE); crystal.discard()`: an earlier blast of the ritual
/// may have removed the crystal already, vanilla's list still holds it and it still explodes.
fn explode_crystal(level: &mut ServerLevel, id: EntityId) {
    fn run(level: &mut ServerLevel, crystal: &mut dyn Entity) {
        if let Some(crystal) = crystal.as_any_mut().downcast_mut::<EndCrystal>() {
            crystal.set_beam_target(None);
        }
        let position = crystal.position();
        level.explode(Explode::new(
            Some(&mut *crystal),
            position.x,
            position.y,
            position.z,
            6.0,
            ExplosionInteraction::None,
        ));
        if !crystal.is_removed() {
            level.remove_held_entity(crystal, RemovalReason::Discarded);
        }
    }
    if level.try_with_entity(id, run).is_none() {
        level.with_recently_removed(id, run);
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
