//! `PotionItem` (+ `SplashPotionItem` / `LingeringPotionItem` via [`super::throwable`]), `TippedArrowItem`, `BottleItem`.
//!
//! Drinking a potion is data driven (`consumable` + the `potion_contents` listener in `world::effect::consume`).

use super::throwable::{ThrowKind, throw_item};
use super::{
    InteractionHand, InteractionResult, ItemBehaviour, ItemStack, PlainItem, UseOnContext,
    create_filled_result, player_pov_hit_result,
};
use crate::register_item_class;
use crate::server::level::ServerLevel;
use crate::world::effect::potion_contents;
use crate::world::entity::area_effect_cloud::AreaEffectCloud;
use crate::world::entity::player::menu_ctx::PlayerCtx;
use crate::world::entity::{Entity, ServerPlayer};
use crate::world::level::block::state::BlockStateExt;
use crate::world::level::gameevent::GameEventContext;
use crate::world::level::material::FluidStateExt;
use crate::world::level::{BlockGetter, LevelAccessor};
use mc_core::direction::Direction;
use mc_data::{EntityType, GameEvent, Item, ParticleType, Potion, SoundEvent};
use mc_protocol::components::{POTION_CONTENTS, PotionContents};
use mc_protocol::packets::play::ParticleOptions;
use mc_protocol::packets::play::SoundSource;
use mc_text::Component;

/// `PotionContents.createItemStack(item, potion)`.
pub fn create_potion_item_stack(item: Item, potion: Potion) -> ItemStack {
    let mut stack = ItemStack::new(item);
    stack.set(
        POTION_CONTENTS,
        PotionContents {
            potion: Some(potion),
            custom_color: None,
            custom_effects: Vec::new(),
            custom_name: None,
        },
    );
    stack
}

/// `PotionContents.getName(descriptionId + ".effect.")` when the stack has `potion_contents`, else `Item.getName`.
fn potion_name(description_id: &str, stack: &ItemStack) -> Component {
    match stack.get(&POTION_CONTENTS) {
        Some(contents) => Component::translatable(
            potion_contents::get_name_key(contents, &format!("{description_id}.effect.")),
            Vec::new(),
        ),
        None => PlainItem.get_name(stack),
    }
}

/// `PotionItem`; `throw` is set for the splash / lingering subclasses (`ThrowablePotionItem`).
pub struct PotionItem {
    description_id: &'static str,
    throw: Option<ThrowKind>,
}

impl ItemBehaviour for PotionItem {
    /// `PotionItem.useOn`: a water potion turns mud-convertible blocks into mud.
    fn use_on(
        &self,
        context: &mut UseOnContext,
        level: &mut ServerLevel,
        player: &mut ServerPlayer,
    ) -> InteractionResult {
        let pos = context.get_clicked_pos();
        let hand = context.get_hand();
        let is_water = context
            .item_stack
            .get(&POTION_CONTENTS)
            .is_some_and(|c| potion_contents::is(c, Potion::WATER));
        let state = BlockGetter::get_block_state(level, pos);
        if context.get_clicked_face() == Direction::Down
            || !state.is_tag("convertible_to_mud")
            || !is_water
        {
            return InteractionResult::PASS;
        }
        level.play_sound(
            None,
            pos,
            SoundEvent::ENTITY_GENERIC_SPLASH,
            SoundSource::Blocks,
            1.0,
            1.0,
        );
        let filled = {
            let mut ctx = PlayerCtx { player, level };
            create_filled_result(
                context.item_stack.clone(),
                &mut ctx,
                ItemStack::new(Item::GLASS_BOTTLE),
                true,
            )
        };
        player.set_item_in_hand_slot(level, hand, filled.clone());
        context.item_stack = filled;
        for _ in 0..5 {
            let x = f64::from(pos.x()) + level.random().next_double();
            let z = f64::from(pos.z()) + level.random().next_double();
            level.send_particles(
                ParticleOptions::Simple(ParticleType::SPLASH),
                x,
                f64::from(pos.y() + 1),
                z,
                1,
                0.0,
                0.0,
                0.0,
                1.0,
            );
        }
        level.play_sound(
            None,
            pos,
            SoundEvent::ITEM_BOTTLE_EMPTY,
            SoundSource::Blocks,
            1.0,
            1.0,
        );
        level.game_event_at_block(
            GameEvent::FLUID_PLACE,
            pos,
            &GameEventContext::of_entity(None),
        );
        level.set_block_and_update(pos, mc_data::block::Block::MUD.default_state());
        InteractionResult::SUCCESS
    }

    /// `ThrowablePotionItem.use` (+ the throw sound of `SplashPotionItem` / `LingeringPotionItem`).
    fn use_item(
        &self,
        level: &mut ServerLevel,
        player: &mut ServerPlayer,
        hand: InteractionHand,
    ) -> InteractionResult {
        match self.throw {
            Some(kind) => throw_item(kind, level, player, hand),
            None => super::consumable::base_use(level, player, hand),
        }
    }

    fn get_name(&self, stack: &ItemStack) -> Component {
        potion_name(self.description_id, stack)
    }
}

register_item_class!("PotionItem", |item| Box::new(PotionItem {
    description_id: item.data().description_id,
    throw: None
}));
register_item_class!("SplashPotionItem", |item| Box::new(PotionItem {
    description_id: item.data().description_id,
    throw: Some(ThrowKind::SplashPotion)
}));
register_item_class!("LingeringPotionItem", |item| Box::new(PotionItem {
    description_id: item.data().description_id,
    throw: Some(ThrowKind::LingeringPotion)
}));

/// `TippedArrowItem.getName`.
pub struct TippedArrowItem {
    description_id: &'static str,
}

impl ItemBehaviour for TippedArrowItem {
    fn as_projectile_item(&self) -> Option<&dyn super::ProjectileItem> {
        Some(&super::arrow_item::ARROW_PROJECTILE)
    }

    fn get_name(&self, stack: &ItemStack) -> Component {
        potion_name(self.description_id, stack)
    }
}

register_item_class!("TippedArrowItem", |item| Box::new(TippedArrowItem {
    description_id: item.data().description_id
}));

/// `Level.mayInteract(player, pos)`.
fn may_interact(level: &ServerLevel, player: &ServerPlayer, pos: mc_core::pos::BlockPos) -> bool {
    !level.is_under_spawn_protection(player, pos)
        && level
            .world_border_ref()
            .is_none_or(|b| b.is_within_bounds_block(&pos))
}

/// `BottleItem`.
pub struct BottleItem;

impl BottleItem {
    /// `turnBottleIntoItem(itemStack, player, newStack)`.
    fn turn_bottle_into_item(
        &self,
        stack: ItemStack,
        level: &mut ServerLevel,
        player: &mut ServerPlayer,
        turn_into: ItemStack,
    ) -> ItemStack {
        player.award_stat_once(
            level,
            mc_protocol::packets::play::Stat::Used(Item::GLASS_BOTTLE),
        );
        let mut ctx = PlayerCtx { player, level };
        create_filled_result(stack, &mut ctx, turn_into, true)
    }
}

impl ItemBehaviour for BottleItem {
    fn use_item(
        &self,
        level: &mut ServerLevel,
        player: &mut ServerPlayer,
        hand: InteractionHand,
    ) -> InteractionResult {
        let stack = player.get_item_in_hand(hand);
        let (x, y, z) = (player.base().x(), player.base().y(), player.base().z());
        // clouds of the ender dragon within 2 blocks: `getEntitiesOfClass(AreaEffectCloud, bb.inflate(2), alive && owner is EnderDragon)`
        let area = player.get_bounding_box().inflate(2.0, 2.0, 2.0);
        let dragon_clouds = {
            let level_ref: &ServerLevel = level;
            level.get_entities_of_class(
                &area,
                |e| e.as_any().is::<AreaEffectCloud>(),
                |e| {
                    e.is_alive()
                        && e.as_any()
                            .downcast_ref::<AreaEffectCloud>()
                            .and_then(|c| c.get_owner(level_ref))
                            .is_some_and(|o| o.get_type() == EntityType::ENDER_DRAGON)
                },
            )
        };
        if let Some(&cloud) = dragon_clouds.first() {
            level.with_entity(cloud, |_, e| {
                if let Some(c) = e.as_any_mut().downcast_mut::<AreaEffectCloud>() {
                    c.set_radius(c.get_radius() - 0.5);
                }
            });
            level.play_sound_at(
                None,
                x,
                y,
                z,
                SoundEvent::ITEM_BOTTLE_FILL_DRAGONBREATH,
                SoundSource::Neutral,
                1.0,
                1.0,
            );
            level.game_event_from_entity(
                Some(player.id()),
                GameEvent::FLUID_PICKUP,
                player.position(),
            );
            if let Some(cloud_entity) = level.get_entity(cloud) {
                let matches =
                    crate::advancements::triggers::CriteriaTriggers::PLAYER_INTERACTED_WITH_ENTITY
                        .matches(&*level, &*player, &stack, cloud_entity);
                matches.award(level, player);
            }
            let InteractionResult::Success(success) = InteractionResult::SUCCESS else {
                unreachable!()
            };
            return InteractionResult::Success(success.held_item_transformed_to(
                self.turn_bottle_into_item(
                    stack,
                    level,
                    player,
                    ItemStack::new(Item::DRAGON_BREATH),
                ),
            ));
        }
        let hit = player_pov_hit_result(level, player, true);
        if hit.miss {
            return InteractionResult::PASS;
        }
        let pos = hit.block_pos;
        if !may_interact(level, player, pos)
            || !BlockGetter::get_fluid_state(level, pos).is_tag("water")
        {
            return InteractionResult::PASS;
        }
        level.play_sound_at(
            Some(player.id()),
            x,
            y,
            z,
            SoundEvent::ITEM_BOTTLE_FILL,
            SoundSource::Neutral,
            1.0,
            1.0,
        );
        level.game_event_at_block(
            GameEvent::FLUID_PICKUP,
            pos,
            &GameEventContext::of_entity(Some(player.id())),
        );
        let water = create_potion_item_stack(Item::POTION, Potion::WATER);
        let InteractionResult::Success(success) = InteractionResult::SUCCESS else {
            unreachable!()
        };
        InteractionResult::Success(
            success
                .held_item_transformed_to(self.turn_bottle_into_item(stack, level, player, water)),
        )
    }
}

register_item_class!("BottleItem", |_| Box::new(BottleItem));
