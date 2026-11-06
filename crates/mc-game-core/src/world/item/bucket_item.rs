//! `BucketItem` (empty/water/lava buckets), `MobBucketItem` (fish/axolotl/tadpole/sulfur cube buckets) and the
//! `DispensibleContainerItem` interface they share with `SolidBucketItem` (powder snow, see `block_item.rs`).

use super::{
    InteractionHand, InteractionResult, ItemBehaviour, ItemStack, bucket_empty_success_item,
    create_filled_result,
    item_use_helpers::{may_interact, may_use_item_at, player_play_sound},
    player_pov_hit_result,
};
use crate::register_item_class;
use crate::server::level::ServerLevel;
use crate::world::entity::player::menu_ctx::PlayerCtx;
use crate::world::entity::{Entity, ServerPlayer};
use crate::world::level::block::{BlockStateExt, behaviour as block_behaviour};
use crate::world::level::gameevent::GameEventContext;
use crate::world::level::{BlockGetter, LevelAccessor};
use mc_core::phys::{BlockHitResult, Vec3};
use mc_core::pos::BlockPos;
use mc_data::fluid::Fluid;
use mc_data::{EntityType, GameEvent, Item, SoundEvent};
use mc_protocol::packets::play::{ParticleOptions, RandomizationType, SoundSource, Stat};

/// `DispensibleContainerItem`: `checkExtraContent` / `emptyContents` (dispensers call these with a null user).
pub trait DispensibleContainerItem {
    /// `checkExtraContent(user, level, stack, pos)`.
    fn check_extra_content(
        &self,
        _user: Option<&dyn Entity>,
        _level: &mut ServerLevel,
        _stack: &ItemStack,
        _pos: BlockPos,
    ) {
    }
    /// `emptyContents(user, level, pos, hitResult)`.
    fn empty_contents(
        &self,
        user: Option<&dyn Entity>,
        level: &mut ServerLevel,
        pos: BlockPos,
        hit: Option<&BlockHitResult>,
    ) -> bool;
}

/// `item instanceof DispensibleContainerItem`: `emptyContents(user, level, pos, hit)`.
pub fn empty_contents(
    item: Item,
    user: Option<&dyn Entity>,
    level: &mut ServerLevel,
    pos: BlockPos,
    hit: Option<&BlockHitResult>,
) -> Option<bool> {
    super::behaviour(item)
        .as_dispensible_container()
        .map(|c| c.empty_contents(user, level, pos, hit))
}

/// `item instanceof DispensibleContainerItem`: `checkExtraContent(user, level, stack, pos)`.
pub fn check_extra_content(
    item: Item,
    user: Option<&dyn Entity>,
    level: &mut ServerLevel,
    stack: &ItemStack,
    pos: BlockPos,
) -> bool {
    match super::behaviour(item).as_dispensible_container() {
        Some(c) => {
            c.check_extra_content(user, level, stack, pos);
            true
        }
        None => false,
    }
}

/// The entity side of a `MobBucketItem`.
struct MobBucket {
    entity_type: EntityType,
    empty_sound: SoundEvent,
}

pub struct BucketItem {
    /// `Fluids.EMPTY`, `WATER` or `LAVA` (the source fluid).
    content: Fluid,
    mob: Option<MobBucket>,
}

impl BucketItem {
    fn plain(item: Item) -> Self {
        let content = match item.data().name {
            "minecraft:water_bucket" => Fluid::WATER,
            "minecraft:lava_bucket" => Fluid::LAVA,
            _ => Fluid::EMPTY,
        };
        BucketItem { content, mob: None }
    }

    /// `Items.<NAME>_BUCKET` constructor arguments: (entity type, content fluid, empty sound).
    fn mob(item: Item) -> Self {
        let (entity_type, content, empty_sound) = match item.data().name {
            "minecraft:pufferfish_bucket" => (
                EntityType::PUFFERFISH,
                Fluid::WATER,
                SoundEvent::ITEM_BUCKET_EMPTY_FISH,
            ),
            "minecraft:salmon_bucket" => (
                EntityType::SALMON,
                Fluid::WATER,
                SoundEvent::ITEM_BUCKET_EMPTY_FISH,
            ),
            "minecraft:cod_bucket" => (
                EntityType::COD,
                Fluid::WATER,
                SoundEvent::ITEM_BUCKET_EMPTY_FISH,
            ),
            "minecraft:tropical_fish_bucket" => (
                EntityType::TROPICAL_FISH,
                Fluid::WATER,
                SoundEvent::ITEM_BUCKET_EMPTY_FISH,
            ),
            "minecraft:axolotl_bucket" => (
                EntityType::AXOLOTL,
                Fluid::WATER,
                SoundEvent::ITEM_BUCKET_EMPTY_AXOLOTL,
            ),
            "minecraft:sulfur_cube_bucket" => (
                EntityType::SULFUR_CUBE,
                Fluid::EMPTY,
                SoundEvent::ITEM_BUCKET_EMPTY_SULFUR_CUBE,
            ),
            "minecraft:tadpole_bucket" => (
                EntityType::TADPOLE,
                Fluid::WATER,
                SoundEvent::ITEM_BUCKET_EMPTY_TADPOLE,
            ),
            other => panic!("{other} is not a MobBucketItem"),
        };
        BucketItem {
            content,
            mob: Some(MobBucket {
                entity_type,
                empty_sound,
            }),
        }
    }

    /// `getFluidContext() == ClipContext.Fluid.SOURCE_ONLY` (mob buckets never clip fluids).
    fn clip_source_only(&self) -> bool {
        self.mob.is_none() && self.content == Fluid::EMPTY
    }

    /// `playEmptySound(user, level, pos)`.
    fn play_empty_sound(&self, user: Option<&dyn Entity>, level: &mut ServerLevel, pos: BlockPos) {
        let user_id = user.map(|u| u.id());
        if let Some(mob) = &self.mob {
            level.play_sound(
                user_id,
                pos,
                mob.empty_sound,
                SoundSource::Neutral,
                1.0,
                1.0,
            );
            return;
        }
        let sound = if self.content == Fluid::LAVA {
            SoundEvent::ITEM_BUCKET_EMPTY_LAVA
        } else {
            SoundEvent::ITEM_BUCKET_EMPTY
        };
        level.play_sound(user_id, pos, sound, SoundSource::Blocks, 1.0, 1.0);
        level.game_event(
            GameEvent::FLUID_PLACE,
            Vec3::at_center_of(&pos),
            &GameEventContext::of_entity(user_id),
        );
    }

    /// `BucketItem.emptyContents` (the fluid placing part, shared by mob buckets).
    fn empty_fluid(
        &self,
        user: Option<&dyn Entity>,
        level: &mut ServerLevel,
        pos: BlockPos,
        hit: Option<&BlockHitResult>,
    ) -> bool {
        if self.content == Fluid::EMPTY {
            // `this.content instanceof FlowingFluid` is false
            return false;
        }
        let state = level.get_block_state(pos);
        let behaviour = block_behaviour(state.block());
        let may_replace = state.can_be_replaced_by_fluid(self.content);
        let shift_key_down = user.is_some_and(|u| u.is_shift_key_down());
        let place_liquid = may_replace
            || (behaviour.is_liquid_block_container()
                && behaviour.can_place_liquid(user, &*level, pos, state, self.content));
        let can_place_fluid_inside_block =
            state.is_air() || (place_liquid && (!shift_key_down || hit.is_none()));
        if can_place_fluid_inside_block {
            if level.attribute_bool(crate::world::attribute::attributes::WATER_EVAPORATES, pos)
                && self.content == Fluid::WATER
            {
                let (x, y, z) = (pos.x(), pos.y(), pos.z());
                let random = LevelAccessor::random(level);
                let pitch = 2.6 + (random.next_float() - random.next_float()) * 0.8;
                level.play_sound(
                    user.map(|u| u.id()),
                    pos,
                    SoundEvent::BLOCK_FIRE_EXTINGUISH,
                    SoundSource::Blocks,
                    0.5,
                    pitch,
                );
                level.send_particles_full(
                    ParticleOptions::Simple(mc_data::ParticleType::LARGE_SMOKE),
                    false,
                    false,
                    f64::from(x),
                    f64::from(y),
                    f64::from(z),
                    8,
                    1.0,
                    1.0,
                    1.0,
                    0.0,
                    0.0,
                    0.0,
                    RandomizationType::Alternative,
                );
                true
            } else if behaviour.is_liquid_block_container() && self.content == Fluid::WATER {
                behaviour.place_liquid(level, pos, state, self.content.default_state());
                self.play_empty_sound(user, level, pos);
                true
            } else {
                if may_replace && !state.liquid() {
                    level.destroy_block(pos, true, None, 512);
                }
                if !LevelAccessor::set_block_flags(
                    level,
                    pos,
                    self.content.default_state().create_legacy_block(),
                    11,
                ) && !state.get_fluid_state().is_source()
                {
                    false
                } else {
                    self.play_empty_sound(user, level, pos);
                    true
                }
            }
        } else {
            match hit {
                Some(hit) => self.empty_contents_impl(
                    user,
                    level,
                    hit.block_pos.relative(hit.direction),
                    None,
                ),
                None => false,
            }
        }
    }

    /// The virtual `emptyContents(user, level, pos, hitResult)`.
    fn empty_contents_impl(
        &self,
        user: Option<&dyn Entity>,
        level: &mut ServerLevel,
        pos: BlockPos,
        hit: Option<&BlockHitResult>,
    ) -> bool {
        if self.mob.is_some() && self.content == Fluid::EMPTY {
            // `MobBucketItem.emptyContents` for the sulfur cube bucket
            self.play_empty_sound(user, level, pos);
            return true;
        }
        self.empty_fluid(user, level, pos, hit)
    }
}

impl DispensibleContainerItem for BucketItem {
    fn check_extra_content(
        &self,
        user: Option<&dyn Entity>,
        level: &mut ServerLevel,
        stack: &ItemStack,
        pos: BlockPos,
    ) {
        if let Some(mob) = &self.mob {
            super::mob_bucket::spawn_from_bucket(mob.entity_type, user, level, stack, pos);
            level.game_event(
                GameEvent::ENTITY_PLACE,
                Vec3::at_center_of(&pos),
                &GameEventContext::of_entity(user.map(|u| u.id())),
            );
        }
    }

    fn empty_contents(
        &self,
        user: Option<&dyn Entity>,
        level: &mut ServerLevel,
        pos: BlockPos,
        hit: Option<&BlockHitResult>,
    ) -> bool {
        self.empty_contents_impl(user, level, pos, hit)
    }
}

impl ItemBehaviour for BucketItem {
    fn use_item(
        &self,
        level: &mut ServerLevel,
        player: &mut ServerPlayer,
        hand: InteractionHand,
    ) -> InteractionResult {
        let stack = player.get_item_in_hand(hand);
        let hit = player_pov_hit_result(level, player, self.clip_source_only());
        if hit.miss {
            return InteractionResult::PASS;
        }
        let pos = hit.block_pos;
        let direction = hit.direction;
        let direction_offset_pos = pos.relative(direction);
        if !(may_interact(level, player, pos)
            && may_use_item_at(level, player, direction_offset_pos, direction, &stack))
        {
            return InteractionResult::FAIL;
        }
        let clicked = level.get_block_state(pos);
        let place_pos = if block_behaviour(clicked.block()).is_liquid_block_container()
            && self.content == Fluid::WATER
        {
            pos
        } else {
            direction_offset_pos
        };
        if self.empty_contents_impl(Some(&*player), level, place_pos, Some(&hit)) {
            self.check_extra_content(Some(&*player), level, &stack, place_pos);
            if self.content != Fluid::EMPTY {
                crate::advancements::triggers::CriteriaTriggers::PLACED_BLOCK
                    .trigger(level, player, place_pos, &stack);
            }
            player.award_stat_once(level, Stat::Used(stack.item()));
            let replacement = bucket_empty_success_item(&stack, player);
            let empty_result =
                create_filled_result(stack, &mut PlayerCtx { player, level }, replacement, false);
            return InteractionResult::SUCCESS.held_item_transformed_to(empty_result);
        }
        if self.content == Fluid::EMPTY {
            let block_state = level.get_block_state(pos);
            let behaviour = block_behaviour(block_state.block());
            if behaviour.is_bucket_pickup() {
                let taken = behaviour.pickup_block(Some(&*player), level, pos, block_state);
                if !taken.is_empty() {
                    player.award_stat_once(level, Stat::Used(stack.item()));
                    if let Some(sound) = behaviour.get_pickup_sound() {
                        player_play_sound(level, player, sound, 1.0, 1.0);
                    }
                    level.game_event(
                        GameEvent::FLUID_PICKUP,
                        Vec3::at_center_of(&pos),
                        &GameEventContext::of_entity(Some(player.base_id())),
                    );
                    let result = create_filled_result(
                        stack,
                        &mut PlayerCtx { player, level },
                        taken.clone(),
                        false,
                    );
                    crate::advancements::triggers::CriteriaTriggers::FILLED_BUCKET
                        .trigger(level, player, &taken);
                    return InteractionResult::SUCCESS.held_item_transformed_to(result);
                }
            }
        }
        InteractionResult::FAIL
    }

    fn as_dispensible_container(&self) -> Option<&dyn DispensibleContainerItem> {
        Some(self)
    }
}

register_item_class!("BucketItem", |item| Box::new(BucketItem::plain(item)));
register_item_class!("MobBucketItem", |item| Box::new(BucketItem::mob(item)));
