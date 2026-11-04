//! `net.minecraft.world.level.block.AttachedStemBlock` (extends `VegetationBlock`).
use super::prelude::*;
use super::vegetation_block::{self, MayPlaceOn};

/// The fruit block the stem attaches to, the (grown) stem block it reverts to, and its support tag.
pub struct AttachedStemBlock {
    fruit: Block,
    stem: Block,
    support_blocks: &'static str,
}

impl MayPlaceOn for AttachedStemBlock {
    fn may_place_on(&self, state: BlockState, _level: &dyn BlockGetter, _pos: BlockPos) -> bool {
        state.is_tag(self.support_blocks)
    }
}

impl BlockBehaviour for AttachedStemBlock {
    fn can_survive(&self, _state: BlockState, level: &dyn LevelReader, pos: BlockPos) -> bool {
        vegetation_block::can_survive(self, level, pos)
    }
    update_shape!(|self, state, level, pos, direction, neighbour_pos, neighbour_state| {
        if !neighbour_state.is_block(self.fruit) && direction == state.get(props::HORIZONTAL_FACING) {
            return self.stem.default_state().try_set(props::AGE_7, 7).unwrap_or(self.stem.default_state());
        }
        vegetation_block::update_shape(state, &*level, pos)
    });
    fn rotate(&self, state: BlockState, rotation: Rotation) -> BlockState {
        helpers::rotate_facing(state, props::HORIZONTAL_FACING, rotation)
    }
    fn mirror(&self, state: BlockState, mirror: Mirror) -> BlockState {
        helpers::mirror_facing(state, props::HORIZONTAL_FACING, mirror)
    }
}

register_block_class!("AttachedStemBlock", |block| Box::new(match block.data().name {
    "minecraft:attached_pumpkin_stem" => AttachedStemBlock { fruit: Block::PUMPKIN, stem: Block::PUMPKIN_STEM, support_blocks: "supports_pumpkin_stem" },
    "minecraft:attached_melon_stem" => AttachedStemBlock { fruit: Block::MELON, stem: Block::MELON_STEM, support_blocks: "supports_melon_stem" },
    name => panic!("unexpected AttachedStemBlock {name}"),
}));
