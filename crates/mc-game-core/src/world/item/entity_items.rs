//! Items that place decoration entities: `ArmorStandItem`, `HangingEntityItem` / `ItemFrameItem` (paintings and item frames) and
//! `CushionItem`.

use super::{
    BlockPlaceContext, InteractionResult, ItemBehaviour, ItemStack, UseOnContext, may_use_item_at,
};
use crate::register_item_class;
use crate::server::level::ServerLevel;
use crate::world::entity::decoration::cushion::Cushion;
use crate::world::entity::decoration::hanging::Hanging;
use crate::world::entity::decoration::item_frame::ItemFrame;
use crate::world::entity::decoration::painting::Painting;
use crate::world::entity::dimensions::EntityDimensions;
use crate::world::entity::spawn_config::{apply_default_stack_config, create_at};
use crate::world::entity::{Entity, ServerPlayer};
use crate::world::level::BlockGetter;
use crate::world::level::block::BlockStateExt;
use crate::world::level::collision::CollisionGetter;
use mc_core::direction::Direction;
use mc_core::math::mth;
use mc_core::phys::{AABB, Vec3};
use mc_core::pos::BlockPos;
use mc_data::enums::EntitySpawnReason;
use mc_data::{EntityType, GameEvent, Item, SoundEvent};
use mc_protocol::packets::play::SoundSource;

/// `ArmorStandItem`.
struct ArmorStandItem;

impl ItemBehaviour for ArmorStandItem {
    fn use_on(
        &self,
        context: &mut UseOnContext,
        level: &mut ServerLevel,
        player: &mut ServerPlayer,
    ) -> InteractionResult {
        if context.get_clicked_face() == Direction::Down {
            return InteractionResult::FAIL;
        }
        let place_context =
            BlockPlaceContext::new(context.clone(), |pos| level.get_block_state(pos));
        let block_pos = place_context.get_clicked_pos();
        let pos = Vec3::at_bottom_center_of(&block_pos);
        let aabb = EntityDimensions::of_type(EntityType::ARMOR_STAND).make_bounding_box_vec(pos);
        let blocked_by_player =
            !player.is_spectator() && player.get_bounding_box().intersects_aabb(&aabb);
        if level.no_collision(None, None, &aabb)
            && !blocked_by_player
            && level
                .get_entities(None, &aabb, |e| !e.is_spectator())
                .is_empty()
        {
            let stack = context.item_stack.clone();
            let Some(mut entity) = create_at(
                level,
                EntityType::ARMOR_STAND,
                Some(&mut |level: &mut ServerLevel, entity: &mut dyn Entity| {
                    apply_default_stack_config(level, entity, &stack, Some(&*player))
                }),
                block_pos,
                EntitySpawnReason::SpawnItemUse,
                true,
                true,
            ) else {
                return InteractionResult::FAIL;
            };
            let y_rot = mth::floor_f32(
                (mth::wrap_degrees_f32(context.get_rotation() - 180.0) + 22.5) / 45.0,
            ) as f32
                * 45.0;
            let p = entity.base().position();
            entity.snap_to(p.x, p.y, p.z, y_rot, 0.0);
            let _entity_id = entity.id();
            level.add_fresh_entity_with_passengers(vec![entity]);
            level.play_sound_at(
                None,
                p.x,
                p.y,
                p.z,
                SoundEvent::ENTITY_ARMOR_STAND_PLACE,
                SoundSource::Blocks,
                0.75,
                0.8,
            );
            level.game_event_from_entity(Some(player.id()), GameEvent::ENTITY_PLACE, p);
            context.item_stack.shrink(1);
            InteractionResult::SUCCESS
        } else {
            InteractionResult::FAIL
        }
    }
}

/// `HangingEntityItem` (`item_frame_variant`: `ItemFrameItem`'s `mayPlace` also needs the build height).
struct HangingEntityItem {
    item_frame_item: bool,
    entity_type: EntityType,
}

impl HangingEntityItem {
    /// `mayPlace(player, direction, itemStack, blockPos)`.
    fn may_place(
        &self,
        level: &ServerLevel,
        player: &ServerPlayer,
        direction: Direction,
        stack: &ItemStack,
        block_pos: BlockPos,
    ) -> bool {
        if self.item_frame_item {
            !level.is_outside_build_height(block_pos.y())
                && may_use_item_at(level, player, block_pos, direction, stack)
        } else {
            !direction.get_axis().is_vertical()
                && may_use_item_at(level, player, block_pos, direction, stack)
        }
    }
}

impl ItemBehaviour for HangingEntityItem {
    fn use_on(
        &self,
        context: &mut UseOnContext,
        level: &mut ServerLevel,
        player: &mut ServerPlayer,
    ) -> InteractionResult {
        let pos = context.get_clicked_pos();
        let clicked_face = context.get_clicked_face();
        let block_pos = pos.relative(clicked_face);
        let stack = context.item_stack.clone();
        if !self.may_place(level, player, clicked_face, &stack, block_pos) {
            return InteractionResult::FAIL;
        }
        let mut entity: Box<dyn Entity> = match self.entity_type {
            EntityType::PAINTING => match Painting::create(level, block_pos, clicked_face) {
                Some(painting) => Box::new(painting),
                None => return InteractionResult::CONSUME,
            },
            EntityType::ITEM_FRAME | EntityType::GLOW_ITEM_FRAME => Box::new(ItemFrame::at(
                self.entity_type,
                level.next_entity_id(),
                block_pos,
                clicked_face,
            )),
            _ => return InteractionResult::SUCCESS,
        };
        apply_default_stack_config(level, entity.as_mut(), &stack, Some(&*player));
        let survives = survives_dyn(entity.as_ref(), level);
        if survives {
            play_placement_sound(entity.as_ref(), level);
            level.game_event_from_entity(
                Some(player.id()),
                GameEvent::ENTITY_PLACE,
                entity.base().position(),
            );
            level.add_fresh_entity(entity);
            context.item_stack.shrink(1);
            InteractionResult::SUCCESS
        } else {
            InteractionResult::CONSUME
        }
    }
}

/// `HangingEntity.survives()` of a boxed hanging entity.
fn survives_dyn(entity: &dyn Entity, level: &ServerLevel) -> bool {
    use crate::world::entity::decoration::block_attached::BlockAttached;
    if let Some(frame) = entity.as_any().downcast_ref::<ItemFrame>() {
        return frame.survives(level);
    }
    entity
        .as_any()
        .downcast_ref::<Painting>()
        .is_some_and(|p| p.survives(level))
}

/// `HangingEntity.playPlacementSound()`.
fn play_placement_sound(entity: &dyn Entity, level: &mut ServerLevel) {
    if let Some(frame) = entity.as_any().downcast_ref::<ItemFrame>() {
        frame.play_placement_sound(level);
    } else if let Some(painting) = entity.as_any().downcast_ref::<Painting>() {
        painting.play_placement_sound(level);
    }
}

register_item_class!("HangingEntityItem", |item| Box::new(HangingEntityItem {
    item_frame_item: false,
    entity_type: hanging_entity_type(item)
}));
register_item_class!("ItemFrameItem", |item| Box::new(HangingEntityItem {
    item_frame_item: true,
    entity_type: hanging_entity_type(item)
}));
register_item_class!("ArmorStandItem", |_| Box::new(ArmorStandItem));
register_item_class!("CushionItem", |_| Box::new(CushionItem));

/// The `EntityType` a hanging entity item places (`Items.PAINTING/ITEM_FRAME/GLOW_ITEM_FRAME`).
fn hanging_entity_type(item: Item) -> EntityType {
    match item {
        Item::ITEM_FRAME => EntityType::ITEM_FRAME,
        Item::GLOW_ITEM_FRAME => EntityType::GLOW_ITEM_FRAME,
        _ => EntityType::PAINTING,
    }
}

/// `CushionItem`.
struct CushionItem;

/// `CushionItem.COLLISION_SHAPE_RAYCAST_EPSILON`.
const COLLISION_SHAPE_RAYCAST_EPSILON: f64 = 0.001;

impl CushionItem {
    /// `recalculateContextForSpecialCollisionShapes(context)`.
    fn recalculate_context(
        context: &UseOnContext,
        level: &ServerLevel,
        player: &ServerPlayer,
    ) -> UseOnContext {
        let clicked_pos = context.get_clicked_pos();
        let clicked_state = level.get_block_state(clicked_pos);
        if !clicked_state
            .block()
            .is_in_tag("cushion_uses_collision_shape")
        {
            return context.clone();
        }
        let ray_from = player.base().get_eye_position();
        let ray = context.get_click_location().subtract(ray_from);
        let ray_to = context
            .get_click_location()
            .add(ray.normalize().scale(COLLISION_SHAPE_RAYCAST_EPSILON));
        match clicked_state.get_collision_shape(level, clicked_pos).clip(
            ray_from,
            ray_to,
            clicked_pos,
        ) {
            Some(hit) => UseOnContext::new(
                context.player,
                context.hand,
                context.item_stack.clone(),
                hit,
            ),
            None => context.clone(),
        }
    }
}

impl ItemBehaviour for CushionItem {
    fn use_on(
        &self,
        context: &mut UseOnContext,
        level: &mut ServerLevel,
        player: &mut ServerPlayer,
    ) -> InteractionResult {
        let recalculated = Self::recalculate_context(context, level, player);
        if recalculated.get_clicked_face() != Direction::Up {
            return InteractionResult::FAIL;
        }
        let place_context =
            BlockPlaceContext::new(recalculated.clone(), |pos| level.get_block_state(pos));
        let block_pos = place_context.get_clicked_pos();
        let entity_pos = Vec3::at_center_of_with_y(&block_pos, recalculated.get_click_location().y);
        let spawn_aabb = get_spawn_aabb(EntityType::CUSHION, entity_pos);
        if !Cushion::can_be_placed_at(level, &spawn_aabb) {
            return InteractionResult::FAIL;
        }
        if !level
            .get_entities_of_class(
                &spawn_aabb,
                |e| e.get_type() == EntityType::CUSHION,
                |_| true,
            )
            .is_empty()
        {
            return InteractionResult::FAIL;
        }
        let stack = context.item_stack.clone();
        let Some(mut cushion) = create_at(
            level,
            EntityType::CUSHION,
            Some(&mut |level: &mut ServerLevel, entity: &mut dyn Entity| {
                apply_default_stack_config(level, entity, &stack, Some(&*player))
            }),
            block_pos,
            EntitySpawnReason::SpawnItemUse,
            true,
            true,
        ) else {
            return InteractionResult::FAIL;
        };
        let y_rot = Direction::from_y_rot(f64::from(place_context.get_rotation())).to_y_rot();
        cushion.snap_to(entity_pos.x, entity_pos.y, entity_pos.z, y_rot, 0.0);
        let id = cushion.id();
        level.add_fresh_entity_with_passengers(vec![cushion]);
        level.with_entity(id, |level, cushion| {
            if let Some(c) = cushion.as_any_mut().downcast_mut::<Cushion>() {
                c.destroy_if_in_fire(level);
            }
        });
        level.play_sound_at(
            None,
            entity_pos.x,
            entity_pos.y,
            entity_pos.z,
            SoundEvent::ENTITY_CUSHION_PLACE,
            SoundSource::Blocks,
            0.75,
            0.8,
        );
        level.game_event_from_entity(Some(player.id()), GameEvent::ENTITY_PLACE, entity_pos);
        super::ItemStackExt::consume(&mut context.item_stack, 1, player.has_infinite_materials());
        InteractionResult::SUCCESS
    }
}

/// `EntityType.getSpawnAABB(pos)`: the type's dimensions scaled by `spawnDimensionsScale` around `pos` (bottom center).
fn get_spawn_aabb(ty: EntityType, pos: Vec3) -> AABB {
    EntityDimensions::of_type(ty)
        .scale(ty.data().spawn_dimensions_scale)
        .make_bounding_box_vec(pos)
}
