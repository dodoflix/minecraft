//! `GoToClosestVillage`.

use super::{BehaviorBuilder, OneShot};
use crate::world::entity::ai::memory::WalkTarget;
use crate::world::entity::ai::memory::memory_module_type::WALK_TARGET;
use crate::world::entity::ai::util::land_random_pos;
use mc_core::pos::{BlockPos, SectionPos};

/// `create(speedModifier, closeEnoughDistance)`.
pub fn create(speed_modifier: f32, close_enough_distance: i32) -> OneShot {
    let mut i = BehaviorBuilder::instance();
    let walk_target = i.absent(&WALK_TARGET);
    i.create(move |c| {
        if c.level.is_village(c.body().block_position()) {
            return false;
        }
        let sections_to_village = c.level.poi_manager().sections_to_village(SectionPos::of_block_pos(&c.body().block_position()));
        let mut target_pos = None;
        for _ in 0..5 {
            let land_pos = land_random_pos::get_pos_weighted(c.body, c.level, 15, 7, &mut |_, level, p| -f64::from(level.poi_manager().sections_to_village(SectionPos::of_block_pos(&p))));
            if let Some(land_pos) = land_pos {
                let land_pos_sections = c.level.poi_manager().sections_to_village(SectionPos::of_block_pos(&BlockPos::containing(land_pos.x, land_pos.y, land_pos.z)));
                if land_pos_sections < sections_to_village {
                    target_pos = Some(land_pos);
                    break;
                }
                if land_pos_sections == sections_to_village {
                    target_pos = Some(land_pos);
                }
            }
        }
        if let Some(target_pos) = target_pos {
            c.set(&walk_target, WalkTarget::from_vec3(target_pos, speed_modifier, close_enough_distance));
        }
        true
    })
}
