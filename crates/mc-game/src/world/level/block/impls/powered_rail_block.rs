//! `net.minecraft.world.level.block.PoweredRailBlock` (extends `BaseRailBlock`).
use super::base_rail_block::rail_behaviour;
use super::prelude::*;

pub struct PoweredRailBlock;

rail_behaviour!(PoweredRailBlock, props::RAIL_SHAPE_STRAIGHT);

register_block_class!("PoweredRailBlock", |_| Box::new(PoweredRailBlock));
