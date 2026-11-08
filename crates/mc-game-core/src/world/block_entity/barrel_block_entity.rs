//! `BarrelBlockEntity`.

use super::base_container::BaseContainer;
use super::container_openers::{
    ContainerOpenersCounter, OpenersHost, player_has_chest_menu_open_on,
};
use super::randomizable_container::{self, RandomizableContainerData};
use super::{
    BLOCK_ENTITY_TYPES, BlockEntity, BlockEntityData, BlockEntityRegistration, ContainerUser,
};
use crate::server::level::ServerLevel;
use crate::world::container::{Container, helper};
use crate::world::entity::Entity;
use crate::world::item::ItemStack;
use crate::world::level::LevelAccessor;
use linkme::distributed_slice;
use mc_core::pos::BlockPos;
use mc_data::block::BlockState;
use mc_data::{BlockEntityType, SoundEvent, props};
use mc_nbt::CompoundTag;
use mc_protocol::components::map::DataComponentMapBuilder;
use mc_protocol::packets::play::level::SoundSource;
use mc_text::Component;

pub struct BarrelBlockEntity {
    base: BlockEntityData,
    pub container: BaseContainer,
    items: Vec<ItemStack>,
    loot: RandomizableContainerData,
    openers: ContainerOpenersCounter,
}

struct BarrelHost;

impl BarrelHost {
    /// `BarrelBlockEntity.playSound(state, event)`.
    fn play_sound(level: &mut ServerLevel, pos: BlockPos, state: BlockState, event: SoundEvent) {
        let direction = state.get(props::FACING);
        let x = f64::from(pos.x()) + 0.5 + f64::from(direction.get_step_x()) / 2.0;
        let y = f64::from(pos.y()) + 0.5 + f64::from(direction.get_step_y()) / 2.0;
        let z = f64::from(pos.z()) + 0.5 + f64::from(direction.get_step_z()) / 2.0;
        let pitch = LevelAccessor::random(level).next_float() * 0.1 + 0.9;
        level.play_sound_at(None, x, y, z, event, SoundSource::Blocks, 0.5, pitch);
    }

    /// `updateBlockState(state, isOpen)` (deferred until the entity is back in its chunk, see `deferred`).
    fn update_block_state(pos: BlockPos, state: BlockState, is_open: bool) {
        super::deferred::push(move |level| {
            level.set_block_and_update(pos, state.set(props::OPEN, is_open));
        });
    }
}

impl OpenersHost for BarrelHost {
    fn on_open(&self, level: &mut ServerLevel, pos: BlockPos, state: BlockState) {
        Self::play_sound(level, pos, state, SoundEvent::BLOCK_BARREL_OPEN);
        Self::update_block_state(pos, state, true);
    }
    fn on_close(&self, level: &mut ServerLevel, pos: BlockPos, state: BlockState) {
        Self::play_sound(level, pos, state, SoundEvent::BLOCK_BARREL_CLOSE);
        Self::update_block_state(pos, state, false);
    }
    fn opener_count_changed(
        &self,
        _level: &mut ServerLevel,
        _pos: BlockPos,
        _state: BlockState,
        _previous: i32,
        _current: i32,
    ) {
    }
    fn is_own_container(&self, player: &dyn Entity, pos: BlockPos) -> bool {
        player_has_chest_menu_open_on(player, pos)
    }
}

impl BarrelBlockEntity {
    pub const CONTAINER_SIZE: usize = 27;

    pub fn new(pos: BlockPos, state: BlockState) -> Self {
        Self {
            base: BlockEntityData::new(BlockEntityType::BARREL, pos, state),
            container: BaseContainer::default(),
            items: vec![ItemStack::empty(); Self::CONTAINER_SIZE],
            loot: RandomizableContainerData::default(),
            openers: ContainerOpenersCounter::default(),
        }
    }

    pub fn default_name() -> Component {
        Component::translatable("container.barrel", vec![])
    }

    pub fn get_display_name(&self) -> Component {
        self.container.get_name(Self::default_name())
    }

    pub fn can_open(&self, player: &dyn crate::world::inventory::MenuPlayer) -> bool {
        (self.loot.loot_table.is_none() || !player.is_spectator())
            && self.container.can_open(player)
    }

    pub fn unpack(
        &mut self,
        level: &mut ServerLevel,
        player: Option<&mut crate::world::entity::ServerPlayer>,
    ) {
        let mut loot = std::mem::take(&mut self.loot);
        let pos = self.base.pos;
        randomizable_container::unpack_loot_table(level, pos, &mut loot, self, player);
        self.loot = loot;
    }

    pub fn recheck_open(&mut self, level: &mut ServerLevel) {
        if !self.base.removed {
            let (pos, state) = (self.base.pos, self.base.block_state);
            self.openers.recheck_openers(&BarrelHost, level, pos, state);
        }
    }
}

impl_items_container!(BarrelBlockEntity, items);

impl BlockEntity for BarrelBlockEntity {
    block_entity_base!();

    fn load_additional(&mut self, tag: &CompoundTag) {
        self.container.load(tag);
        self.items = vec![ItemStack::empty(); Self::CONTAINER_SIZE];
        if !self.loot.try_load_loot_table(tag) {
            helper::load_all_items(tag, &mut self.items);
        }
    }

    fn save_additional(&self, tag: &mut CompoundTag) {
        self.container.save(tag);
        if !self.loot.try_save_loot_table(tag)
            && let Some(list) = helper::save_all_items(&self.items, true)
        {
            tag.put("Items", mc_nbt::Tag::List(list));
        }
    }

    fn apply_components_from_item_stack(&mut self, stack: &ItemStack) {
        self.container
            .apply_implicit_components(stack, &mut self.items);
        if let Some(loot) = stack.get(&mc_protocol::components::CONTAINER_LOOT) {
            self.loot.set_loot_table(loot.loot_table.clone(), loot.seed);
        }
    }

    fn collect_implicit_components(&self, builder: &mut DataComponentMapBuilder) {
        *builder = self
            .container
            .collect_implicit_components(std::mem::take(builder), &self.items);
        if let Some(table) = &self.loot.loot_table {
            *builder = std::mem::take(builder).set(
                mc_protocol::components::CONTAINER_LOOT,
                mc_protocol::components::SeededContainerLoot {
                    loot_table: table.clone(),
                    seed: self.loot.loot_table_seed,
                },
            );
        }
    }

    fn get_custom_name(&self) -> Option<Component> {
        self.container.get_custom_name().cloned()
    }

    fn as_container(&self) -> Option<&dyn Container> {
        Some(self)
    }

    fn as_container_mut(&mut self) -> Option<&mut dyn Container> {
        Some(self)
    }

    fn container_pending(&self) -> bool {
        self.base.changed
    }

    fn container_accessed(&mut self, level: &mut ServerLevel) {
        self.base.changed = false;
        super::set_changed(level, self.base.pos, self.base.block_state);
    }

    fn start_open(&mut self, level: &mut ServerLevel, user: &ContainerUser) {
        if !self.base.removed && !user.spectator {
            let (pos, state) = (self.base.pos, self.base.block_state);
            self.openers.increment_openers(
                &BarrelHost,
                user.id,
                level,
                pos,
                state,
                user.interaction_range,
            );
        }
    }

    fn stop_open(&mut self, level: &mut ServerLevel, user: &ContainerUser) {
        if !self.base.removed && !user.spectator {
            let (pos, state) = (self.base.pos, self.base.block_state);
            self.openers
                .decrement_openers(&BarrelHost, user.id, level, pos, state);
        }
    }

    fn unpack_loot_table(
        &mut self,
        level: &mut ServerLevel,
        player: Option<&mut crate::world::entity::ServerPlayer>,
    ) {
        self.unpack(level, player);
    }

    fn has_loot_table(&self) -> bool {
        self.loot.loot_table.is_some()
    }

    fn set_loot_table(&mut self, table: mc_core::ident::Identifier, seed: i64) {
        self.loot.set_loot_table(table, seed);
    }
}

#[distributed_slice(BLOCK_ENTITY_TYPES)]
static BARREL: BlockEntityRegistration = BlockEntityRegistration {
    ty: BlockEntityType::BARREL,
    create: |pos, state| Box::new(BarrelBlockEntity::new(pos, state)),
};
