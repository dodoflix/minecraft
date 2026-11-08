//! `net.minecraft.world.level.block.ComposterBlock`, including its `WorldlyContainerHolder` containers.
use super::prelude::*;
use crate::server::level::ServerLevel;
use crate::stats::Stat;
use crate::world::container::{Container, WorldlyContainer, empty_stack, helper};
use crate::world::entity::item::ItemEntity;
use crate::world::entity::{Entity, EntityId, ServerPlayer};
use crate::world::inventory::MenuPlayer;
use crate::world::item::{InteractionHand, InteractionResult, ItemStack, ItemStackExt};
use crate::world::level::gameevent::GameEventContext;
use crate::world::level::storage::loot::context::{LootContext, LootParams};
use crate::world::level::storage::loot::parameters::LootContextParamSets;
use crate::world::level::storage::loot::providers::number::ResolvableInt as GameResolvableInt;
use mc_core::phys::{BlockHitResult, Vec3};
use mc_data::{GameEvent, Item, SoundEvent};
use mc_protocol::components::{COMPOSTABLE, Compostable, ResolvableInt};
use mc_protocol::packets::play::level::SoundSource;
use std::any::Any;

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

/// The `EmptyContainer` / `InputContainer` / `OutputContainer` of `ComposterBlock.getContainer`. Vanilla applies the
/// fill / emptying inside `setChanged` (the container holds the level); here `setChanged` records it and
/// `finish_worldly_container` applies it right after the transfer that used the container.
struct ComposterContainer {
    kind: ComposterContainerKind,
    stacks: Vec<ItemStack>,
    /// `changed`: the container already took part in a change and refuses further transfers.
    changed: bool,
    /// `InputContainer.setChanged`: the compostable item taken out of the slot, added as a layer afterwards.
    pending_layer: Option<Compostable>,
    /// `OutputContainer.setChanged`: the composter is emptied afterwards.
    pending_empty: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ComposterContainerKind {
    Empty,
    Input,
    Output,
}

impl ComposterContainer {
    fn new(kind: ComposterContainerKind, contents: ItemStack) -> Self {
        let stacks = match kind {
            ComposterContainerKind::Empty => Vec::new(),
            ComposterContainerKind::Input => vec![ItemStack::empty()],
            ComposterContainerKind::Output => vec![contents],
        };
        Self {
            kind,
            stacks,
            changed: false,
            pending_layer: None,
            pending_empty: false,
        }
    }
}

impl Container for ComposterContainer {
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
    fn get_container_size(&self) -> usize {
        self.stacks.len()
    }
    fn is_empty(&self) -> bool {
        self.stacks.iter().all(ItemStack::is_empty)
    }
    fn get_item(&self, slot: usize) -> &ItemStack {
        self.stacks.get(slot).unwrap_or_else(|| empty_stack())
    }
    fn get_item_mut(&mut self, slot: usize) -> Option<&mut ItemStack> {
        self.stacks.get_mut(slot)
    }
    fn remove_item(&mut self, slot: usize, count: i32) -> ItemStack {
        let result = helper::remove_item(&mut self.stacks, slot, count);
        if !result.is_empty() {
            self.set_changed();
        }
        result
    }
    fn remove_item_no_update(&mut self, slot: usize) -> ItemStack {
        self.stacks
            .get_mut(slot)
            .map_or(ItemStack::empty(), std::mem::take)
    }
    fn set_item(&mut self, slot: usize, mut stack: ItemStack) {
        let max = self.get_max_stack_size_for(&stack);
        stack.limit_size(max);
        if let Some(s) = self.stacks.get_mut(slot) {
            *s = stack;
        }
        self.set_changed();
    }
    fn clear_content(&mut self) {
        self.stacks.fill(ItemStack::empty());
        self.set_changed();
    }
    fn set_changed(&mut self) {
        match self.kind {
            ComposterContainerKind::Empty => {}
            ComposterContainerKind::Input => {
                let contents = self.get_item(0);
                if let Some(compostable) = contents.get(&COMPOSTABLE).cloned()
                    && !contents.is_empty()
                {
                    self.changed = true;
                    self.pending_layer = Some(compostable);
                    self.remove_item_no_update(0);
                }
            }
            ComposterContainerKind::Output => {
                self.pending_empty = true;
                self.changed = true;
            }
        }
    }
    fn still_valid(&self, _player: &dyn MenuPlayer) -> bool {
        true
    }
    fn get_max_stack_size(&self) -> i32 {
        if self.kind == ComposterContainerKind::Empty {
            99
        } else {
            1
        }
    }
    fn as_worldly(&self) -> Option<&dyn WorldlyContainer> {
        Some(self)
    }
}

impl WorldlyContainer for ComposterContainer {
    fn get_slots_for_face(&self, direction: Direction) -> &'static [usize] {
        match (self.kind, direction) {
            (ComposterContainerKind::Input, Direction::Up)
            | (ComposterContainerKind::Output, Direction::Down) => &[0],
            _ => &[],
        }
    }
    fn can_place_item_through_face(
        &self,
        _level: &ServerLevel,
        _slot: usize,
        stack: &ItemStack,
        direction: Option<Direction>,
    ) -> bool {
        self.kind == ComposterContainerKind::Input
            && !self.changed
            && direction == Some(Direction::Up)
            && stack.get(&COMPOSTABLE).is_some()
    }
    fn can_take_item_through_face(
        &self,
        _slot: usize,
        stack: &ItemStack,
        direction: Direction,
    ) -> bool {
        self.kind == ComposterContainerKind::Output
            && !self.changed
            && direction == Direction::Down
            && stack.is(Item::BONE_MEAL)
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

    fn is_worldly_container_holder(&self) -> bool {
        true
    }

    fn get_worldly_container(
        &self,
        state: BlockState,
        _level: &mut ServerLevel,
        _pos: BlockPos,
    ) -> Option<Box<dyn Container>> {
        let content_level = state.get(props::LEVEL_COMPOSTER);
        let (kind, contents) = if content_level == READY {
            (
                ComposterContainerKind::Output,
                ItemStack::new(Item::BONE_MEAL),
            )
        } else if content_level < MAX_LEVEL {
            (ComposterContainerKind::Input, ItemStack::empty())
        } else {
            (ComposterContainerKind::Empty, ItemStack::empty())
        };
        Some(Box::new(ComposterContainer::new(kind, contents)))
    }

    fn finish_worldly_container(
        &self,
        state: BlockState,
        level: &mut ServerLevel,
        pos: BlockPos,
        container: Box<dyn Container>,
    ) {
        let Some(container) = container.as_any().downcast_ref::<ComposterContainer>() else {
            return;
        };
        if let Some(compostable) = &container.pending_layer {
            let new_state = add_layer(None, state, level, pos, compostable);
            level.level_event(None, 1500, pos, i32::from(new_state != state));
        }
        if container.pending_empty {
            empty(None, state, level, pos);
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
