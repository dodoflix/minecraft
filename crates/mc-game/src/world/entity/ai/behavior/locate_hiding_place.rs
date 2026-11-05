//! `LocateHidingPlace`: pick a bed (or the remembered home) to hide at during a raid.

use super::{BehaviorBuilder, OneShot};
use crate::world::entity::ai::memory::WalkTarget;
use crate::world::entity::ai::memory::memory_module_type::{BREED_TARGET, HIDING_PLACE, HOME, INTERACTION_TARGET, LOOK_TARGET, PATH, WALK_TARGET};
use crate::world::entity::ai::village::poi::{Occupancy, PoiTypes};
use mc_protocol::codec::GlobalPos;

/// `create(radius, speedModifier, closeEnoughDist)`.
pub fn create(radius: i32, speed_modifier: f32, close_enough_dist: i32) -> OneShot {
    let mut i = BehaviorBuilder::instance();
    let walk_target = i.absent(&WALK_TARGET);
    let home = i.registered(&HOME);
    let hiding_place = i.registered(&HIDING_PLACE);
    let path = i.registered(&PATH);
    let look_target = i.registered(&LOOK_TARGET);
    let breed_target = i.registered(&BREED_TARGET);
    let interaction_target = i.registered(&INTERACTION_TARGET);
    i.create(move |c| {
        let body_pos = c.body().block_position();
        let body_position = c.body().position();
        let manager = c.level.poi_manager();
        let found = manager.find(&|p| p.is(PoiTypes::HOME), &|_| true, body_pos, close_enough_dist + 1, Occupancy::Any).filter(|p| p.closer_to_center_than(&body_position, f64::from(close_enough_dist)));
        let pos = match found {
            Some(p) => Some(p),
            None => {
                // `body.getRandom()`
                let random = &mut c.body.base_mut().random;
                c.level.poi_manager().get_random(&|p| p.is(PoiTypes::HOME), &|_| true, Occupancy::Any, body_pos, radius, random)
            }
        };
        let pos = pos.or_else(|| c.try_get(&home).map(|g| g.pos));
        if let Some(pos) = pos {
            c.erase(&path);
            c.erase(&look_target);
            c.erase(&breed_target);
            c.erase(&interaction_target);
            let dimension = c.level.dimension.clone();
            c.set(&hiding_place, GlobalPos { dimension, pos });
            if !pos.closer_to_center_than(&body_position, f64::from(close_enough_dist)) {
                c.set(&walk_target, WalkTarget::from_block_pos(pos, speed_modifier, close_enough_dist));
            }
        }
        true
    })
}
