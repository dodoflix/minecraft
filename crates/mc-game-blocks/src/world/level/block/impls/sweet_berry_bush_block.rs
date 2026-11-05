//! `net.minecraft.world.level.block.SweetBerryBushBlock` (`entityInside`).
use mc_game_core::register_block_class;
use mc_game_core::world::level::block::impls::prelude::*;
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::entity::Entity;
use mc_game_core::world::entity::inside_block_effects::InsideBlockEffectApplier;
use mc_game_core::world::level::block::impls::vegetation_block::{MayPlaceOn, vegetation_behaviour_methods};
use mc_game_core::world::entity::ServerPlayer;
use mc_game_core::world::item::{InteractionHand, InteractionResult, ItemStack};
use mc_game_core::world::level::block::bonemeal::BonemealSource;
use mc_game_core::world::level::gameevent::GameEventContext;
use mc_game_core::world::level::storage::loot::block_drops::block_interact_drops;
use mc_core::phys::Vec3;
use mc_data::{GameEvent, Item, SoundEvent};
use mc_protocol::packets::play::level::SoundSource;

pub struct SweetBerryBushBlock;

impl MayPlaceOn for SweetBerryBushBlock {}

impl BlockBehaviour for SweetBerryBushBlock {
    vegetation_behaviour_methods!();
    fn get_clone_item_stack(&self, _state: BlockState, _level: &ServerLevel, _pos: BlockPos, _include_data: bool) -> ItemStack {
        ItemStack::new(Item::SWEET_BERRIES)
    }
    fn random_tick(&self, state: BlockState, level: &mut ServerLevel, pos: BlockPos) {
        let age = state.get(props::AGE_3);
        if age < 3 && level.random().next_int_bounded(5) == 0 && level.get_raw_brightness(pos.above(), 0) >= 9 {
            let new_state = state.set(props::AGE_3, age + 1);
            level.set_block_flags(pos, new_state, 2);
            level.game_event_at_block(GameEvent::BLOCK_CHANGE, pos, &GameEventContext::of_state(Some(new_state)));
        }
    }
    fn use_item_on(
        &self,
        stack: &ItemStack,
        state: BlockState,
        _level: &mut ServerLevel,
        _pos: BlockPos,
        _player: &mut ServerPlayer,
        _hand: InteractionHand,
        _hit: &mc_core::phys::BlockHitResult,
    ) -> InteractionResult {
        let is_max_age = state.get(props::AGE_3) == 3;
        if !is_max_age && stack.is(Item::BONE_MEAL) { InteractionResult::PASS } else { InteractionResult::TRY_WITH_EMPTY_HAND }
    }
    fn use_without_item(&self, state: BlockState, level: &mut ServerLevel, pos: BlockPos, player: &mut ServerPlayer, _hit: &mc_core::phys::BlockHitResult) -> InteractionResult {
        if state.get(props::AGE_3) > 1 {
            let drops = block_interact_drops(level, "harvest/sweet_berry_bush", pos, state, None, Some(&*player));
            for stack in drops {
                helpers::pop_resource(level, pos, stack);
            }
            let pitch = 0.8f32 + level.random().next_float() * 0.4f32;
            level.play_sound(None, pos, SoundEvent::BLOCK_SWEET_BERRY_BUSH_PICK_BERRIES, SoundSource::Blocks, 1.0, pitch);
            let new_state = state.set(props::AGE_3, 1);
            level.set_block_flags(pos, new_state, 2);
            level.game_event_at_block(GameEvent::BLOCK_CHANGE, pos, &GameEventContext::of(Some(player.base_id()), Some(new_state)));
            InteractionResult::SUCCESS
        } else {
            InteractionResult::PASS
        }
    }
    fn is_bonemealable(&self) -> bool {
        true
    }
    fn is_valid_bonemeal_target(&self, _level: &dyn LevelReader, _pos: BlockPos, state: BlockState, _source: BonemealSource) -> bool {
        state.get(props::AGE_3) < 3
    }
    fn perform_bonemeal(&self, level: &mut ServerLevel, pos: BlockPos, state: BlockState, _source: BonemealSource) {
        let new_age = 3.min(state.get(props::AGE_3) + 1);
        level.set_block_flags(pos, state.set(props::AGE_3, new_age), 2);
    }
    fn entity_inside(&self, state: BlockState, level: &mut ServerLevel, _pos: BlockPos, entity: &mut dyn Entity, _effect_applier: &mut dyn InsideBlockEffectApplier, _is_precise: bool) {
        use mc_data::EntityType;
        if entity.living().is_some() && entity.get_type() != EntityType::FOX && entity.get_type() != EntityType::BEE {
            entity.make_stuck_in_block(state, Vec3::new(f64::from(0.8f32), 0.75, f64::from(0.8f32)));
            if state.get(props::AGE_3) != 0 {
                let movement = if entity.is_client_authoritative() { entity.get_known_movement(level) } else { entity.base().old_position().subtract(entity.base().position()) };
                if movement.horizontal_distance_sqr() > 0.0 {
                    let (xs, zs) = (movement.x.abs(), movement.z.abs());
                    if xs >= f64::from(0.003f32) || zs >= f64::from(0.003f32) {
                        let source = level.damage_sources().sweet_berry_bush();
                        entity.hurt_server(level, &source, 1.0);
                    }
                }
            }
        }
    }
}

register_block_class!("SweetBerryBushBlock", |_| Box::new(SweetBerryBushBlock));
