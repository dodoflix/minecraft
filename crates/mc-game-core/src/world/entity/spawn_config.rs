//! `EntityType.create(level, postSpawnConfig, pos, reason, tryMoveDown, movedUp)`, `EntityType.createDefaultStackConfig`
//! (`appendComponentsConfig` + `appendCustomEntityStackConfig`) and `EntityType.updateCustomEntityTag`.
//!
//! A `PostSpawnProcessor` is a closure over the entity; the default stack config is [`apply_default_stack_config`].

use super::factory::{self, EntitySpawnRequest};
use super::{Entity, ServerPlayer};
use crate::server::level::ServerLevel;
use crate::world::item::ItemStack;
use mc_core::math::mth;
use mc_core::phys::AABB;
use mc_core::pos::BlockPos;
use mc_data::EntityType;
use mc_data::enums::EntitySpawnReason;
use mc_protocol::components::{CUSTOM_DATA, CUSTOM_NAME, ENTITY_DATA};

/// `entity.applyComponentsFromItemStack(stack)` for the components every entity understands (`Entity.applyImplicitComponents`):
/// `custom_name` and `custom_data`. Entity classes with more implicit components call this from their override.
pub fn apply_base_implicit_components(entity: &mut dyn Entity, stack: &ItemStack) {
    if let Some(name) = stack.get(&CUSTOM_NAME) {
        entity.base_mut().set_custom_name(Some(name.clone()));
    }
    if let Some(data) = stack.get(&CUSTOM_DATA) {
        entity.base_mut().custom_data = data.tag().clone();
    }
}

/// `EntityType.updateCustomEntityTag(level, user, entity, entityData)`; `user_is_op` tells whether the spawning user is an
/// op player (`server.getPlayerList().isOp(player.nameAndId())`).
pub fn update_custom_entity_tag(
    entity: &mut dyn Entity,
    entity_data: &mc_protocol::components::TypedEntityData<EntityType>,
    user_is_op: bool,
) {
    if entity.get_type() != entity_data.ty {
        return;
    }
    if !only_op_can_set_nbt(entity.get_type()) || user_is_op {
        load_into(entity, &entity_data.tag);
    }
}

/// `TypedEntityData.loadInto(entity)`: the entity's own save data with the tag merged over it, loaded back (uuid kept).
pub fn load_into(entity: &mut dyn Entity, tag: &mc_nbt::CompoundTag) {
    let mut data = mc_nbt::CompoundTag::new();
    entity.save_without_id(&mut data, &|_| None);
    let uuid = entity.uuid();
    data.merge(tag);
    if let Err(message) = entity.load(&data) {
        log::warn!("Failed to load entity data: {message}");
    }
    entity.base_mut().uuid = uuid;
}

/// `EntityType.onlyOpCanSetNbt()`.
pub fn only_op_can_set_nbt(ty: EntityType) -> bool {
    matches!(
        ty,
        EntityType::FALLING_BLOCK
            | EntityType::COMMAND_BLOCK_MINECART
            | EntityType::SPAWNER_MINECART
    )
}

/// `EntityType.createDefaultStackConfig(level, stack, user)` applied to `entity`.
pub fn apply_default_stack_config(
    level: &ServerLevel,
    entity: &mut dyn Entity,
    stack: &ItemStack,
    user: Option<&ServerPlayer>,
) {
    entity.apply_implicit_components(stack);
    if let Some(entity_data) = stack.get(&ENTITY_DATA) {
        let user_is_op = user.is_some_and(|p| level.is_op_uuid(Entity::uuid(p)));
        update_custom_entity_tag(entity, entity_data, user_is_op);
    }
}

/// `EntityType.getYOffset(level, spawnPos, movedUp, entityBox)`.
fn get_y_offset(
    level: &ServerLevel,
    spawn_pos: BlockPos,
    moved_up: bool,
    entity_box: &AABB,
) -> f64 {
    use crate::world::level::collision::CollisionGetter;
    use mc_core::direction::Axis;
    use mc_core::phys::shapes::Shapes;
    let mut aabb = AABB::from_block_pos(&spawn_pos);
    if moved_up {
        aabb = aabb.expand_towards(0.0, -1.0, 0.0);
    }
    let mut shapes = super::movement::EntityCollisionQuery::entity_collisions(level, None, &aabb);
    shapes.extend(level.get_block_collisions(None, &aabb));
    1.0 + Shapes::collide(
        Axis::Y,
        entity_box,
        &shapes,
        if moved_up { -2.0 } else { -1.0 },
    )
}

/// `postSpawnConfig`: runs on the freshly created entity before it is added.
pub type SpawnConfig<'a> = &'a mut dyn FnMut(&mut ServerLevel, &mut dyn Entity);

/// `EntityType.create(level, postSpawnConfig, spawnPos, reason, tryMoveDown, movedUp)` for entities that are not mobs
/// (mobs additionally get `yHeadRot`/`yBodyRot` and `finalizeSpawn`, owned by the mob code through `Entity::finalize_spawn_on_create`).
pub fn create_at(
    level: &mut ServerLevel,
    ty: EntityType,
    config: Option<SpawnConfig<'_>>,
    spawn_pos: BlockPos,
    reason: EntitySpawnReason,
    try_move_down: bool,
    moved_up: bool,
) -> Option<Box<dyn Entity>> {
    let mut entity = factory::create(
        ty,
        level,
        EntitySpawnRequest {
            reason,
            ignore_checks: false,
        },
    )?;
    let y_off = if try_move_down {
        entity.set_pos(
            f64::from(spawn_pos.x()) + 0.5,
            f64::from(spawn_pos.y() + 1),
            f64::from(spawn_pos.z()) + 0.5,
        );
        get_y_offset(level, spawn_pos, moved_up, &entity.base().bounding_box())
    } else {
        0.0
    };
    let y_rot = mth::wrap_degrees_f32(
        crate::world::level::LevelAccessor::random(level).next_float() * 360.0,
    );
    entity.snap_to(
        f64::from(spawn_pos.x()) + 0.5,
        f64::from(spawn_pos.y()) + y_off,
        f64::from(spawn_pos.z()) + 0.5,
        y_rot,
        0.0,
    );
    entity.finalize_spawn_on_create(level, reason);
    if let Some(config) = config {
        config(level, entity.as_mut());
    }
    Some(entity)
}

/// `EntityType.spawn(level, postSpawnConfig, spawnPos, reason, tryMoveDown, movedUp)`: [`create_at`], then
/// `addFreshEntityWithPassengers` and, for a mob, `playAmbientSound()`. The spawned entity's id.
pub fn spawn_at(
    level: &mut ServerLevel,
    ty: EntityType,
    config: Option<SpawnConfig<'_>>,
    spawn_pos: BlockPos,
    reason: EntitySpawnReason,
    try_move_down: bool,
    moved_up: bool,
) -> Option<super::EntityId> {
    let entity = create_at(
        level,
        ty,
        config,
        spawn_pos,
        reason,
        try_move_down,
        moved_up,
    )?;
    let id = entity.id();
    level.add_fresh_entity_with_passengers(vec![entity]);
    level.with_entity(id, |level, entity| {
        if entity.is_mob() {
            entity.play_ambient_sound(level);
        }
    });
    Some(id)
}
