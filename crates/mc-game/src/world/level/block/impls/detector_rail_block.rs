//! `net.minecraft.world.level.block.DetectorRailBlock` (extends `BaseRailBlock`).
use super::base_rail_block::rail_behaviour;
use super::prelude::*;

pub struct DetectorRailBlock;

rail_behaviour!(DetectorRailBlock, props::RAIL_SHAPE_STRAIGHT);

register_block_class!("DetectorRailBlock", |_| Box::new(DetectorRailBlock));
