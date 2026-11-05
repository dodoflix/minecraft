//! `PiglinArmPose`.

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PiglinArmPose {
    AttackingWithMeleeWeapon,
    CrossbowHold,
    CrossbowCharge,
    AdmiringItem,
    Dancing,
    Default,
}
