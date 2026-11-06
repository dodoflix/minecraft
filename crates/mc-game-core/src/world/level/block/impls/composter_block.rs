//! `net.minecraft.world.level.block.ComposterBlock`. The hopper-facing `WorldlyContainerHolder` containers are not
//! implemented here.
use super::prelude::*;
use crate::server::level::ServerLevel;
use crate::stats::Stat;
use crate::world::entity::item::ItemEntity;
use crate::world::entity::{Entity, EntityId, ServerPlayer};
use crate::world::item::{InteractionHand, InteractionResult, ItemStack, ItemStackExt};
use crate::world::level::gameevent::GameEventContext;
use crate::world::level::storage::loot::context::{LootContext, LootParams};
use crate::world::level::storage::loot::parameters::LootContextParamSets;
use crate::world::level::storage::loot::providers::number::ResolvableInt as GameResolvableInt;
use mc_core::phys::{BlockHitResult, Vec3};
use mc_data::{GameEvent, Item, SoundEvent};
use mc_protocol::components::{COMPOSTABLE, Compostable, ResolvableInt};
use mc_protocol::packets::play::level::SoundSource;

pub struct ComposterBlock;

const READY: i32 = 8;
const MAX_LEVEL: i32 = 7;

/// `compostable.layers().get(lootContext, 0)` with the `BLOCK_INTERACT` context.
fn layers_to_add(
    level: &ServerLevel,
    state: BlockState,
    pos: BlockPos,
    source: Option<&dyn Entity>,
    compostable: &Compostable,
) -> i32 {
    match &compostable.layers {
        ResolvableInt::Constant(v) => *v,
        ResolvableInt::Reference(key) => {
            let params = LootParams::builder(level)
                .block_state(state)
                .origin(Vec3::at_center_of(&pos))
                .interacting_entity(source)
                .create(&LootContextParamSets::BLOCK_INTERACT);
            let mut context = LootContext::builder(&params).create(None);
            GameResolvableInt::Reference(key.clone()).get(&mut context, 0)
        }
    }
}

/// `addLayer(sourceEntity, state, level, pos, compostable)`.
fn add_layer(
    source: Option<&dyn Entity>,
    state: BlockState,
    level: &mut ServerLevel,
    pos: BlockPos,
    compostable: &Compostable,
) -> BlockState {
    let fill_level = state.get(props::LEVEL_COMPOSTER);
    let layers = layers_to_add(level, state, pos, source, compostable);
    if layers > 0 {
        let new_level = (fill_level + layers).clamp(0, MAX_LEVEL);
        let new_state = state.set(props::LEVEL_COMPOSTER, new_level);
        level.set_block_and_update(pos, new_state);
        level.game_event_at_block(
            GameEvent::BLOCK_CHANGE,
            pos,
            &GameEventContext::of(source.map(|e| e.id()), Some(new_state)),
        );
        if new_level == 7 {
            level.schedule_tick_block(pos, state.block(), 20);
        }
        new_state
    } else {
        state
    }
}

/// `empty(sourceEntity, state, level, pos)`.
fn empty(
    source: Option<EntityId>,
    state: BlockState,
    level: &mut ServerLevel,
    pos: BlockPos,
) -> BlockState {
    let new_state = state.set(props::LEVEL_COMPOSTER, 0);
    level.set_block_and_update(pos, new_state);
    level.game_event_at_block(
        GameEvent::BLOCK_CHANGE,
        pos,
        &GameEventContext::of(source, Some(new_state)),
    );
    new_state
}

/// `extractProduce(sourceEntity, state, level, pos)`.
pub fn extract_produce(
    source: Option<EntityId>,
    state: BlockState,
    level: &mut ServerLevel,
    pos: BlockPos,
) -> BlockState {
    let item_pos = Vec3::at_lower_corner_with_offset(&pos, 0.5, 1.01, 0.5)
        .offset_random_xz(LevelAccessor::random(level), 0.7);
    let mut entity = ItemEntity::with_stack(
        level,
        item_pos.x,
        item_pos.y,
        item_pos.z,
        ItemStack::new(Item::BONE_MEAL),
    );
    entity.set_default_pick_up_delay();
    level.add_fresh_entity(Box::new(entity));
    let empty_state = empty(source, state, level, pos);
    level.play_sound(
        None,
        pos,
        SoundEvent::BLOCK_COMPOSTER_EMPTY,
        SoundSource::Blocks,
        1.0,
        1.0,
    );
    empty_state
}

/// `insertItem(sourceEntity, state, level, itemStack, pos)` (dispenser/hopper insertion).
pub fn insert_item(
    source: Option<&dyn Entity>,
    state: BlockState,
    level: &mut ServerLevel,
    stack: &mut ItemStack,
    pos: BlockPos,
) -> BlockState {
    let fill_level = state.get(props::LEVEL_COMPOSTER);
    match stack.get(&COMPOSTABLE).cloned() {
        Some(compostable) if fill_level < 7 => {
            let new_state = add_layer(source, state, level, pos, &compostable);
            stack.shrink(1);
            new_state
        }
        _ => state,
    }
}

impl BlockBehaviour for ComposterBlock {
    fn get_interaction_shape(
        &self,
        _state: BlockState,
        _level: &dyn BlockGetter,
        _pos: BlockPos,
    ) -> mc_core::phys::shapes::VoxelShape {
        mc_core::phys::shapes::Shapes::block()
    }

    fn on_place(
        &self,
        state: BlockState,
        level: &mut ServerLevel,
        pos: BlockPos,
        _old_state: BlockState,
        _moved_by_piston: bool,
    ) {
        if state.get(props::LEVEL_COMPOSTER) == 7 {
            level.schedule_tick_block(pos, state.block(), 20);
        }
    }

    fn use_item_on(
        &self,
        stack: &ItemStack,
        state: BlockState,
        level: &mut ServerLevel,
        pos: BlockPos,
        player: &mut ServerPlayer,
        hand: InteractionHand,
        _hit: &BlockHitResult,
    ) -> InteractionResult {
        let fill_level = state.get(props::LEVEL_COMPOSTER);
        match stack.get(&COMPOSTABLE).cloned() {
            Some(compostable) if fill_level < 8 => {
                if fill_level < 7 {
                    let new_state = add_layer(Some(&*player), state, level, pos, &compostable);
                    level.level_event(None, 1500, pos, i32::from(state != new_state));
                    player.award_stat_once(&*level, Stat::Used(stack.item()));
                    let infinite = player.player.abilities.instabuild;
                    player
                        .player
                        .inventory
                        .equipment_get_mut(hand.as_equipment_slot())
                        .consume(1, infinite);
                }
                InteractionResult::SUCCESS
            }
            _ => InteractionResult::TRY_WITH_EMPTY_HAND,
        }
    }

    fn use_without_item(
        &self,
        state: BlockState,
        level: &mut ServerLevel,
        pos: BlockPos,
        player: &mut ServerPlayer,
        _hit: &BlockHitResult,
    ) -> InteractionResult {
        if state.get(props::LEVEL_COMPOSTER) == READY {
            extract_produce(Some(player.id()), state, level, pos);
            InteractionResult::SUCCESS
        } else {
            InteractionResult::PASS
        }
    }

    fn tick(&self, state: BlockState, level: &mut ServerLevel, pos: BlockPos) {
        if state.get(props::LEVEL_COMPOSTER) == 7 {
            level.set_block_and_update(pos, state.cycle(props::LEVEL_COMPOSTER));
            level.play_sound(
                None,
                pos,
                SoundEvent::BLOCK_COMPOSTER_READY,
                SoundSource::Blocks,
                1.0,
                1.0,
            );
        }
    }

    fn get_analog_output_signal(
        &self,
        state: BlockState,
        _level: &ServerLevel,
        _pos: BlockPos,
        _direction: Direction,
    ) -> i32 {
        state.get(props::LEVEL_COMPOSTER)
    }
}

register_block_class!("ComposterBlock", |_| Box::new(ComposterBlock));
