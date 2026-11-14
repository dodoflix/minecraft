//! `net.minecraft.world.item.BoatItem`: places a boat / chest boat / raft on the block or water the player looks at.

use super::abstract_boat::{AbstractBoat, boat_spec};
use crate::register_item_class;
use crate::server::level::ServerLevel;
use crate::world::entity::factory::{self, EntitySpawnRequest};
use crate::world::entity::spawn_config::apply_default_stack_config;
use crate::world::entity::vehicle::no_collision;
use crate::world::entity::{Entity, ServerPlayer};
use crate::world::item::player_pov_hit_result_fluid;
use crate::world::item::{
    InteractionHand, InteractionResult, ItemBehaviour, ItemStackExt, PlainItem,
};
use crate::world::level::collision::ClipFluid;
use mc_data::enums::EntitySpawnReason;
use mc_data::{EntityType, GameEvent, Item};
use mc_protocol::packets::play::Stat;

/// Every boat entity type (the `EntityType` a `BoatItem` places is the one whose drop item it is).
const BOAT_TYPES: [EntityType; 22] = [
    EntityType::OAK_BOAT,
    EntityType::OAK_CHEST_BOAT,
    EntityType::SPRUCE_BOAT,
    EntityType::SPRUCE_CHEST_BOAT,
    EntityType::BIRCH_BOAT,
    EntityType::BIRCH_CHEST_BOAT,
    EntityType::JUNGLE_BOAT,
    EntityType::JUNGLE_CHEST_BOAT,
    EntityType::ACACIA_BOAT,
    EntityType::ACACIA_CHEST_BOAT,
    EntityType::CHERRY_BOAT,
    EntityType::CHERRY_CHEST_BOAT,
    EntityType::DARK_OAK_BOAT,
    EntityType::DARK_OAK_CHEST_BOAT,
    EntityType::PALE_OAK_BOAT,
    EntityType::PALE_OAK_CHEST_BOAT,
    EntityType::MANGROVE_BOAT,
    EntityType::MANGROVE_CHEST_BOAT,
    EntityType::POPLAR_BOAT,
    EntityType::POPLAR_CHEST_BOAT,
    EntityType::BAMBOO_RAFT,
    EntityType::BAMBOO_CHEST_RAFT,
];

struct BoatItem {
    entity_type: EntityType,
    item: Item,
}

impl BoatItem {
    /// `getBoat(level, hitResult, itemStack, player)`.
    fn get_boat(
        &self,
        level: &mut ServerLevel,
        location: mc_core::phys::Vec3,
        stack: &crate::world::item::ItemStack,
        player: &ServerPlayer,
    ) -> Option<Box<dyn Entity>> {
        let mut boat = factory::create(
            self.entity_type,
            level,
            EntitySpawnRequest {
                reason: EntitySpawnReason::SpawnItemUse,
                ignore_checks: false,
            },
        )?;
        if let Some(b) = boat.as_any_mut().downcast_mut::<AbstractBoat>() {
            b.set_initial_pos(location.x, location.y, location.z);
        }
        apply_default_stack_config(level, boat.as_mut(), stack, Some(player));
        Some(boat)
    }
}

impl ItemBehaviour for BoatItem {
    fn use_item(
        &self,
        level: &mut ServerLevel,
        player: &mut ServerPlayer,
        hand: InteractionHand,
    ) -> InteractionResult {
        let mut stack = player.get_item_in_hand(hand);
        let hit = player_pov_hit_result_fluid(level, player, ClipFluid::Any);
        if hit.miss {
            return InteractionResult::PASS;
        }
        let view = player.get_view_vector(1.0);
        let area = player
            .get_bounding_box()
            .expand_towards_vec3(view.scale(5.0))
            .inflate_scalar(1.0);
        let entities = level.get_entities(Some(player.id()), &area, |e| e.is_pickable());
        if !entities.is_empty() {
            let from = player.base().get_eye_position();
            for id in entities {
                if let Some(e) = level.get_entity_or_part(id)
                    && e.get_bounding_box()
                        .inflate_scalar(f64::from(e.get_pick_radius()))
                        .contains_vec3(from)
                {
                    return InteractionResult::PASS;
                }
            }
        }
        let Some(mut boat) = self.get_boat(level, hit.location, &stack, player) else {
            return InteractionResult::FAIL;
        };
        boat.set_y_rot(player.base().y_rot());
        if !no_collision(level, boat.as_dyn(), &boat.get_bounding_box()) {
            return InteractionResult::FAIL;
        }
        level.add_fresh_entity(boat);
        level.game_event_from_entity_ref(&*player, GameEvent::ENTITY_PLACE, hit.location);
        stack.consume(1, player.player.abilities.instabuild);
        player.set_item_in_hand_slot(level, hand, stack);
        player.award_stat_once(level, Stat::Used(self.item));
        InteractionResult::SUCCESS
    }
}

register_item_class!("BoatItem", |item| {
    let placed = BOAT_TYPES
        .iter()
        .find(|ty| boat_spec(**ty).is_some_and(|(drop, _, _)| drop == item));
    match placed {
        Some(&entity_type) => Box::new(BoatItem { entity_type, item }),
        // A boat item without a boat type behaves like a plain item instead of failing the table build.
        None => Box::new(PlainItem),
    }
});
