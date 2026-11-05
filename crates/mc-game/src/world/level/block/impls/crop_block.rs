//! `net.minecraft.world.level.block.CropBlock` (extends `VegetationBlock`; wheat) and its subclasses `CarrotBlock`,
//! `PotatoBlock`, `BeetrootBlock`, `TorchflowerCropBlock` (the overrides are the [`Kind`] of the struct).
use super::prelude::*;
use super::vegetation_block::{self, MayPlaceOn};
use crate::server::level::ServerLevel;
use crate::world::item::ItemStack;
use crate::world::level::block::bonemeal::BonemealSource;
use mc_core::math::mth;
use mc_data::Item;
use mc_data::props::IntegerProperty;

/// Which Java class the block is (their overrides of `CropBlock`).
#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Crop,
    Beetroot,
    Torchflower,
}

pub struct CropBlock {
    kind: Kind,
    /// `getAgeProperty()`.
    age: IntegerProperty,
    /// `getMaxAge()`.
    max_age: i32,
    /// `getBaseSeedId()`.
    seed: Item,
}

/// `CropBlock.hasSufficientLight`.
pub fn has_sufficient_light(level: &dyn LevelReader, pos: BlockPos) -> bool {
    level.get_raw_brightness(pos, 0) >= 8
}

/// `CropBlock.getGrowthSpeed(type, level, pos)` (also used by `StemBlock`).
pub fn get_growth_speed(block_type: Block, level: &dyn BlockGetter, pos: BlockPos) -> f32 {
    let mut speed = 1.0f32;
    let below = pos.below();
    for xx in -1..=1 {
        for zz in -1..=1 {
            let mut block_speed = 0.0f32;
            let block_state = level.get_block_state(below.offset(xx, 0, zz));
            if block_state.is_tag("grows_crops") {
                block_speed = 1.0;
                if block_state.try_get(props::MOISTURE).unwrap_or(0) > 0 {
                    block_speed = 3.0;
                }
            }
            if xx != 0 || zz != 0 {
                block_speed /= 4.0;
            }
            speed += block_speed;
        }
    }
    let (north, south, west, east) = (pos.north(), pos.south(), pos.west(), pos.east());
    let is_type = |p: BlockPos| level.get_block_state(p).is_block(block_type);
    let horizontal = is_type(west) || is_type(east);
    let vertical = is_type(north) || is_type(south);
    if horizontal && vertical {
        speed /= 2.0;
    } else {
        let diagonal = is_type(west.north()) || is_type(east.north()) || is_type(east.south()) || is_type(west.south());
        if diagonal {
            speed /= 2.0;
        }
    }
    speed
}

impl CropBlock {
    fn get_age(&self, state: BlockState) -> i32 {
        state.get(self.age)
    }

    fn is_max_age(&self, state: BlockState) -> bool {
        self.get_age(state) >= self.max_age
    }

    /// `getStateForAge(age)`.
    fn get_state_for_age(&self, block: Block, age: i32) -> BlockState {
        if self.kind == Kind::Torchflower && age == 2 { Block::TORCHFLOWER.default_state() } else { block.default_state().set(self.age, age) }
    }

    /// `getBonemealAgeIncrease(level)`.
    fn get_bonemeal_age_increase(&self, level: &mut ServerLevel) -> i32 {
        match self.kind {
            Kind::Crop => mth::next_int(level.random(), 2, 5),
            Kind::Beetroot => mth::next_int(level.random(), 2, 5) / 3,
            Kind::Torchflower => 1,
        }
    }

    /// `growCrops(level, pos, state)`.
    pub fn grow_crops(&self, level: &mut ServerLevel, pos: BlockPos, state: BlockState) {
        let age = self.max_age.min(self.get_age(state) + self.get_bonemeal_age_increase(level));
        level.set_block_flags(pos, self.get_state_for_age(state.block(), age), 2);
    }

    /// `CropBlock.randomTick` (the `super` of the beetroot and torchflower overrides).
    fn grow_randomly(&self, state: BlockState, level: &mut ServerLevel, pos: BlockPos) {
        if level.get_raw_brightness(pos, 0) >= 9 {
            let age = self.get_age(state);
            if age < self.max_age {
                let growth_speed = get_growth_speed(state.block(), &*level, pos);
                if level.random().next_int_bounded((25.0f32 / growth_speed) as i32 + 1) == 0 {
                    level.set_block_flags(pos, self.get_state_for_age(state.block(), age + 1), 2);
                }
            }
        }
    }
}

impl MayPlaceOn for CropBlock {
    fn may_place_on(&self, state: BlockState, _level: &dyn BlockGetter, _pos: BlockPos) -> bool {
        state.is_tag("supports_crops")
    }
}

impl BlockBehaviour for CropBlock {
    fn can_survive(&self, _state: BlockState, level: &dyn LevelReader, pos: BlockPos) -> bool {
        has_sufficient_light(level, pos) && vegetation_block::can_survive(self, level, pos)
    }
    update_shape!(|self, state, level, pos, direction, neighbour_pos, neighbour_state| {
        vegetation_block::update_shape(state, &*level, pos)
    });
    fn random_tick(&self, state: BlockState, level: &mut ServerLevel, pos: BlockPos) {
        if self.kind != Kind::Crop && level.random().next_int_bounded(3) == 0 {
            return;
        }
        self.grow_randomly(state, level, pos);
    }
    fn entity_inside(
        &self,
        _state: BlockState,
        level: &mut ServerLevel,
        pos: BlockPos,
        entity: &mut dyn crate::world::entity::Entity,
        _effect_applier: &mut dyn crate::world::entity::inside_block_effects::InsideBlockEffectApplier,
        _is_precise: bool,
    ) {
        vegetation_block::ravager_destroys_crop(level, pos, &*entity);
    }
    fn get_clone_item_stack(&self, _state: BlockState, _level: &ServerLevel, _pos: BlockPos, _include_data: bool) -> ItemStack {
        ItemStack::new(self.seed)
    }
    fn is_bonemealable(&self) -> bool {
        true
    }
    fn is_valid_bonemeal_target(&self, _level: &dyn LevelReader, _pos: BlockPos, state: BlockState, _source: BonemealSource) -> bool {
        !self.is_max_age(state)
    }
    fn perform_bonemeal(&self, level: &mut ServerLevel, pos: BlockPos, state: BlockState, _source: BonemealSource) {
        self.grow_crops(level, pos, state);
    }
}

register_block_class!("CropBlock", |_| Box::new(CropBlock { kind: Kind::Crop, age: props::AGE_7, max_age: 7, seed: Item::WHEAT_SEEDS }));
register_block_class!("CarrotBlock", |_| Box::new(CropBlock { kind: Kind::Crop, age: props::AGE_7, max_age: 7, seed: Item::CARROT }));
register_block_class!("PotatoBlock", |_| Box::new(CropBlock { kind: Kind::Crop, age: props::AGE_7, max_age: 7, seed: Item::POTATO }));
register_block_class!("BeetrootBlock", |_| Box::new(CropBlock { kind: Kind::Beetroot, age: props::AGE_3, max_age: 3, seed: Item::BEETROOT_SEEDS }));
register_block_class!("TorchflowerCropBlock", |_| Box::new(CropBlock { kind: Kind::Torchflower, age: props::AGE_1, max_age: 2, seed: Item::TORCHFLOWER_SEEDS }));
