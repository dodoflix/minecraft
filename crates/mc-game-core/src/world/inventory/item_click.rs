//! `AbstractContainerMenu.tryItemClickBehaviourOverride`: lets the carried stack / the clicked stack's item take over a
//! pickup click (bundles), unless its item is disabled by the level's feature flags. A level-less test player has no
//! flags to check: every item counts as enabled.

use super::{ClickAction, MenuBase, MenuPlayer};
use crate::world::item::{ItemStack, behaviour, is_item_enabled};
use crate::world::level::level_access::LevelReader;

pub(super) fn try_item_click_behaviour_override(
    base: &mut MenuBase,
    player: &mut dyn MenuPlayer,
    action: ClickAction,
    slot_idx: usize,
    clicked: &ItemStack,
    carried: &mut ItemStack,
) -> bool {
    let features = player.level_ref().map(LevelReader::enabled_features);
    let enabled = |stack: &ItemStack| features.is_none_or(|f| is_item_enabled(stack, f));
    let carried_behaviour = behaviour(carried.item());
    let overridden = enabled(carried)
        && base.with_slot(player, slot_idx, |slot, env| {
            carried_behaviour.override_stacked_on_other(carried, slot, env, action)
        });
    if overridden {
        return true;
    }
    let clicked_behaviour = behaviour(clicked.item());
    enabled(clicked)
        && base.with_slot(player, slot_idx, |slot, env| {
            // `self` is the stack stored in the slot: mutate a copy and store it back (in-place in Java).
            let mut me = clicked.clone();
            let result =
                clicked_behaviour.override_other_stacked_on_me(&mut me, carried, slot, env, action);
            if me != *clicked {
                slot.with_item_mut(env, |t| *t = me);
            }
            result
        })
}
