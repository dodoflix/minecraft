//! `net.minecraft.advancements.predicates.DistancePredicate`.

use mc_dfu::Codec;
use mc_protocol::components::Doubles;
use std::sync::LazyLock;

#[derive(Clone, Debug, PartialEq)]
pub struct DistancePredicate {
    pub x: Doubles,
    pub y: Doubles,
    pub z: Doubles,
    pub horizontal: Doubles,
    pub absolute: Doubles,
}

impl DistancePredicate {
    pub fn codec() -> Codec<DistancePredicate> {
        static C: LazyLock<Codec<DistancePredicate>> = LazyLock::new(|| {
            mc_dfu::record!(DistancePredicate {
                x: Doubles::codec().optional_field_of_default("x", Doubles::any()),
                y: Doubles::codec().optional_field_of_default("y", Doubles::any()),
                z: Doubles::codec().optional_field_of_default("z", Doubles::any()),
                horizontal: Doubles::codec()
                    .optional_field_of_default("horizontal", Doubles::any()),
                absolute: Doubles::codec().optional_field_of_default("absolute", Doubles::any()),
            })
            .codec()
        });
        C.clone()
    }

    fn only(f: impl FnOnce(&mut DistancePredicate)) -> DistancePredicate {
        let mut p = DistancePredicate {
            x: Doubles::any(),
            y: Doubles::any(),
            z: Doubles::any(),
            horizontal: Doubles::any(),
            absolute: Doubles::any(),
        };
        f(&mut p);
        p
    }

    pub fn horizontal(horizontal: Doubles) -> DistancePredicate {
        Self::only(|p| p.horizontal = horizontal)
    }

    pub fn vertical(y: Doubles) -> DistancePredicate {
        Self::only(|p| p.y = y)
    }

    pub fn absolute(absolute: Doubles) -> DistancePredicate {
        Self::only(|p| p.absolute = absolute)
    }

    /// `matches(x0, y0, z0, x1, y1, z1)`: the differences are narrowed to `float` first, exactly like vanilla.
    pub fn matches(&self, x0: f64, y0: f64, z0: f64, x1: f64, y1: f64, z1: f64) -> bool {
        let xd = (x0 - x1) as f32;
        let yd = (y0 - y1) as f32;
        let zd = (z0 - z1) as f32;
        if !self.x.matches(f64::from(xd.abs()))
            || !self.y.matches(f64::from(yd.abs()))
            || !self.z.matches(f64::from(zd.abs()))
        {
            false
        } else {
            self.horizontal.matches_sqr(f64::from(xd * xd + zd * zd))
                && self
                    .absolute
                    .matches_sqr(f64::from(xd * xd + yd * yd + zd * zd))
        }
    }
}
