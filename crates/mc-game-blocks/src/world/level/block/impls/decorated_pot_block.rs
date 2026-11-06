//! `net.minecraft.world.level.block.DecoratedPotBlock`.
use crate::world::block_entity::decorated_pot_block_entity::{
    DecoratedPotBlockEntity, WobbleStyle, create_decorated_pot_instance,
};
use mc_core::ident::Identifier;
use mc_core::phys::BlockHitResult;
use mc_data::fluid::Fluid;
use mc_data::{GameEvent, Item, ParticleType, SoundEvent};
use mc_game_core::server::level::ServerLevel;
use mc_game_core::stats::Stat;
use mc_game_core::world::entity::projectile::Projectile;
use mc_game_core::world::entity::{Entity, ServerPlayer};
use mc_game_core::world::item::{
    BlockPlaceContext, InteractionHand, InteractionResult, ItemStack, ItemStackExt,
};
use mc_game_core::world::level::block::impls::prelude::*;
use mc_game_core::world::level::gameevent::GameEventContext;
use mc_game_core::{register_block_class, update_shape};
use mc_protocol::packets::play::ParticleOptions;
use mc_protocol::packets::play::level::SoundSource;

pub struct DecoratedPotBlock;

fn item_in_tag(item: Item, tag: &str) -> bool {
    mc_data::tags::get::<Item>(tag).is_some_and(|t| t.contains(item))
}

impl BlockBehaviour for DecoratedPotBlock {
    mc_game_core::simple_waterlogged!();
    fn get_state_for_placement(
        &self,
        block: Block,
        level: &mut ServerLevel,
        context: &BlockPlaceContext,
    ) -> Option<BlockState> {
        let replaced_fluid_state = level.get_fluid_state(context.get_clicked_pos());
        Some(
            block
                .default_state()
                .set(props::HORIZONTAL_FACING, context.get_horizontal_direction())
                .set(
                    props::WATERLOGGED,
                    replaced_fluid_state.fluid() == Fluid::WATER,
                )
                .set(props::CRACKED, false),
        )
    }

    update_shape!(
        |self, state, level, pos, direction, neighbour_pos, neighbour_state| {
            if state.get(props::WATERLOGGED) {
                helpers::schedule_water_tick(level, pos);
            }
            state
        }
    );

    fn use_item_on(
        &self,
        stack: &ItemStack,
        _state: BlockState,
        level: &mut ServerLevel,
        pos: BlockPos,
        player: &mut ServerPlayer,
        hand: InteractionHand,
        _hit: &BlockHitResult,
    ) -> InteractionResult {
        if !level
            .get_block_entity(pos)
            .is_some_and(|be| be.as_any().is::<DecoratedPotBlockEntity>())
        {
            return InteractionResult::PASS;
        }
        let item_stack = stack.clone();
        let pot_item = level
            .get_block_entity(pos)
            .and_then(|be| be.as_any().downcast_ref::<DecoratedPotBlockEntity>())
            .map(|p| p.the_item().clone())
            .unwrap_or_default();
        if !item_stack.is_empty()
            && (pot_item.is_empty()
                || (ItemStack::is_same_item_same_components(&pot_item, &item_stack)
                    && pot_item.count() < pot_item.get_max_stack_size()))
        {
            level.with_block_entity(pos, |level, be| {
                if let Some(pot) = be.as_any_mut().downcast_mut::<DecoratedPotBlockEntity>() {
                    pot.wobble(level, WobbleStyle::Positive);
                }
            });
            player.award_stat_once(&*level, Stat::Used(item_stack.item()));
            let infinite = player.player.abilities.instabuild;
            let awarded = player
                .player
                .inventory
                .equipment_get_mut(hand.as_equipment_slot())
                .consume_and_return(1, infinite);
            let pitch_bend = level
                .with_block_entity(pos, |_, be| {
                    let pot = be.as_any_mut().downcast_mut::<DecoratedPotBlockEntity>()?;
                    if pot.the_item().is_empty() {
                        let bend = awarded.count() as f32 / awarded.get_max_stack_size() as f32;
                        pot.set_the_item(awarded);
                        Some(bend)
                    } else {
                        pot.the_item_mut().grow(1);
                        Some(
                            pot.the_item().count() as f32
                                / pot.the_item().get_max_stack_size() as f32,
                        )
                    }
                })
                .flatten()
                .unwrap_or(0.0);
            level.play_sound(
                None,
                pos,
                SoundEvent::BLOCK_DECORATED_POT_INSERT,
                SoundSource::Blocks,
                1.0,
                0.7 + 0.5 * pitch_bend,
            );
            level.send_particles(
                ParticleOptions::Simple(ParticleType::DUST_PLUME),
                f64::from(pos.x()) + 0.5,
                f64::from(pos.y()) + 1.2,
                f64::from(pos.z()) + 0.5,
                7,
                0.0,
                0.0,
                0.0,
                0.0,
            );
            mc_game_core::world::block_entity::set_changed(level, pos, level.get_block_state(pos));
            level.game_event_at_block(
                GameEvent::BLOCK_CHANGE,
                pos,
                &GameEventContext::of_entity(Some(player.id())),
            );
            InteractionResult::SUCCESS
        } else {
            InteractionResult::TRY_WITH_EMPTY_HAND
        }
    }

    fn use_without_item(
        &self,
        _state: BlockState,
        level: &mut ServerLevel,
        pos: BlockPos,
        player: &mut ServerPlayer,
        _hit: &BlockHitResult,
    ) -> InteractionResult {
        if level
            .get_block_entity(pos)
            .is_some_and(|be| be.as_any().is::<DecoratedPotBlockEntity>())
        {
            level.play_sound(
                None,
                pos,
                SoundEvent::BLOCK_DECORATED_POT_INSERT_FAIL,
                SoundSource::Blocks,
                1.0,
                1.0,
            );
            level.with_block_entity(pos, |level, be| {
                if let Some(pot) = be.as_any_mut().downcast_mut::<DecoratedPotBlockEntity>() {
                    pot.wobble(level, WobbleStyle::Negative);
                }
            });
            level.game_event_at_block(
                GameEvent::BLOCK_CHANGE,
                pos,
                &GameEventContext::of_entity(Some(player.id())),
            );
            InteractionResult::SUCCESS
        } else {
            InteractionResult::PASS
        }
    }

    fn trigger_event(
        &self,
        _state: BlockState,
        level: &mut ServerLevel,
        pos: BlockPos,
        b0: i32,
        b1: i32,
    ) -> bool {
        helpers::base_entity_trigger_event(level, pos, b0, b1)
    }

    fn affect_neighbors_after_removal(
        &self,
        state: BlockState,
        level: &mut ServerLevel,
        pos: BlockPos,
        _moved_by_piston: bool,
    ) {
        level.update_neighbour_for_output_signal(pos, state.block());
    }

    fn player_will_destroy(
        &self,
        state: BlockState,
        level: &mut ServerLevel,
        pos: BlockPos,
        player: &mut ServerPlayer,
    ) -> BlockState {
        let destroyed_with = player.get_item_in_hand(InteractionHand::MainHand);
        let mut next_state = state;
        if item_in_tag(destroyed_with.item(), "breaks_decorated_pots")
            && !mc_game_core::world::item::enchantment_helper::has_tag(
                &level.registries,
                &destroyed_with,
                &Identifier::parse("minecraft:prevents_decorated_pot_shattering")
                    .expect("valid id"),
            )
        {
            next_state = state.set(props::CRACKED, true);
            level.set_block(pos, next_state, 260, 512);
        }
        mc_game_core::server::level::block_break::player_will_destroy(
            level, pos, next_state, player,
        )
    }

    fn on_projectile_hit(
        &self,
        level: &mut ServerLevel,
        state: BlockState,
        hit: &BlockHitResult,
        projectile: &mut dyn Projectile,
    ) {
        let pos = hit.block_pos;
        if projectile.may_interact_projectile(level, pos) && projectile.may_break(level, pos) {
            level.set_block(pos, state.set(props::CRACKED, true), 260, 512);
            LevelAccessor::destroy_block(level, pos, true, Some(projectile.id()), 512);
        }
    }

    fn get_clone_item_stack(
        &self,
        state: BlockState,
        level: &ServerLevel,
        pos: BlockPos,
        _include_data: bool,
    ) -> ItemStack {
        match level
            .get_block_entity(pos)
            .and_then(|be| be.as_any().downcast_ref::<DecoratedPotBlockEntity>())
        {
            Some(pot) => create_decorated_pot_instance(pot.get_decorations()),
            None => state
                .block()
                .item()
                .map_or_else(ItemStack::empty, ItemStack::new),
        }
    }

    fn get_analog_output_signal(
        &self,
        _state: BlockState,
        level: &ServerLevel,
        pos: BlockPos,
        _direction: Direction,
    ) -> i32 {
        mc_game_core::world::container::get_redstone_signal_from_block_entity(
            level.get_block_entity(pos),
        )
    }

    fn rotate(&self, state: BlockState, rotation: Rotation) -> BlockState {
        helpers::rotate_facing(state, props::HORIZONTAL_FACING, rotation)
    }
    fn mirror(&self, state: BlockState, mirror: Mirror) -> BlockState {
        helpers::mirror_facing(state, props::HORIZONTAL_FACING, mirror)
    }
}

register_block_class!("DecoratedPotBlock", |_| Box::new(DecoratedPotBlock));
