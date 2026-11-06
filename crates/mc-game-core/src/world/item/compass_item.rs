//! `CompassItem` and `LodestoneTracker.tick`: lodestone compasses.

use super::{InteractionResult, ItemBehaviour, ItemStack, ItemStackExt, PlainItem, UseOnContext};
use crate::register_item_class;
use crate::server::level::ServerLevel;
use crate::world::entity::equipment::EquipmentSlot;
use crate::world::entity::{Entity, ServerPlayer};
use crate::world::inventory::Prediction;
use crate::world::level::{BlockGetter, LevelReader};
use mc_core::pos::BlockPos;
use mc_data::block::Block;
use mc_data::{Item, SoundEvent};
use mc_protocol::codec::GlobalPos;
use mc_protocol::components::{LODESTONE_TRACKER, LodestoneTracker};
use mc_protocol::packets::play::level::SoundSource;
use mc_text::Component;

pub struct CompassItem;

register_item_class!("CompassItem", |_| Box::new(CompassItem));

/// `level.getPoiManager().existsAtPosition(PoiTypes.LODESTONE, pos)`: a lodestone POI exists exactly where a lodestone
/// block stands. An unloaded chunk counts as present (vanilla reads the POI storage, which this check cannot).
fn lodestone_poi_exists(level: &ServerLevel, pos: BlockPos) -> bool {
    !level.has_chunk(pos.x() >> 4, pos.z() >> 4)
        || level.get_block_state(pos).block() == Block::LODESTONE
}

/// `LodestoneTracker.tick(level)`: the tracker with a vanished lodestone cleared, `None` when it stays as it is.
pub fn tick_lodestone_tracker(
    tracker: &LodestoneTracker,
    level: &ServerLevel,
) -> Option<LodestoneTracker> {
    let target = tracker.target.as_ref().filter(|_| tracker.tracked)?;
    if target.dimension != level.dimension {
        return None;
    }
    let pos = target.pos;
    if level.is_in_world_bounds(pos) && lodestone_poi_exists(level, pos) {
        None
    } else {
        Some(LodestoneTracker {
            target: None,
            tracked: true,
        })
    }
}

impl ItemBehaviour for CompassItem {
    fn is_foil(&self, stack: &ItemStack) -> bool {
        stack.has(mc_data::DataComponentType::LODESTONE_TRACKER) || stack.is_enchanted()
    }

    fn inventory_tick(
        &self,
        stack: &mut ItemStack,
        level: &mut ServerLevel,
        _owner: &mut dyn Entity,
        _slot: Option<EquipmentSlot>,
    ) {
        if let Some(new_tracker) = stack
            .get(&LODESTONE_TRACKER)
            .and_then(|tracker| tick_lodestone_tracker(tracker, level))
        {
            stack.set(LODESTONE_TRACKER, new_tracker);
        }
    }

    fn use_on(
        &self,
        context: &mut UseOnContext,
        level: &mut ServerLevel,
        player: &mut ServerPlayer,
    ) -> InteractionResult {
        let pos = context.get_clicked_pos();
        if level.get_block_state(pos).block() != Block::LODESTONE {
            return PlainItem.use_on(context, level, player);
        }
        level.play_sound(
            None,
            pos,
            SoundEvent::ITEM_LODESTONE_COMPASS_LOCK,
            SoundSource::Players,
            1.0,
            1.0,
        );
        let infinite = player.player.abilities.instabuild;
        let replace_existing_stack = !infinite && context.item_stack.count() == 1;
        let target = LodestoneTracker {
            target: Some(GlobalPos {
                dimension: level.dimension.clone(),
                pos,
            }),
            tracked: true,
        };
        if replace_existing_stack {
            context.item_stack.set(LODESTONE_TRACKER, target);
        } else {
            let mut lodestone_compass =
                ItemStack::with_patch(Item::COMPASS, 1, &context.item_stack.get_components_patch());
            context.item_stack.consume(1, infinite);
            lodestone_compass.set(LODESTONE_TRACKER, target);
            let mut to_add = lodestone_compass.clone();
            if !player.player.inventory.add(&mut to_add, infinite) {
                player.drop(level, lodestone_compass, false, Prediction::Predicted);
            }
        }
        InteractionResult::SUCCESS
    }

    fn get_name(&self, stack: &ItemStack) -> Component {
        if stack.has(mc_data::DataComponentType::LODESTONE_TRACKER) {
            Component::translatable("item.minecraft.lodestone_compass", Vec::new())
        } else {
            PlainItem.get_name(stack)
        }
    }
}
