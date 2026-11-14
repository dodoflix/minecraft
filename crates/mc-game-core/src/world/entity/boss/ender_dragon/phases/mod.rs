//! `net.minecraft.world.entity.boss.enderdragon.phases`: the behaviour of the ender dragon, one phase at a time.
//!
//! Vanilla keeps one `DragonPhaseInstance` per phase in `EnderDragonPhaseManager`, created on first use and kept for the dragon's
//! life (a path, a counter or a target stays in an instance while another phase runs). Here the state of every phase lives in
//! [`DragonPhases`] inside the dragon and each phase module holds the functions that act on its state, so a phase may reach the
//! state of another phase (`getPhase(X).setTarget(..)`) through the dragon.

#![allow(
    clippy::manual_range_contains,
    reason = "`x < a || x > b` is false for NaN (an `acos` of a dot product just over 1), `!(a..=b).contains(&x)` is true"
)]

mod charge_player;
mod death;
mod holding_pattern;
mod hover;
mod landing;
mod landing_approach;
mod sitting_attacking;
mod sitting_flaming;
mod sitting_scanning;
mod strafe_player;
mod takeoff;

use super::EnderDragon;
use crate::server::level::ServerLevel;
use crate::world::damagesource::DamageSource;
use crate::world::entity::projectile::hurting::windcharge::WindCharge;
use crate::world::entity::{Entity, EntityId};
use mc_core::phys::Vec3;
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_data::EntityType;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EnderDragonPhase {
    HoldingPattern = 0,
    StrafePlayer = 1,
    LandingApproach = 2,
    Landing = 3,
    Takeoff = 4,
    SittingFlaming = 5,
    SittingScanning = 6,
    SittingAttacking = 7,
    ChargingPlayer = 8,
    Dying = 9,
    Hovering = 10,
}

impl EnderDragonPhase {
    /// `EnderDragonPhase.getById(id)`: an unknown id is the holding pattern.
    pub fn by_id(id: i32) -> Self {
        match id {
            1 => Self::StrafePlayer,
            2 => Self::LandingApproach,
            3 => Self::Landing,
            4 => Self::Takeoff,
            5 => Self::SittingFlaming,
            6 => Self::SittingScanning,
            7 => Self::SittingAttacking,
            8 => Self::ChargingPlayer,
            9 => Self::Dying,
            10 => Self::Hovering,
            _ => Self::HoldingPattern,
        }
    }

    /// `getId()`.
    pub fn id(self) -> i32 {
        self as i32
    }

    /// `DragonPhaseInstance.isSitting()`: the three sitting phases and the hover phase.
    pub fn is_sitting(self) -> bool {
        matches!(
            self,
            Self::SittingFlaming | Self::SittingScanning | Self::SittingAttacking | Self::Hovering
        )
    }
}

/// The instances of `EnderDragonPhaseManager.phases`: the state each phase keeps.
#[derive(Default)]
pub struct DragonPhases {
    pub(super) charge_player: charge_player::State,
    pub(super) dying: death::State,
    pub(super) holding_pattern: holding_pattern::State,
    pub(super) hover: hover::State,
    pub(super) landing: landing::State,
    pub(super) landing_approach: landing_approach::State,
    pub(super) sitting_attacking: sitting_attacking::State,
    pub(super) sitting_flaming: sitting_flaming::State,
    pub(super) sitting_scanning: sitting_scanning::State,
    pub(super) strafe_player: strafe_player::State,
    pub(super) takeoff: takeoff::State,
}

/// `phase.begin()`.
pub(super) fn begin(dragon: &mut EnderDragon, phase: EnderDragonPhase) {
    use EnderDragonPhase as P;
    match phase {
        P::HoldingPattern => holding_pattern::begin(dragon),
        P::StrafePlayer => strafe_player::begin(dragon),
        P::LandingApproach => landing_approach::begin(dragon),
        P::Landing => landing::begin(dragon),
        P::Takeoff => takeoff::begin(dragon),
        P::SittingFlaming => sitting_flaming::begin(dragon),
        P::SittingScanning => sitting_scanning::begin(dragon),
        P::SittingAttacking => sitting_attacking::begin(dragon),
        P::ChargingPlayer => charge_player::begin(dragon),
        P::Dying => death::begin(dragon),
        P::Hovering => hover::begin(dragon),
    }
}

/// `phase.end()`: only the flaming phase has something to undo, its cloud. Without a level (a phase restored from saved data)
/// the cloud is discarded at the dragon's next tick.
pub(super) fn end(
    dragon: &mut EnderDragon,
    level: Option<&mut ServerLevel>,
    phase: EnderDragonPhase,
) {
    if phase == EnderDragonPhase::SittingFlaming {
        sitting_flaming::end(dragon, level);
    }
}

/// The clouds of a flaming phase that ended without a level, discarded at the start of the dragon's tick.
pub(super) fn discard_pending(dragon: &mut EnderDragon, level: &mut ServerLevel) {
    sitting_flaming::discard_pending(dragon, level);
}

/// `phase.doServerTick(level)`.
pub(super) fn do_server_tick(dragon: &mut EnderDragon, level: &mut ServerLevel) {
    use EnderDragonPhase as P;
    match dragon.phase() {
        P::HoldingPattern => holding_pattern::do_server_tick(dragon, level),
        P::StrafePlayer => strafe_player::do_server_tick(dragon, level),
        P::LandingApproach => landing_approach::do_server_tick(dragon, level),
        P::Landing => landing::do_server_tick(dragon, level),
        P::Takeoff => takeoff::do_server_tick(dragon, level),
        P::SittingFlaming => sitting_flaming::do_server_tick(dragon, level),
        P::SittingScanning => sitting_scanning::do_server_tick(dragon, level),
        P::SittingAttacking => sitting_attacking::do_server_tick(dragon, level),
        P::ChargingPlayer => charge_player::do_server_tick(dragon, level),
        P::Dying => death::do_server_tick(dragon, level),
        P::Hovering => hover::do_server_tick(dragon),
    }
}

/// `phase.getFlyTargetLocation()`.
pub(super) fn fly_target_location(dragon: &EnderDragon) -> Option<Vec3> {
    use EnderDragonPhase as P;
    let phases = &dragon.phases;
    match dragon.phase() {
        P::HoldingPattern => phases.holding_pattern.target_location,
        P::StrafePlayer => phases.strafe_player.target_location,
        P::LandingApproach => phases.landing_approach.target_location,
        P::Landing => phases.landing.target_location,
        P::Takeoff => phases.takeoff.target_location,
        P::ChargingPlayer => phases.charge_player.target_location,
        P::Dying => phases.dying.target_location,
        P::Hovering => phases.hover.target_location,
        P::SittingFlaming | P::SittingScanning | P::SittingAttacking => None,
    }
}

/// `phase.getFlySpeed()`.
pub(super) fn fly_speed(dragon: &EnderDragon) -> f32 {
    use EnderDragonPhase as P;
    match dragon.phase() {
        P::ChargingPlayer | P::Dying => 3.0,
        P::Landing => 1.5,
        P::Hovering => 1.0,
        _ => 0.6,
    }
}

/// `phase.getTurnSpeed()`.
pub(super) fn turn_speed(dragon: &EnderDragon) -> f32 {
    let rot_speed = dragon.base().delta_movement().horizontal_distance() as f32 + 1.0;
    let dist = rot_speed.min(40.0);
    if dragon.phase() == EnderDragonPhase::Landing {
        dist / rot_speed
    } else {
        0.7 / dist / rot_speed
    }
}

/// `phase.onHurt(source, damage)`: the sitting phases stop arrows and wind charges (and set them on fire).
pub(super) fn on_hurt(
    dragon: &EnderDragon,
    level: &mut ServerLevel,
    source: &DamageSource,
    damage: f32,
) -> f32 {
    if !matches!(
        dragon.phase(),
        EnderDragonPhase::SittingFlaming
            | EnderDragonPhase::SittingScanning
            | EnderDragonPhase::SittingAttacking
    ) {
        return damage;
    }
    let stopped = source.get_direct_entity().and_then(|direct| {
        level.try_with_entity(direct, |_, entity| {
            let stops = is_abstract_arrow(entity) || entity.as_any().is::<WindCharge>();
            if stops {
                entity.ignite_for_seconds(1.0);
            }
            stops
        })
    });
    if stopped == Some(true) { 0.0 } else { damage }
}

/// `phase.onCrystalDestroyed(crystal, pos, source, player)`: the holding pattern turns on the player.
pub(super) fn on_crystal_destroyed(
    dragon: &mut EnderDragon,
    level: &mut ServerLevel,
    player: Option<EntityId>,
) {
    if dragon.phase() == EnderDragonPhase::HoldingPattern {
        holding_pattern::on_crystal_destroyed(dragon, level, player);
    }
}

/// `instanceof AbstractArrow`: arrows, spectral arrows and tridents.
fn is_abstract_arrow(entity: &dyn Entity) -> bool {
    matches!(
        entity.get_type(),
        EntityType::ARROW | EntityType::SPECTRAL_ARROW | EntityType::TRIDENT
    )
}

/// The shared tail of every `navigateToNextPathNode`: a point above the node, `nextFloat() * 20.0F` blocks at most (redrawn
/// while the float sum rounds below the node).
fn random_target_above(random: &mut impl RandomSource, node: BlockPos) -> Vec3 {
    let node_y = node.y();
    let y_target = loop {
        let y = f64::from(node_y as f32 + random.next_float() * 20.0);
        if y >= f64::from(node_y) {
            break y;
        }
    };
    Vec3::new(f64::from(node.x()), y_target, f64::from(node.z()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_round_trip_and_unknown_ids_hold_the_pattern() {
        for id in 0..=10 {
            assert_eq!(EnderDragonPhase::by_id(id).id(), id);
        }
        assert_eq!(
            EnderDragonPhase::by_id(-1),
            EnderDragonPhase::HoldingPattern
        );
        assert_eq!(
            EnderDragonPhase::by_id(11),
            EnderDragonPhase::HoldingPattern
        );
    }

    #[test]
    fn only_the_sitting_phases_and_the_hover_sit() {
        let sitting: Vec<i32> = (0..=10)
            .filter(|id| EnderDragonPhase::by_id(*id).is_sitting())
            .collect();
        assert_eq!(sitting, [5, 6, 7, 10]);
    }
}
