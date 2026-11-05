//! `net.minecraft.world.level.block.GlowLichenBlock` (extends `MultifaceSpreadeableBlock`; the shape logic is `MultifaceBlock`'s).
use super::multiface_spreader::MultifaceSpreader;
use super::prelude::*;
use std::sync::LazyLock;

static SPREADER: LazyLock<MultifaceSpreader> = LazyLock::new(|| MultifaceSpreader::new(Block::GLOW_LICHEN));

/// `GlowLichenBlock.getSpreader()`.
pub fn spreader() -> &'static MultifaceSpreader {
    &SPREADER
}
