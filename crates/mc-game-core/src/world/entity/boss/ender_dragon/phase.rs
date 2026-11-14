//! `EnderDragonPhase`: the ids and the sitting flag of the dragon's phases.
//!
//! Not implemented: the phase instances (`DragonPhaseInstance`: holding pattern, strafe, landing, takeoff, sitting, charge,
//! death and hover behaviour, `onHurt`, `onCrystalDestroyed`, the fly target and speed); a dragon keeps only its phase id.

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
