//! `ItemStack`-level item operations: the wrappers around `ItemBehaviour` that vanilla's `ItemStack` methods add
//! (`useOn`, `use`, `finishUsingItem`, `applyAfterUseComponentSideEffects`, `hurtAndBreak`, `inventoryTick`,
//! `onUseTick`, `mineBlock`, adventure-mode predicates, `isItemEnabled`).
//!
//! `ItemStack` here is a value; vanilla's methods mutate the very object stored in the player's hand. The `&mut
//! ItemStack` parameters below are the caller's *copy* of that stack and the caller writes it back (see
//! `world::item::context` for the same convention).

use super::{InteractionHand, InteractionResult, ItemStack, ItemStackExt, UseOnContext, behaviour};
use crate::server::level::ServerLevel;
use crate::world::entity::equipment::EquipmentSlot;
use crate::world::entity::{Entity, ServerPlayer};
use crate::world::level::BlockGetter;
use mc_core::pos::BlockPos;
use mc_data::Item;
use mc_data::block::BlockState;
use mc_nbt::CompoundTag;
use mc_protocol::components::{
    AdventureBlockEntities, AdventureModePredicate, CAN_BREAK, CAN_PLACE_ON, TOOL, USE_COOLDOWN,
    USE_REMAINDER, WEAPON,
};
use mc_world::level_storage::FeatureFlagSet;

/// `Item.isEnabled(enabledFeatures)` (`requiredFeatures.isSubsetOf(enabledFeatures)`).
pub fn item_is_enabled(item: Item, enabled_features: FeatureFlagSet) -> bool {
    let required: Vec<mc_core::ident::Identifier> = item
        .data()
        .required_features
        .iter()
        .filter_map(|n| mc_core::ident::Identifier::try_parse(n))
        .collect();
    FeatureFlagSet::from_names(required.iter(), |_| {}).is_subset_of(enabled_features)
}

/// `ItemStack.isItemEnabled(enabledFeatures)`.
pub fn is_item_enabled(stack: &ItemStack, enabled_features: FeatureFlagSet) -> bool {
    stack.is_empty() || item_is_enabled(stack.item(), enabled_features)
}

/// `ItemStack.applyAfterUseComponentSideEffects(user, stackBeforeUsing)`: `use_remainder` conversion and
/// `use_cooldown`. Returns the (possibly replaced) stack.
pub fn apply_after_use_component_side_effects(
    level: &mut ServerLevel,
    stack: ItemStack,
    user: &mut dyn Entity,
    before: &ItemStack,
) -> ItemStack {
    let count_before = before.count();
    let infinite = user.player().is_some_and(|p| p.abilities.instabuild);
    let mut result = stack;
    if let Some(remainder) = before.get(&USE_REMAINDER) {
        // `UseRemainder.convertIntoRemainder(usedStack, countBefore, infinite, handleExtraItemsCreatedOnUse)`
        if !infinite && result.count() < count_before {
            let remainder_stack = remainder.convert_into.create();
            if result.is_empty() {
                result = remainder_stack;
            } else if let Some(player) = user.as_any_mut().downcast_mut::<ServerPlayer>() {
                // `Player.handleExtraItemsCreatedOnUse`: add to the inventory or drop
                let mut extra = remainder_stack;
                let inf = player.player.abilities.instabuild;
                if !player.player.inventory.add(&mut extra, inf) {
                    player.drop(
                        level,
                        extra,
                        false,
                        crate::world::inventory::Prediction::Predicted,
                    );
                }
            }
        }
    }
    if let Some(cooldown) = before.get(&USE_COOLDOWN) {
        // `UseCooldown.apply(stack, user)`: players only
        if let Some(p) = user.as_any_mut().downcast_mut::<ServerPlayer>() {
            p.player
                .cooldowns
                .add_cooldown(before, (cooldown.seconds * 20.0f32) as i32);
            p.flush_item_cooldown_packets();
        }
    }
    result
}

/// `ItemStack.canPlaceOnBlockInAdventureMode` / `canBreakBlockInAdventureMode`.
pub fn can_place_on_block_in_adventure_mode(
    stack: &ItemStack,
    level: &ServerLevel,
    pos: BlockPos,
) -> bool {
    stack
        .get(&CAN_PLACE_ON)
        .is_some_and(|p| adventure_predicate_test(p, level, pos))
}

pub fn can_break_block_in_adventure_mode(
    stack: &ItemStack,
    level: &ServerLevel,
    pos: BlockPos,
) -> bool {
    stack
        .get(&CAN_BREAK)
        .is_some_and(|p| adventure_predicate_test(p, level, pos))
}

/// `AdventureModePredicate.test(BlockInWorld)` for the block at `pos` (the world is read when an `nbt` predicate asks).
pub fn adventure_predicate_test(
    predicate: &AdventureModePredicate,
    level: &ServerLevel,
    pos: BlockPos,
) -> bool {
    predicate.test(pos, level.get_block_state(pos), &LevelBlockEntities(level))
}

/// `BlockInWorld.getEntity()` over a level.
struct LevelBlockEntities<'a>(&'a ServerLevel);

impl AdventureBlockEntities for LevelBlockEntities<'_> {
    fn full_metadata(&self, pos: BlockPos) -> Option<CompoundTag> {
        self.0
            .get_block_entity(pos)
            .map(|e| e.save_with_full_metadata())
    }

    /// `BlockEntity.saveWithId`: the saved data and the id, without the position.
    fn with_id(&self, pos: BlockPos) -> Option<CompoundTag> {
        self.0.get_block_entity(pos).map(|e| {
            let mut tag = CompoundTag::new();
            e.save_without_metadata(&mut tag);
            tag.put_string("id", e.get_type().name());
            tag
        })
    }
}

/// `ItemStack.useOn(context)` for a player user. `context.item_stack` is the held stack (the caller stores it back).
pub fn item_stack_use_on(
    context: &mut UseOnContext,
    level: &mut ServerLevel,
    player: &mut ServerPlayer,
) -> InteractionResult {
    let pos = context.get_clicked_pos();
    if !player.player.abilities.may_build
        && !can_place_on_block_in_adventure_mode(&context.item_stack, level, pos)
    {
        return InteractionResult::PASS;
    }
    let before = context.item_stack.copy();
    let used_item = context.item_stack.item();
    let result = behaviour(used_item).use_on(context, level, player);
    if let InteractionResult::Success(success) = &result
        && success.was_item_interaction()
    {
        player.award_stat_once(level, crate::stats::Stat::Used(used_item));
        let transform_to = success
            .item_context
            .held_item_transformed_to
            .clone()
            .unwrap_or_else(|| context.item_stack.clone());
        let result_stack =
            apply_after_use_component_side_effects(level, transform_to, player, &before);
        return InteractionResult::Success(success.clone().held_item_transformed_to(result_stack));
    }
    result
}

/// `ItemStack.use(level, player, hand)`.
pub fn item_stack_use(
    level: &mut ServerLevel,
    player: &mut ServerPlayer,
    hand: InteractionHand,
) -> InteractionResult {
    let stack = player.get_item_in_hand(hand);
    let before = stack.copy();
    let is_instantly_used = behaviour(stack.item()).get_use_duration(&stack) <= 0;
    let result = behaviour(stack.item()).use_item(level, player, hand);
    if is_instantly_used && let InteractionResult::Success(success) = &result {
        let transformed = success.item_context.held_item_transformed_to.is_some();
        let transform_to = success
            .item_context
            .held_item_transformed_to
            .clone()
            .unwrap_or_else(|| player.get_item_in_hand(hand));
        let was_empty = transform_to.is_empty();
        let result_stack =
            apply_after_use_component_side_effects(level, transform_to, player, &before);
        // vanilla returns the very same stack object unless the use remainder replaced it: callers compare by identity
        if !transformed && !(was_empty && !result_stack.is_empty()) {
            return result;
        }
        return InteractionResult::Success(success.clone().held_item_transformed_to(result_stack));
    }
    result
}

/// `ItemStack.finishUsingItem(level, entity)`.
pub fn finish_using_item(
    stack: &mut ItemStack,
    level: &mut ServerLevel,
    entity: &mut dyn Entity,
) -> ItemStack {
    let before = stack.copy();
    let result = behaviour(stack.item()).finish_using_item(stack, level, entity);
    apply_after_use_component_side_effects(level, result, entity, &before)
}

/// `ItemStack.onUseTick(level, entity, ticksRemaining)`.
pub fn on_use_tick(
    stack: &mut ItemStack,
    level: &mut ServerLevel,
    entity: &mut dyn Entity,
    ticks_remaining: i32,
) {
    if let Some(consumable) = stack.get(&mc_protocol::components::CONSUMABLE).cloned() {
        use super::ConsumableExt;
        if consumable.should_emit_particles_and_sounds(ticks_remaining) {
            super::consumable::emit_particles_and_sounds(&consumable, level, entity, stack, 5);
        }
    }
    // `KineticWeapon.damageEntities` (spear lunge) belongs to the combat code.
    behaviour(stack.item()).on_use_tick(level, entity, stack, ticks_remaining);
}

/// `ItemStack.mineBlock(level, state, pos, owner)`.
pub fn mine_block(
    stack: &mut ItemStack,
    level: &mut ServerLevel,
    state: BlockState,
    pos: BlockPos,
    owner: &mut ServerPlayer,
) {
    let used_item = stack.item();
    if behaviour(used_item).mine_block(stack, level, state, pos, owner) {
        owner.award_stat_once(level, crate::stats::Stat::Used(used_item));
    }
}

/// `ItemStack.hurtEnemy(mob, attacker)`: whether the stack is a weapon.
pub fn hurt_enemy(
    level: &mut ServerLevel,
    stack: &mut ItemStack,
    mob: &mut dyn Entity,
    attacker: &mut dyn Entity,
) -> bool {
    behaviour(stack.item()).hurt_enemy(level, stack, mob, attacker);
    stack.has(mc_data::DataComponentType::WEAPON)
}

/// `ItemStack.postHurtEnemy(mob, attacker)`.
pub fn post_hurt_enemy(
    stack: &mut ItemStack,
    level: &mut ServerLevel,
    mob: &mut dyn Entity,
    attacker: &mut ServerPlayer,
) {
    behaviour(stack.item()).post_hurt_enemy(level, stack, mob, attacker);
    if let Some(weapon) = stack.get(&WEAPON) {
        let amount = weapon.item_damage_per_attack;
        hurt_and_break(stack, amount, level, attacker, EquipmentSlot::MainHand);
    }
}

/// `ItemStack.processDurabilityChange(amount, level, player)`; `player_infinite` = `player != null && hasInfiniteMaterials()`.
fn process_durability_change(
    stack: &ItemStack,
    amount: i32,
    level: &mut ServerLevel,
    player_infinite: bool,
) -> i32 {
    if !stack.is_damageable_item() || player_infinite {
        0
    } else if amount > 0 {
        super::enchantment_helper::process_durability_change(level, stack, amount)
    } else {
        amount
    }
}

/// `ItemStack.applyDamage(newDamage, player, onBreak)`: returns the broken copy when the stack broke.
fn apply_damage(
    stack: &mut ItemStack,
    new_damage: i32,
    level: &mut ServerLevel,
    player: Option<&mut ServerPlayer>,
) -> Option<ItemStack> {
    if let Some(player) = player {
        crate::advancements::triggers::CriteriaTriggers::ITEM_DURABILITY_CHANGED
            .trigger(level, player, stack, new_damage);
    }
    stack.set_damage_value(new_damage);
    if stack.is_broken() {
        let broken = stack.copy();
        stack.shrink(1);
        Some(broken)
    } else {
        None
    }
}

/// `ItemStack.hurtAndBreak(amount, level, player, onBreak)`: the broken copy is returned for the caller's `onBreak`.
/// `player` is the owner when it is a `ServerPlayer` (`null` otherwise).
pub fn hurt_and_break_stack(
    stack: &mut ItemStack,
    amount: i32,
    level: &mut ServerLevel,
    player: Option<&mut ServerPlayer>,
) -> Option<ItemStack> {
    let player_infinite = player
        .as_ref()
        .is_some_and(|p| p.player.abilities.instabuild);
    let new_amount = process_durability_change(stack, amount, level, player_infinite);
    if new_amount != 0 {
        let new_damage = stack.get_damage_value() + new_amount;
        apply_damage(stack, new_damage, level, player)
    } else {
        None
    }
}

/// `ItemStack.hurtAndBreak(amount, owner, slot)` for a player owner (`onEquippedItemBroken` on break).
pub fn hurt_and_break(
    stack: &mut ItemStack,
    amount: i32,
    level: &mut ServerLevel,
    owner: &mut ServerPlayer,
    slot: EquipmentSlot,
) {
    if let Some(broken) = hurt_and_break_stack(stack, amount, level, Some(&mut *owner)) {
        owner.on_equipped_item_broken(level, &broken, slot);
    }
}

/// `ItemStack.hurtAndConvertOnBreak(amount, newItem, owner, slot)`: the stack itself, or (when it broke) a single `new_item`
/// that keeps the components (`transmuteCopyIgnoreEmpty`) with the damage reset.
pub fn hurt_and_convert_on_break(
    stack: &mut ItemStack,
    amount: i32,
    new_item: Item,
    level: &mut ServerLevel,
    owner: &mut ServerPlayer,
    slot: EquipmentSlot,
) -> ItemStack {
    let broken = hurt_and_break_stack(stack, amount, level, Some(&mut *owner));
    if let Some(broken) = &broken {
        owner.on_equipped_item_broken(level, broken, slot);
    }
    if !stack.is_empty() {
        return stack.clone();
    }
    // the emptied stack no longer reports its components: take them from the broken copy
    let patch = broken.as_ref().unwrap_or(stack).get_components_patch();
    let mut replacement = ItemStack::with_patch(new_item, 1, &patch);
    if replacement.is_damageable_item() {
        replacement.set_damage_value(0);
    }
    replacement
}

/// `ItemStack.hurtWithoutBreaking(amount, player)`.
pub fn hurt_without_breaking(
    stack: &mut ItemStack,
    amount: i32,
    level: &mut ServerLevel,
    player: &mut ServerPlayer,
) {
    let new_amount =
        process_durability_change(stack, amount, level, player.player.abilities.instabuild);
    if new_amount != 0 {
        let new_damage = (stack.get_damage_value() + new_amount).min(stack.get_max_damage() - 1);
        apply_damage(stack, new_damage, level, Some(player));
    }
}

/// `Item.canDestroyBlock` default (the `tool` component's `can_destroy_blocks_in_creative`).
pub fn tool_can_destroy_block(stack: &ItemStack, user_is_creative: bool) -> bool {
    match stack.get(&TOOL) {
        Some(t) if !t.can_destroy_blocks_in_creative => !user_is_creative,
        _ => true,
    }
}

/// `ItemStack.onDestroyed(itemEntity)`.
pub fn on_destroyed(stack: &mut ItemStack, level: &mut ServerLevel, pos: mc_core::phys::Vec3) {
    behaviour(stack.item()).on_destroyed(level, stack, pos);
}

/// `ItemStack.interactLivingEntity(player, target, hand)`.
pub fn interact_living_entity(
    level: &mut ServerLevel,
    stack: &mut ItemStack,
    player: &mut ServerPlayer,
    target: &mut dyn Entity,
    hand: InteractionHand,
) -> InteractionResult {
    if let Some(equippable) = stack.get(&mc_protocol::components::EQUIPPABLE).cloned()
        && equippable.equip_on_interact
    {
        let result = equip_on_target(level, &equippable, target, stack);
        if result != InteractionResult::PASS {
            return result;
        }
    }
    behaviour(stack.item()).interact_living_entity(level, stack, player, target, hand)
}

/// `Equippable.equipOnTarget(player, target, itemStack)`.
fn equip_on_target(
    level: &mut ServerLevel,
    equippable: &mc_protocol::components::Equippable,
    target: &mut dyn Entity,
    stack: &mut ItemStack,
) -> InteractionResult {
    let slot = EquipmentSlot::from_protocol(equippable.slot);
    if target.is_equippable_in_slot(stack, slot)
        && target.get_item_by_slot(slot).is_empty()
        && target.is_alive()
    {
        target.set_item_slot(level, slot, stack.split(1));
        if target.mob().is_some() {
            target.set_drop_chance(slot, 2.0);
        }
        InteractionResult::SUCCESS
    } else {
        InteractionResult::PASS
    }
}

/// `ItemStack.inventoryTick(level, owner, slot)` for a non-player owner (players: `ServerPlayer::tick_inventory`).
pub fn inventory_tick(
    stack: &mut ItemStack,
    level: &mut ServerLevel,
    owner: &mut dyn Entity,
    slot: Option<EquipmentSlot>,
) {
    behaviour(stack.item()).inventory_tick(stack, level, owner, slot);
}
