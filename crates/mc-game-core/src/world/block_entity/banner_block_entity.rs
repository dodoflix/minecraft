//! `BannerBlockEntity`: custom name, the pattern layers and the base colour of the banner block.

use super::{BLOCK_ENTITY_TYPES, BlockEntity, BlockEntityData, BlockEntityRegistration, read_codec};
use crate::world::item::ItemStack;
use linkme::distributed_slice;
use mc_core::pos::BlockPos;
use mc_data::BlockEntityType;
use mc_data::block::BlockState;
use mc_nbt::CompoundTag;
use mc_protocol::components::{BANNER_PATTERNS, BannerPatternLayers, CUSTOM_NAME, DataComponentMap, DyeColor, component_codec};
use mc_protocol::packets::play::level::ClientboundBlockEntityDataPacket;
use mc_text::Component;

/// `AbstractBannerBlock.getColor()`: the colour of the banner block named `<color>_banner` / `<color>_wall_banner`.
pub fn banner_color_of(state: BlockState) -> DyeColor {
    let name = state.block().data().name;
    let path = name.strip_prefix("minecraft:").unwrap_or(name);
    let colour = path.strip_suffix("_wall_banner").or_else(|| path.strip_suffix("_banner")).unwrap_or(path);
    DyeColor::from_name(colour).unwrap_or(DyeColor::White)
}

pub struct BannerBlockEntity {
    base: BlockEntityData,
    name: Option<Component>,
    base_color: DyeColor,
    patterns: BannerPatternLayers,
}

impl BannerBlockEntity {
    pub const MAX_PATTERNS: usize = 6;

    /// `new BannerBlockEntity(pos, state)`.
    pub fn new(pos: BlockPos, state: BlockState) -> Self {
        Self::with_color(pos, state, banner_color_of(state))
    }

    /// `new BannerBlockEntity(pos, state, color)`.
    pub fn with_color(pos: BlockPos, state: BlockState, color: DyeColor) -> Self {
        Self { base: BlockEntityData::new(BlockEntityType::BANNER, pos, state), name: None, base_color: color, patterns: BannerPatternLayers::default() }
    }

    /// `getName()`.
    pub fn get_name(&self) -> Component {
        self.name.clone().unwrap_or_else(|| Component::translatable("block.minecraft.banner", vec![]))
    }

    pub fn get_patterns(&self) -> &BannerPatternLayers {
        &self.patterns
    }

    pub fn get_base_color(&self) -> DyeColor {
        self.base_color
    }

    /// `getItem()`: the banner item carrying this block entity's components (pick block).
    pub fn get_item(&self) -> ItemStack {
        let mut stack = self.get_block_state().block().item().map_or_else(ItemStack::empty, ItemStack::new);
        if !stack.is_empty() {
            for (id, value) in BlockEntity::collect_components(self).iter() {
                stack.set_value(id, Some(value.clone()));
            }
        }
        stack
    }
}

impl BlockEntity for BannerBlockEntity {
    block_entity_base!();

    fn save_additional(&self, tag: &mut CompoundTag) {
        if self.patterns != BannerPatternLayers::default() {
            if let Ok(patterns) = mc_dfu::encode_nbt(&BannerPatternLayers::codec(), &self.patterns).into_result() {
                tag.put("patterns", patterns);
            }
        }
        if let Some(name) = &self.name
            && let Ok(encoded) = mc_dfu::encode_nbt(&component_codec(), name).into_result()
        {
            tag.put("CustomName", encoded);
        }
    }

    fn load_additional(&mut self, tag: &CompoundTag) {
        self.name = read_codec(tag, "CustomName", &component_codec());
        self.patterns = read_codec(tag, "patterns", &BannerPatternLayers::codec()).unwrap_or_default();
    }

    /// `saveWithoutMetadata`: the saved data plus the `components` map.
    fn get_update_tag(&self) -> CompoundTag {
        let mut tag = CompoundTag::new();
        self.save_additional(&mut tag);
        if let Ok(components) = mc_dfu::encode_nbt(&DataComponentMap::codec(), &self.base.components).into_result() {
            tag.put("components", components);
        }
        tag
    }

    fn get_update_packet(&self) -> Option<ClientboundBlockEntityDataPacket> {
        Some(ClientboundBlockEntityDataPacket { pos: self.get_block_pos(), r#type: self.get_type(), tag: self.get_update_tag() })
    }

    /// `collectImplicitComponents(components)`.
    fn collect_implicit_components(&self, builder: &mut mc_protocol::components::map::DataComponentMapBuilder) {
        let mut b = std::mem::take(builder).set(BANNER_PATTERNS, self.patterns.clone());
        if let Some(name) = &self.name {
            b = b.set(CUSTOM_NAME, name.clone());
        }
        *builder = b;
    }

    fn get_custom_name(&self) -> Option<Component> {
        self.name.clone()
    }

    /// `applyImplicitComponents`.
    fn apply_components_from_item_stack(&mut self, stack: &ItemStack) {
        self.patterns = stack.get(&BANNER_PATTERNS).cloned().unwrap_or_default();
        self.name = stack.get(&CUSTOM_NAME).cloned();
    }
}

#[distributed_slice(BLOCK_ENTITY_TYPES)]
static BANNER: BlockEntityRegistration = BlockEntityRegistration { ty: BlockEntityType::BANNER, create: |pos, state| Box::new(BannerBlockEntity::new(pos, state)) };
