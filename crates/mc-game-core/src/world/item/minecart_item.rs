//! `MinecartItem`: places the minecart of its type on a rail.

use super::{InteractionResult, ItemBehaviour, UseOnContext};
use crate::register_item_class;
use crate::server::level::ServerLevel;
use crate::world::entity::ServerPlayer;
use crate::world::entity::vehicle::minecart::abstract_minecart::{create_minecart, rail_shape};
use crate::world::level::BlockGetter;
use crate::world::level::block::impls::base_rail_block::is_slope;
use crate::world::level::block::state::BlockStateExt;
use mc_core::phys::Vec3;
use mc_data::EntityType;
use mc_data::GameEvent;
use mc_data::enums::EntitySpawnReason;

struct MinecartItem {
    entity_type: EntityType,
}

impl ItemBehaviour for MinecartItem {
    /// `useOn(context)`: any rail takes the cart, a slope half a block higher.
    // Not implemented: the `NewMinecartBehavior` overlap check (another cart in the new cart's box refuses the placement).
    fn use_on(
        &self,
        context: &mut UseOnContext,
        level: &mut ServerLevel,
        player: &mut ServerPlayer,
    ) -> InteractionResult {
        let pos = context.get_clicked_pos();
        let state = level.get_block_state(pos);
        if !state.is_tag("rails") {
            return InteractionResult::FAIL;
        }
        let offset = if is_slope(rail_shape(state)) {
            0.5
        } else {
            0.0
        };
        let Some(cart) = create_minecart(
            level,
            f64::from(pos.x()) + 0.5,
            f64::from(pos.y()) + 0.0625 + offset,
            f64::from(pos.z()) + 0.5,
            self.entity_type,
            EntitySpawnReason::Dispenser,
            &context.item_stack,
            Some(player),
        ) else {
            return InteractionResult::FAIL;
        };
        level.add_fresh_entity(cart);
        let below = level.get_block_state(pos.below());
        level.game_event_from_entity_ref_in_state(
            &*player,
            Some(below),
            GameEvent::ENTITY_PLACE,
            Vec3::at_center_of(&pos),
        );
        context.item_stack.shrink(1);
        InteractionResult::SUCCESS
    }
}

register_item_class!("MinecartItem", |item| Box::new(MinecartItem {
    entity_type: EntityType::by_name(item.data().name)
        .expect("minecart entity type named like its item"),
}));
