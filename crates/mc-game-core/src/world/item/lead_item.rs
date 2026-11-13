//! `LeadItem`: tying the leashed mobs of a player to a fence.

use super::{InteractionResult, ItemBehaviour, UseOnContext};
use crate::register_item_class;
use crate::server::level::ServerLevel;
use crate::world::entity::decoration::leash_knot::{create_knot, get_knot};
use crate::world::entity::mob::leashable::{self, LeashHolder};
use crate::world::entity::{Entity, ServerPlayer};
use crate::world::level::BlockGetter;
use crate::world::level::block::BlockStateExt;
use mc_core::phys::Vec3;
use mc_core::pos::BlockPos;
use mc_data::{GameEvent, SoundEvent};

struct LeadItem;

impl ItemBehaviour for LeadItem {
    /// `useOn(context)`: a fence takes the player's leashed mobs.
    fn use_on(
        &self,
        context: &mut UseOnContext,
        level: &mut ServerLevel,
        player: &mut ServerPlayer,
    ) -> InteractionResult {
        let pos = context.get_clicked_pos();
        if level.get_block_state(pos).is_tag("fences") {
            bind_player_mobs(player, level, pos)
        } else {
            InteractionResult::PASS
        }
    }
}

register_item_class!("LeadItem", |_| Box::new(LeadItem));

/// `LeadItem.bindPlayerMobs(player, level, pos)`: the mobs the player leads (within the scan cube around `pos`) are tied to
/// the knot of the fence at `pos`; `PASS` when none can be.
pub fn bind_player_mobs(
    player: &ServerPlayer,
    level: &mut ServerLevel,
    pos: BlockPos,
) -> InteractionResult {
    let player_id = player.id();
    let to_leash = leashable::leashable_in_area(level, Vec3::at_center_of(&pos), |l| {
        leashable::get_leash_holder(l) == Some(player_id)
    });
    if to_leash.is_empty() {
        return InteractionResult::PASS;
    }
    let existing_knot = get_knot(level, pos);
    let knot = existing_knot.unwrap_or_else(|| create_knot(level, pos));
    let holder = level.get_entity(knot).map(LeashHolder::of);
    let mut any_leashed = false;
    for id in to_leash {
        let can = level
            .get_entity(id)
            .zip(level.get_entity(knot))
            .is_some_and(|(mob, knot)| leashable::can_have_a_leash_attached_to(level, mob, knot));
        if can && let Some(holder) = holder {
            level.with_entity(id, |level, m| {
                leashable::set_leashed_to(m, level, holder, true)
            });
            any_leashed = true;
        }
    }
    if any_leashed {
        // `knot.playPlacementSound()`
        level.with_entity(knot, |level, k| {
            k.play_sound(level, SoundEvent::ITEM_LEAD_TIED, 1.0, 1.0)
        });
        level.game_event_from_entity(
            Some(player_id),
            GameEvent::BLOCK_ATTACH,
            Vec3::at_center_of(&pos),
        );
        InteractionResult::SUCCESS_SERVER
    } else {
        if existing_knot.is_none() {
            level.with_entity(knot, |_, k| k.discard());
        }
        InteractionResult::PASS
    }
}
