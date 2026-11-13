//! `EndCrystalItem`: places an `EndCrystal` on obsidian or bedrock.

use super::{InteractionResult, ItemBehaviour, UseOnContext};
use crate::register_item_class;
use crate::server::level::ServerLevel;
use crate::world::entity::ServerPlayer;
use crate::world::entity::boss::end_crystal::EndCrystal;
use crate::world::level::BlockGetter;
use crate::world::level::level_access::LevelReader;
use mc_core::phys::{AABB, Vec3};
use mc_data::GameEvent;
use mc_data::block::Block;

struct EndCrystalItem;

impl ItemBehaviour for EndCrystalItem {
    /// `useOn(context)`: obsidian or bedrock with a free 1x2x1 space above takes the crystal (without its bottom slab).
    fn use_on(
        &self,
        context: &mut UseOnContext,
        level: &mut ServerLevel,
        player: &mut ServerPlayer,
    ) -> InteractionResult {
        let pos = context.get_clicked_pos();
        let block = level.get_block_state(pos).block();
        if block != Block::OBSIDIAN && block != Block::BEDROCK {
            return InteractionResult::FAIL;
        }
        let above = pos.above();
        if !level.is_empty_block(above) {
            return InteractionResult::FAIL;
        }
        let (x, y, z) = (
            f64::from(above.x()),
            f64::from(above.y()),
            f64::from(above.z()),
        );
        let aabb = AABB::new(x, y, z, x + 1.0, y + 2.0, z + 1.0);
        // `level.getEntities(null, aabb)` includes the player, who is out of the level storage while the item is used.
        if !level.get_players_in_aabb(&aabb).is_empty()
            || !level
                .get_entities(None, &aabb, |e| !e.is_spectator())
                .is_empty()
        {
            return InteractionResult::FAIL;
        }
        let mut crystal = EndCrystal::at(level, x + 0.5, y, z + 0.5);
        crystal.set_show_bottom(false);
        level.add_fresh_entity(Box::new(crystal));
        level.game_event_from_entity_ref(
            &*player,
            GameEvent::ENTITY_PLACE,
            Vec3::at_center_of(&above),
        );
        // Not implemented: `EnderDragonFight.tryRespawn()` of the level's dragon fight (the fight is not ported).
        context.item_stack.shrink(1);
        InteractionResult::SUCCESS
    }
}

register_item_class!("EndCrystalItem", |_| Box::new(EndCrystalItem));
