//! `ChestBlockEntity` (chests, trapped chests, copper chests) and `TrappedChestBlockEntity`.

use super::base_container::BaseContainer;
use super::container_openers::{
    ChestLidController, ContainerOpenersCounter, OpenersHost, player_has_chest_menu_open_on,
};
use super::randomizable_container::{self, RandomizableContainerData};
use super::{
    BLOCK_ENTITY_TYPES, BlockEntity, BlockEntityData, BlockEntityRegistration, ContainerUser,
};
use crate::server::level::ServerLevel;
use crate::world::container::{Container, helper};
use crate::world::entity::{Entity, EntityId};
use crate::world::item::ItemStack;
use crate::world::level::block::BlockStateExt;
use crate::world::level::block::impls::chest_block;
use crate::world::level::{BlockGetter, LevelAccessor};
use linkme::distributed_slice;
use mc_core::direction::Direction;
use mc_core::pos::BlockPos;
use mc_data::BlockEntityType;
use mc_data::block::BlockState;
use mc_data::enums::ChestType;
use mc_data::props;
use mc_nbt::CompoundTag;
use mc_protocol::components::map::DataComponentMapBuilder;
use mc_protocol::packets::play::level::SoundSource;
use mc_text::Component;

pub struct ChestBlockEntity {
    base: BlockEntityData,
    pub container: BaseContainer,
    items: Vec<ItemStack>,
    loot: RandomizableContainerData,
    openers: ContainerOpenersCounter,
    lid: ChestLidController,
}

/// The anonymous `ContainerOpenersCounter` of `ChestBlockEntity` (+ `TrappedChestBlockEntity.signalOpenCount`).
struct ChestHost {
    trapped: bool,
}

impl OpenersHost for ChestHost {
    fn on_open(&self, level: &mut ServerLevel, pos: BlockPos, state: BlockState) {
        if let Some((open, _)) = chest_block::chest_sounds(state.block()) {
            play_sound(level, pos, state, open);
        }
    }
    fn on_close(&self, level: &mut ServerLevel, pos: BlockPos, state: BlockState) {
        if let Some((_, close)) = chest_block::chest_sounds(state.block()) {
            play_sound(level, pos, state, close);
        }
    }
    fn opener_count_changed(
        &self,
        level: &mut ServerLevel,
        pos: BlockPos,
        state: BlockState,
        previous: i32,
        current: i32,
    ) {
        // `ChestBlockEntity.signalOpenCount`
        level.block_event(pos, state.block(), 1, current);
        if self.trapped && previous != current {
            let orientation = level.initial_orientation(
                Some(state.get(props::HORIZONTAL_FACING).get_opposite()),
                Some(Direction::Up),
            );
            level.update_neighbors_at_with_orientation_impl(pos, state.block(), orientation);
            level.update_neighbors_at_with_orientation_impl(
                pos.below(),
                state.block(),
                orientation,
            );
        }
    }
    fn is_own_container(&self, player: &dyn Entity, pos: BlockPos) -> bool {
        player_has_chest_menu_open_on(player, pos)
    }
}

/// `ChestBlockEntity.playSound`.
fn play_sound(
    level: &mut ServerLevel,
    pos: BlockPos,
    state: BlockState,
    sound: mc_data::SoundEvent,
) {
    let ty = state.get(props::CHEST_TYPE);
    if ty != ChestType::Left {
        let mut x = f64::from(pos.x()) + 0.5;
        let y = f64::from(pos.y()) + 0.5;
        let mut z = f64::from(pos.z()) + 0.5;
        if ty == ChestType::Right {
            let direction = chest_block::get_connected_direction(state);
            x += f64::from(direction.get_step_x()) * 0.5;
            z += f64::from(direction.get_step_z()) * 0.5;
        }
        let pitch = LevelAccessor::random(level).next_float() * 0.1 + 0.9;
        level.play_sound_at(None, x, y, z, sound, SoundSource::Blocks, 0.5, pitch);
    }
}

impl ChestBlockEntity {
    pub const CONTAINER_SIZE: usize = 27;

    pub fn new(ty: BlockEntityType, pos: BlockPos, state: BlockState) -> Self {
        Self {
            base: BlockEntityData::new(ty, pos, state),
            container: BaseContainer::default(),
            items: vec![ItemStack::empty(); Self::CONTAINER_SIZE],
            loot: RandomizableContainerData::default(),
            openers: ContainerOpenersCounter::default(),
            lid: ChestLidController::default(),
        }
    }

    fn host(&self) -> ChestHost {
        ChestHost {
            trapped: self.base.ty == BlockEntityType::TRAPPED_CHEST,
        }
    }

    /// `getDefaultName()`.
    pub fn default_name() -> Component {
        Component::translatable("container.chest", vec![])
    }

    pub fn get_display_name(&self) -> Component {
        self.container.get_name(Self::default_name())
    }

    pub fn can_open(&self, player: &dyn crate::world::inventory::MenuPlayer) -> bool {
        (self.loot.loot_table.is_none() || !player.is_spectator())
            && self.container.can_open(player)
    }

    pub fn items(&self) -> &[ItemStack] {
        &self.items
    }

    /// `ChestBlockEntity.getOpenCount(level, pos)`.
    pub fn get_open_count(level: &dyn BlockGetter, pos: BlockPos) -> i32 {
        if !level.get_block_state(pos).has_block_entity() {
            return 0;
        }
        level
            .get_block_entity(pos)
            .and_then(|be| be.as_any().downcast_ref::<ChestBlockEntity>())
            .map_or(0, |c| c.openers.get_opener_count())
    }

    /// `ChestBlockEntity.swapContents(one, two)`.
    pub fn swap_contents(one: &mut ChestBlockEntity, two: &mut ChestBlockEntity) {
        std::mem::swap(&mut one.items, &mut two.items);
    }

    /// `getOpenNess(a)`.
    pub fn get_open_ness(&self, a: f32) -> f32 {
        self.lid.get_openness(a)
    }

    /// `RandomizableContainer.unpackLootTable(player)`.
    pub fn unpack(
        &mut self,
        level: &ServerLevel,
        player: Option<&dyn crate::world::entity::Entity>,
    ) {
        let mut loot = std::mem::take(&mut self.loot);
        let pos = self.base.pos;
        randomizable_container::unpack_loot_table(level, pos, &mut loot, self, player);
        self.loot = loot;
    }

    /// `recheckOpen()`.
    pub fn recheck_open(&mut self, level: &mut ServerLevel) {
        if !self.base.removed {
            let (pos, state) = (self.base.pos, self.base.block_state);
            let host = self.host();
            self.openers.recheck_openers(&host, level, pos, state);
        }
    }

    /// `getEntitiesWithContainerOpen()`.
    pub fn entities_with_container_open(&self, level: &ServerLevel) -> Vec<EntityId> {
        self.openers
            .get_entities_with_container_open(&self.host(), level, self.base.pos)
    }
}

impl_items_container!(ChestBlockEntity, items {
    fn start_open(&mut self, _user: EntityId) {}
    fn stop_open(&mut self, _user: EntityId) {}
});

impl BlockEntity for ChestBlockEntity {
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
        if !self.loot.try_save_loot_table(tag) {
            if let Some(list) = helper::save_all_items(&self.items, true) {
                tag.put("Items", mc_nbt::Tag::List(list));
            }
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

    fn trigger_event(&mut self, _level: &mut ServerLevel, b0: i32, b1: i32) -> bool {
        if b0 == 1 {
            self.lid.should_be_open(b1 > 0);
            true
        } else {
            false
        }
    }

    fn pre_remove_side_effects(
        &mut self,
        level: &mut ServerLevel,
        pos: BlockPos,
        _state: BlockState,
    ) {
        self.unpack(level, None);
        crate::world::containers::drop_contents_list(level, pos, &self.items);
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
            let host = self.host();
            self.openers.increment_openers(
                &host,
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
            let host = self.host();
            self.openers
                .decrement_openers(&host, user.id, level, pos, state);
        }
    }

    fn unpack_loot_table(
        &mut self,
        level: &ServerLevel,
        player: Option<&dyn crate::world::entity::Entity>,
    ) {
        self.unpack(level, player);
    }

    fn set_loot_table(&mut self, table: mc_core::ident::Identifier, seed: i64) {
        self.loot.set_loot_table(table, seed);
    }
}

#[distributed_slice(BLOCK_ENTITY_TYPES)]
static CHEST: BlockEntityRegistration = BlockEntityRegistration {
    ty: BlockEntityType::CHEST,
    create: |pos, state| Box::new(ChestBlockEntity::new(BlockEntityType::CHEST, pos, state)),
};

#[distributed_slice(BLOCK_ENTITY_TYPES)]
static TRAPPED_CHEST: BlockEntityRegistration = BlockEntityRegistration {
    ty: BlockEntityType::TRAPPED_CHEST,
    create: |pos, state| {
        Box::new(ChestBlockEntity::new(
            BlockEntityType::TRAPPED_CHEST,
            pos,
            state,
        ))
    },
};
