//! `net.minecraft.world.level.block.FlowerBlock` (extends `VegetationBlock`, `SuspiciousEffectHolder`); the effect of
//! every flower is the constructor argument in `Blocks.java`.
use mc_game_core::register_block_class;
use mc_game_core::world::level::block::impls::prelude::*;
use mc_game_core::world::level::block::impls::vegetation_block::vegetation_behaviour_methods;
use mc_core::math::mth;
use mc_data::MobEffect;
use mc_protocol::components::{SuspiciousStewEffects, SuspiciousStewEffectsEntry};

pub struct FlowerBlock {
    effects: SuspiciousStewEffects,
}

/// `FlowerBlock.makeEffectList(effect, effectSeconds)`.
pub fn make_effect_list(effect: MobEffect, effect_seconds: f32) -> SuspiciousStewEffects {
    SuspiciousStewEffects { effects: vec![SuspiciousStewEffectsEntry { effect, duration: mth::floor_f32(effect_seconds * 20.0) }] }
}

/// The `new FlowerBlock(effect, seconds, p)` arguments of `Blocks.java`.
fn effect_of(name: &str) -> (MobEffect, f32) {
    match name {
        "minecraft:dandelion" | "minecraft:golden_dandelion" | "minecraft:blue_orchid" => (MobEffect::SATURATION, 0.35),
        "minecraft:torchflower" | "minecraft:poppy" => (MobEffect::NIGHT_VISION, 5.0),
        "minecraft:allium" => (MobEffect::FIRE_RESISTANCE, 3.0),
        "minecraft:azure_bluet" => (MobEffect::BLINDNESS, 11.0),
        "minecraft:red_tulip" | "minecraft:orange_tulip" | "minecraft:white_tulip" | "minecraft:pink_tulip" => (MobEffect::WEAKNESS, 7.0),
        "minecraft:oxeye_daisy" => (MobEffect::REGENERATION, 7.0),
        "minecraft:cornflower" => (MobEffect::JUMP_BOOST, 5.0),
        "minecraft:lily_of_the_valley" => (MobEffect::POISON, 11.0),
        "minecraft:wither_rose" => (MobEffect::WITHER, 7.0),
        name => panic!("unexpected FlowerBlock {name}"),
    }
}

/// `Bee.attractsBees(state)`.
pub fn attracts_bees(state: BlockState) -> bool {
    if state.is_tag("bee_attractive") {
        if state.try_get(props::WATERLOGGED).unwrap_or(false) {
            false
        } else if state.is_block(Block::SUNFLOWER) {
            state.get(props::DOUBLE_BLOCK_HALF) == mc_data::enums::DoubleBlockHalf::Upper
        } else {
            true
        }
    } else {
        false
    }
}

impl FlowerBlock {
    pub fn new(block: Block) -> FlowerBlock {
        let (effect, seconds) = effect_of(block.data().name);
        FlowerBlock { effects: make_effect_list(effect, seconds) }
    }
}

impl mc_game_core::world::level::block::impls::vegetation_block::MayPlaceOn for FlowerBlock {}

impl BlockBehaviour for FlowerBlock {
    vegetation_behaviour_methods!();
    fn get_suspicious_effects(&self) -> Option<SuspiciousStewEffects> {
        Some(self.effects.clone())
    }
}

register_block_class!("FlowerBlock", |block| Box::new(FlowerBlock::new(block)));
