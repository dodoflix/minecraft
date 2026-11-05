//! `net.minecraft.world.level.block.MultifaceSpreadeableBlock` (glow lichen, sculk vein): `getSpreader()`.
use super::multiface_spreader::MultifaceSpreader;
use super::prelude::*;

/// `block instanceof MultifaceSpreadeableBlock`.
pub fn is_multiface_spreadeable(block: Block) -> bool {
    block.data().classes.contains(&"MultifaceSpreadeableBlock")
}

/// `((MultifaceSpreadeableBlock) block).getSpreader()`.
pub fn get_spreader(block: Block) -> Option<&'static MultifaceSpreader> {
    if block == Block::GLOW_LICHEN {
        Some(super::glow_lichen_block::spreader())
    } else if block == Block::SCULK_VEIN {
        Some(super::sculk_vein_block::vein_spreader())
    } else {
        None
    }
}
