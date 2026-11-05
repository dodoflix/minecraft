//! `net.minecraft.world.level.block.RailBlock` (extends `BaseRailBlock`).
use super::base_rail_block::rail_behaviour;
use super::prelude::*;

pub struct RailBlock;

rail_behaviour!(RailBlock, props::RAIL_SHAPE);

register_block_class!("RailBlock", |_| Box::new(RailBlock));
