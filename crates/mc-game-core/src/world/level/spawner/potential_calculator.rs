//! `net.minecraft.world.level.PotentialCalculator`: the spawn cost (`spawn_costs`) charge field of the mobs already in the
//! level. A candidate position may only be used while the potential energy change it causes stays within the budget.

use mc_core::pos::BlockPos;

#[derive(Clone, Copy, Debug)]
struct PointCharge {
    pos: BlockPos,
    charge: f64,
}

impl PointCharge {
    /// `PointCharge.getPotentialChange(pos)`.
    fn get_potential_change(&self, pos: BlockPos) -> f64 {
        let dist_sqr = dist_sqr(self.pos, pos);
        if dist_sqr == 0.0 {
            f64::INFINITY
        } else {
            self.charge / dist_sqr.sqrt()
        }
    }
}

/// `Vec3i.distSqr(other)`: the exact `double` arithmetic of `distToLowCornerSqr(x, y, z)`.
fn dist_sqr(a: BlockPos, b: BlockPos) -> f64 {
    let dx = f64::from(a.x()) - f64::from(b.x());
    let dy = f64::from(a.y()) - f64::from(b.y());
    let dz = f64::from(a.z()) - f64::from(b.z());
    dx * dx + dy * dy + dz * dz
}

/// `PotentialCalculator`.
#[derive(Clone, Debug, Default)]
pub struct PotentialCalculator {
    charges: Vec<PointCharge>,
}

impl PotentialCalculator {
    pub fn new() -> PotentialCalculator {
        PotentialCalculator::default()
    }

    /// `addCharge(pos, charge)`: zero charges are not stored.
    pub fn add_charge(&mut self, pos: BlockPos, charge: f64) {
        if charge != 0.0 {
            self.charges.push(PointCharge { pos, charge });
        }
    }

    /// The stored charges in insertion order (`charges` of vanilla's calculator).
    pub fn charges(&self) -> Vec<(BlockPos, f64)> {
        self.charges.iter().map(|c| (c.pos, c.charge)).collect()
    }

    /// `getPotentialEnergyChange(pos, charge)`.
    pub fn get_potential_energy_change(&self, pos: BlockPos, charge: f64) -> f64 {
        if charge == 0.0 {
            return 0.0;
        }
        let mut potential_change = 0.0;
        for point in &self.charges {
            potential_change += point.get_potential_change(pos);
        }
        potential_change * charge
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn charges_sum_by_inverse_distance() {
        let mut calculator = PotentialCalculator::new();
        calculator.add_charge(BlockPos::new(0, 64, 0), 0.15);
        calculator.add_charge(BlockPos::new(3, 64, 4), 0.0); // not stored
        calculator.add_charge(BlockPos::new(3, 64, 4), 0.5);
        assert!(
            calculator
                .get_potential_energy_change(BlockPos::new(0, 64, 0), 0.15)
                .is_infinite()
        );
        let change = calculator.get_potential_energy_change(BlockPos::new(0, 64, 8), 0.15);
        assert_eq!(change, (0.15 / 8.0 + 0.5 / 5.0) * 0.15);
        assert_eq!(
            calculator.get_potential_energy_change(BlockPos::new(1, 1, 1), 0.0),
            0.0
        );
    }
}
