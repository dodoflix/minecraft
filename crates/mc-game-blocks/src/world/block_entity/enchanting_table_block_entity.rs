//! `EnchantingTableBlockEntity`: only the custom name is state (`Nameable`); the book animation (`bookAnimationTick`) is
//! client side.
//!
//! Not implemented: `removeComponentsFromTag` (creative pick block with data keeps `CustomName`).

use linkme::distributed_slice;
use mc_core::pos::BlockPos;
use mc_data::BlockEntityType;
use mc_data::block::BlockState;
use mc_game_core::block_entity_base;
use mc_game_core::world::block_entity::ComponentReader;
use mc_game_core::world::block_entity::{
    BLOCK_ENTITY_TYPES, BlockEntity, BlockEntityData, BlockEntityRegistration, read_codec,
};
use mc_nbt::CompoundTag;
use mc_protocol::components::{CUSTOM_NAME, component_codec, map::DataComponentMapBuilder};
use mc_text::Component;

pub struct EnchantingTableBlockEntity {
    base: BlockEntityData,
    name: Option<Component>,
}

impl EnchantingTableBlockEntity {
    pub fn new(pos: BlockPos, state: BlockState) -> Self {
        Self {
            base: BlockEntityData::new(BlockEntityType::ENCHANTING_TABLE, pos, state),
            name: None,
        }
    }

    /// `getName()` (= `getDisplayName()`): the custom name, else `container.enchant`.
    pub fn get_name(&self) -> Component {
        self.name
            .clone()
            .unwrap_or_else(|| Component::translatable("container.enchant", Vec::new()))
    }
}

impl BlockEntity for EnchantingTableBlockEntity {
    block_entity_base!();

    fn save_additional(&self, tag: &mut CompoundTag) {
        if let Some(name) = &self.name
            && let Ok(encoded) = mc_dfu::encode_nbt(&component_codec(), name).into_result()
        {
            tag.put("CustomName", encoded);
        }
    }

    fn load_additional(&mut self, tag: &CompoundTag) {
        self.name = read_codec(tag, "CustomName", &component_codec());
    }

    fn get_custom_name(&self) -> Option<Component> {
        self.name.clone()
    }

    /// `removeComponentsFromTag(output)`.
    fn remove_components_from_tag(&self, tag: &mut CompoundTag) {
        tag.remove("CustomName");
    }

    fn apply_implicit_components(&mut self, components: &ComponentReader) {
        self.name = components.get(&CUSTOM_NAME).cloned();
    }

    fn collect_implicit_components(&self, builder: &mut DataComponentMapBuilder) {
        *builder = std::mem::take(builder).set_opt(CUSTOM_NAME, self.name.clone());
    }
}

#[distributed_slice(BLOCK_ENTITY_TYPES)]
static ENCHANTING_TABLE: BlockEntityRegistration = BlockEntityRegistration {
    ty: BlockEntityType::ENCHANTING_TABLE,
    create: |pos, state| Box::new(EnchantingTableBlockEntity::new(pos, state)),
};
