//! `Bucketable` (fish, axolotls, tadpoles): saving a mob into a bucket and loading it back out of one.

use super::{Entity, EntityId, ServerPlayer};
use crate::server::level::ServerLevel;
use crate::world::item::{InteractionHand, InteractionResult, ItemStack, create_filled_result};
use mc_data::{Item, SoundEvent};
use mc_nbt::CompoundTag;
use mc_protocol::components::{BUCKET_ENTITY_DATA, CUSTOM_NAME, CustomData};

pub trait Bucketable {
    /// `fromBucket()`.
    #[expect(
        clippy::wrong_self_convention,
        reason = "mirrors the vanilla accessor `Bucketable.fromBucket`"
    )]
    fn from_bucket(&self) -> bool;
    /// `setFromBucket(fromBucket)`.
    fn set_from_bucket(&mut self, from_bucket: bool);
    /// `saveToBucketTag(bucket)`.
    fn save_to_bucket_tag(&self, bucket: &mut ItemStack);
    /// `loadFromBucketTag(tag)`.
    fn load_from_bucket_tag(&mut self, tag: &CompoundTag);
    /// `getBucketItemStack()`.
    fn get_bucket_item_stack(&self) -> ItemStack;
    /// `getPickupSound()`.
    fn get_pickup_sound(&self) -> SoundEvent;
    /// `canBePickedUpWithBucket(itemStack)`.
    fn can_be_picked_up_with_bucket(&self, stack: &ItemStack) -> bool {
        stack.item() == Item::WATER_BUCKET
    }
}

/// `Bucketable.saveDefaultDataToBucketTag(entity, bucket)`.
pub fn save_default_data_to_bucket_tag(entity: &dyn Entity, bucket: &mut ItemStack) {
    if let Some(name) = entity.base().get_custom_name() {
        bucket.set(CUSTOM_NAME, name);
    }
    let mut tag = bucket
        .get(&BUCKET_ENTITY_DATA)
        .map(|d| d.copy_tag())
        .unwrap_or_default();
    if entity.mob().is_some_and(|m| m.is_no_ai()) {
        tag.put_boolean("NoAI", true);
    }
    if entity.base().is_silent() {
        tag.put_boolean("Silent", true);
    }
    if entity.base().is_no_gravity() {
        tag.put_boolean("NoGravity", true);
    }
    if entity.base().has_glowing_tag {
        tag.put_boolean("Glowing", true);
    }
    if entity.is_permanently_invulnerable() {
        tag.put_boolean("Invulnerable", true);
    }
    if entity.is_persistence_required() {
        tag.put_boolean("PersistenceRequired", true);
    }
    tag.put_float("Health", entity.get_health());
    bucket.set(BUCKET_ENTITY_DATA, CustomData::of(&tag));
}

/// `Bucketable.loadDefaultDataFromBucketTag(entity, tag)`.
pub fn load_default_data_from_bucket_tag(entity: &mut dyn Entity, tag: &CompoundTag) {
    if let Some(v) = tag.get_boolean("NoAI")
        && let Some(mob) = entity.mob_mut()
    {
        mob.set_no_ai(v);
    }
    if let Some(v) = tag.get_boolean("Silent") {
        entity.base_mut().set_silent(v);
    }
    if let Some(v) = tag.get_boolean("NoGravity") {
        entity.base_mut().set_no_gravity(v);
    }
    if let Some(v) = tag.get_boolean("Glowing") {
        entity.base_mut().has_glowing_tag = v;
    }
    if let Some(v) = tag.get_boolean("Invulnerable") {
        entity.base_mut().permanently_invulnerable = v;
    }
    if tag.get_boolean("PersistenceRequired") == Some(true) {
        entity.set_persistence_required();
    }
    if let Some(v) = tag.get_float("Health")
        && let Some(living) = entity.living_mut()
    {
        living.set_health(v);
    }
}

/// `Bucketable.bucketMobPickup(player, hand, pickupEntity)` for the mob currently being interacted with (taken out of the
/// level while its `mobInteract` runs, so it is passed in): `None` when it cannot be picked up with the held stack.
pub fn bucket_mob_pickup_in_place(
    level: &mut ServerLevel,
    player: &mut ServerPlayer,
    hand: InteractionHand,
    entity: &mut dyn Entity,
) -> Option<InteractionResult> {
    let stack = player.get_item_in_hand(hand);
    let bucketable = entity.as_bucketable()?;
    if !(bucketable.can_be_picked_up_with_bucket(&stack) && entity.is_alive()) {
        return None;
    }
    let sound = bucketable.get_pickup_sound();
    entity.play_sound(level, sound, 1.0, 1.0);
    let mut bucket = entity.as_bucketable()?.get_bucket_item_stack();
    entity.as_bucketable()?.save_to_bucket_tag(&mut bucket);
    crate::world::entity::mob::leashable::drop_leash(entity, level);
    entity.discard();
    let result = create_filled_result(
        stack,
        &mut crate::world::entity::player::menu_ctx::PlayerCtx { player, level },
        bucket.clone(),
        false,
    );
    player.set_item_in_hand(hand, result);
    crate::advancements::triggers::CriteriaTriggers::FILLED_BUCKET.trigger(level, player, &bucket);
    Some(InteractionResult::SUCCESS)
}

/// `Bucketable.bucketMobPickup(player, hand, pickupEntity)`: `None` when the mob cannot be picked up with the held stack.
pub fn bucket_mob_pickup(
    level: &mut ServerLevel,
    player: &mut ServerPlayer,
    hand: InteractionHand,
    mob: EntityId,
) -> Option<InteractionResult> {
    let stack = player.get_item_in_hand(hand);
    let bucket = level.with_entity(mob, |level, entity| {
        let bucketable = entity.as_bucketable()?;
        if !(bucketable.can_be_picked_up_with_bucket(&stack) && entity.is_alive()) {
            return None;
        }
        let sound = bucketable.get_pickup_sound();
        entity.play_sound(level, sound, 1.0, 1.0);
        let mut bucket = entity.as_bucketable()?.get_bucket_item_stack();
        entity.as_bucketable()?.save_to_bucket_tag(&mut bucket);
        crate::world::entity::mob::leashable::drop_leash(entity, level);
        entity.discard();
        Some(bucket)
    })??;
    let result = create_filled_result(
        stack,
        &mut crate::world::entity::player::menu_ctx::PlayerCtx { player, level },
        bucket.clone(),
        false,
    );
    player.set_item_in_hand(hand, result);
    crate::advancements::triggers::CriteriaTriggers::FILLED_BUCKET.trigger(level, player, &bucket);
    Some(InteractionResult::SUCCESS)
}
