//! `MobBucketItem.spawn`: the mob inside a bucket is created at the clicked position and loaded from `bucket_entity_data`.

use super::ItemStack;
use crate::server::level::ServerLevel;
use crate::world::entity::Entity;
use crate::world::entity::spawn_config::{apply_default_stack_config, create_at};
use mc_core::pos::BlockPos;
use mc_data::EntityType;
use mc_data::enums::EntitySpawnReason;
use mc_protocol::components::BUCKET_ENTITY_DATA;

/// `MobBucketItem.spawn(level, itemStack, spawnPos)`: `createDefaultStackConfig` runs without a user.
pub fn spawn_from_bucket(
    entity_type: EntityType,
    level: &mut ServerLevel,
    stack: &ItemStack,
    pos: BlockPos,
) {
    let stack_for_config = stack.clone();
    let Some(mut mob) = create_at(
        level,
        entity_type,
        Some(&mut |level: &mut ServerLevel, entity: &mut dyn Entity| {
            apply_default_stack_config(level, entity, &stack_for_config, None)
        }),
        pos,
        EntitySpawnReason::Bucket,
        true,
        false,
    ) else {
        return;
    };
    if mob.as_bucketable().is_some() {
        let tag = stack
            .get(&BUCKET_ENTITY_DATA)
            .map(|d| d.copy_tag())
            .unwrap_or_default();
        if let Some(b) = mob.as_bucketable_mut() {
            b.load_from_bucket_tag(&tag);
            b.set_from_bucket(true);
        }
    }
    let id = mob.id();
    level.add_fresh_entity(mob);
    level.with_entity(id, |level, mob| mob.play_ambient_sound(level));
}
