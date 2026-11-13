//! `MovementPredicate`.

use super::entity_predicate::EntitySubPredicateImpl;
use crate::server::level::ServerLevel;
use crate::world::entity::Entity;
use mc_core::math::mth;
use mc_core::phys::Vec3;
use mc_dfu::Codec;
use mc_protocol::components::Doubles;
use std::sync::LazyLock;

#[derive(Clone, Debug, PartialEq)]
pub struct MovementPredicate {
    pub x: Doubles,
    pub y: Doubles,
    pub z: Doubles,
    pub speed: Doubles,
    pub horizontal_speed: Doubles,
    pub vertical_speed: Doubles,
    pub fall_distance: Doubles,
}

impl MovementPredicate {
    fn only(f: impl FnOnce(&mut MovementPredicate)) -> MovementPredicate {
        let any = Doubles::any;
        let mut p = MovementPredicate {
            x: any(),
            y: any(),
            z: any(),
            speed: any(),
            horizontal_speed: any(),
            vertical_speed: any(),
            fall_distance: any(),
        };
        f(&mut p);
        p
    }

    pub fn speed(bounds: Doubles) -> MovementPredicate {
        Self::only(|p| p.speed = bounds)
    }

    pub fn horizontal_speed(bounds: Doubles) -> MovementPredicate {
        Self::only(|p| p.horizontal_speed = bounds)
    }

    pub fn vertical_speed(bounds: Doubles) -> MovementPredicate {
        Self::only(|p| p.vertical_speed = bounds)
    }

    pub fn fall_distance(bounds: Doubles) -> MovementPredicate {
        Self::only(|p| p.fall_distance = bounds)
    }

    /// `matches(x, y, z, fallDistance)`.
    pub fn matches_values(&self, x: f64, y: f64, z: f64, fall_distance: f64) -> bool {
        self.x.matches(x)
            && self.y.matches(y)
            && self.z.matches(z)
            && self.speed.matches_sqr(mth::length_squared_xyz_f64(x, y, z))
            && self
                .horizontal_speed
                .matches_sqr(mth::length_squared_f64(x, z))
            && self.vertical_speed.matches(y.abs())
            && self.fall_distance.matches(fall_distance)
    }
}

impl EntitySubPredicateImpl for MovementPredicate {
    fn codec() -> Codec<Self> {
        static C: LazyLock<Codec<MovementPredicate>> = LazyLock::new(|| {
            let any = || Doubles::any();
            mc_dfu::record!(MovementPredicate {
                x: Doubles::codec().optional_field_of_default("x", any()),
                y: Doubles::codec().optional_field_of_default("y", any()),
                z: Doubles::codec().optional_field_of_default("z", any()),
                speed: Doubles::codec().optional_field_of_default("speed", any()),
                horizontal_speed: Doubles::codec()
                    .optional_field_of_default("horizontal_speed", any()),
                vertical_speed: Doubles::codec().optional_field_of_default("vertical_speed", any()),
                fall_distance: Doubles::codec().optional_field_of_default("fall_distance", any()),
            })
            .codec()
        });
        C.clone()
    }

    fn matches<'a>(
        &self,
        entity: &'a dyn Entity,
        level: &'a ServerLevel,
        _position: Option<Vec3>,
        _lent: &[&'a dyn Entity],
    ) -> bool {
        let m = entity.get_known_movement(level);
        let velocity = Vec3::new(m.x * 20.0, m.y * 20.0, m.z * 20.0);
        self.matches_values(
            velocity.x,
            velocity.y,
            velocity.z,
            entity.base().fall_distance,
        )
    }
}
