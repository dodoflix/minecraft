//! `DecoratedPotBlockEntity` (single-item container with sherds, loot table and the wobble animation state).

use linkme::distributed_slice;
use mc_core::pos::BlockPos;
use mc_data::block::BlockState;
use mc_data::{BlockEntityType, Item};
use mc_game_core::block_entity_base;
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::block_entity::randomizable_container::{self, RandomizableContainerData};
use mc_game_core::world::block_entity::{
    BLOCK_ENTITY_TYPES, BlockEntity, BlockEntityData, BlockEntityRegistration,
};
use mc_game_core::world::container::Container;
use mc_game_core::world::item::ItemStack;
use mc_nbt::CompoundTag;
use mc_protocol::components::{
    CONTAINER, ItemContainerContents, POT_DECORATIONS, PotDecorations, map::DataComponentMapBuilder,
};
use mc_protocol::packets::play::level::ClientboundBlockEntityDataPacket;

/// `DecoratedPotBlockEntity.WobbleStyle` (`ordinal` is the block event parameter).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum WobbleStyle {
    Positive,
    Negative,
}

impl WobbleStyle {
    pub fn duration(self) -> i32 {
        match self {
            WobbleStyle::Positive => 7,
            WobbleStyle::Negative => 10,
        }
    }
}

pub struct DecoratedPotBlockEntity {
    base: BlockEntityData,
    pub wobble_started_at_tick: i64,
    pub last_wobble_style: Option<WobbleStyle>,
    decorations: PotDecorations,
    item: ItemStack,
    loot: RandomizableContainerData,
}

/// `createDecoratedPotInstance(decorations)`.
pub fn create_decorated_pot_instance(decorations: &PotDecorations) -> ItemStack {
    let mut stack = ItemStack::new(Item::DECORATED_POT);
    stack.set(POT_DECORATIONS, decorations.clone());
    stack
}

impl DecoratedPotBlockEntity {
    pub fn new(pos: BlockPos, state: BlockState) -> Self {
        Self {
            base: BlockEntityData::new(BlockEntityType::DECORATED_POT, pos, state),
            wobble_started_at_tick: 0,
            last_wobble_style: None,
            decorations: PotDecorations::default(),
            item: ItemStack::empty(),
            loot: RandomizableContainerData::default(),
        }
    }

    pub fn get_decorations(&self) -> &PotDecorations {
        &self.decorations
    }

    /// `getTheItem()` (without the loot unpack, see [`DecoratedPotBlockEntity::unpack`]).
    pub fn the_item(&self) -> &ItemStack {
        &self.item
    }

    pub fn the_item_mut(&mut self) -> &mut ItemStack {
        &mut self.item
    }

    pub fn set_the_item(&mut self, stack: ItemStack) {
        self.item = stack;
    }

    pub fn unpack(
        &mut self,
        level: &ServerLevel,
        player: Option<&dyn mc_game_core::world::entity::Entity>,
    ) {
        let mut loot = std::mem::take(&mut self.loot);
        let pos = self.base.pos;
        randomizable_container::unpack_loot_table(level, pos, &mut loot, self, player);
        self.loot = loot;
    }

    /// `wobble(style)`.
    pub fn wobble(&self, level: &mut ServerLevel, style: WobbleStyle) {
        level.block_event(
            self.base.pos,
            self.base.block_state.block(),
            1,
            style as i32,
        );
    }
}

impl Container for DecoratedPotBlockEntity {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
    fn get_container_size(&self) -> usize {
        1
    }
    fn is_empty(&self) -> bool {
        self.item.is_empty()
    }
    fn get_item(&self, slot: usize) -> &ItemStack {
        if slot == 0 {
            &self.item
        } else {
            mc_game_core::world::container::empty_stack()
        }
    }
    fn get_item_mut(&mut self, slot: usize) -> Option<&mut ItemStack> {
        if slot == 0 {
            Some(&mut self.item)
        } else {
            None
        }
    }
    fn remove_item(&mut self, slot: usize, count: i32) -> ItemStack {
        if slot != 0 {
            return ItemStack::empty();
        }
        let result = self.item.split(count);
        if self.item.is_empty() {
            self.item = ItemStack::empty();
        }
        result
    }
    fn remove_item_no_update(&mut self, slot: usize) -> ItemStack {
        self.remove_item(slot, self.get_max_stack_size())
    }
    fn set_item(&mut self, slot: usize, stack: ItemStack) {
        if slot == 0 {
            self.item = stack;
        }
    }
    fn clear_content(&mut self) {
        let n = self.get_max_stack_size();
        self.remove_item(0, n);
    }
    fn set_changed(&mut self) {
        self.base.changed = true;
    }
    fn still_valid(&self, player: &dyn mc_game_core::world::inventory::MenuPlayer) -> bool {
        mc_game_core::world::container::still_valid_block_entity(self.base.pos, player)
    }
}

impl BlockEntity for DecoratedPotBlockEntity {
    block_entity_base!();

    fn load_additional(&mut self, tag: &CompoundTag) {
        self.decorations =
            mc_game_core::world::block_entity::read_codec(tag, "sherds", &PotDecorations::codec())
                .unwrap_or_default();
        if !self.loot.try_load_loot_table(tag) {
            self.item = tag
                .get("item")
                .and_then(|t| ItemStack::from_tag(t).into_result().ok())
                .unwrap_or_else(ItemStack::empty);
        } else {
            self.item = ItemStack::empty();
        }
    }

    fn save_additional(&self, tag: &mut CompoundTag) {
        if self.decorations != PotDecorations::default()
            && let Ok(t) =
                mc_dfu::encode_nbt(&PotDecorations::codec(), &self.decorations).into_result()
        {
            tag.put("sherds", t);
        }
        if !self.loot.try_save_loot_table(tag)
            && !self.item.is_empty()
            && let Ok(t) = self.item.to_tag().into_result()
        {
            tag.put("item", t);
        }
    }

    fn get_update_tag(&self) -> CompoundTag {
        let mut tag = CompoundTag::new();
        self.save_additional(&mut tag);
        tag
    }

    fn get_update_packet(&self) -> Option<ClientboundBlockEntityDataPacket> {
        Some(ClientboundBlockEntityDataPacket {
            pos: self.base.pos,
            r#type: self.base.ty,
            tag: self.get_update_tag(),
        })
    }

    fn apply_components_from_item_stack(&mut self, stack: &ItemStack) {
        self.decorations = stack.get(&POT_DECORATIONS).cloned().unwrap_or_default();
        self.item = stack
            .get(&CONTAINER)
            .map(|c| {
                let mut one = [ItemStack::empty()];
                c.copy_into(&mut one);
                let [first] = one;
                first
            })
            .unwrap_or_default();
    }

    fn collect_implicit_components(&self, builder: &mut DataComponentMapBuilder) {
        *builder = std::mem::take(builder)
            .set(POT_DECORATIONS, self.decorations.clone())
            .set(
                CONTAINER,
                ItemContainerContents::from_item_stacks(std::slice::from_ref(&self.item)),
            );
    }

    fn trigger_event(&mut self, level: &mut ServerLevel, event: i32, data: i32) -> bool {
        if event == 1 && (0..2).contains(&data) {
            self.wobble_started_at_tick = level.game_time();
            self.last_wobble_style = Some(if data == 0 {
                WobbleStyle::Positive
            } else {
                WobbleStyle::Negative
            });
            true
        } else {
            false
        }
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
        mc_game_core::world::block_entity::set_changed(level, self.base.pos, self.base.block_state);
    }

    fn unpack_loot_table(
        &mut self,
        level: &ServerLevel,
        player: Option<&dyn mc_game_core::world::entity::Entity>,
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
static DECORATED_POT: BlockEntityRegistration = BlockEntityRegistration {
    ty: BlockEntityType::DECORATED_POT,
    create: |pos, state| Box::new(DecoratedPotBlockEntity::new(pos, state)),
};
