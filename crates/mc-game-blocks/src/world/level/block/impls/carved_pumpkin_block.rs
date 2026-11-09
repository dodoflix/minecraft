//! `net.minecraft.world.level.block.CarvedPumpkinBlock` (carved pumpkin and jack o'lantern; extends
//! `HorizontalDirectionalBlock`): the golem patterns built by placing the head.
use super::horizontal_directional_block::HorizontalDirectionalBlock;
use mc_data::EntityType;
use mc_data::block::Block;
use mc_game_core::register_block_class;
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::entity::Entity;
use mc_game_core::world::item::BlockPlaceContext;
use mc_game_core::world::level::block::impls::copper_chest_block::get_from_copper_block;
use mc_game_core::world::level::block::impls::prelude::*;
use mc_game_core::world::level::block::pattern::BlockPatternMatch;
use mc_game_core::world::level::block::pattern::{BlockPattern, BlockPatternBuilder};
use mc_game_core::world::level::block::pattern_blocks;
use mc_game_core::world::level::block::weathering_copper::{self, WeatherState};
use mc_nbt::CompoundTag;
use std::sync::LazyLock;

pub struct CarvedPumpkinBlock;

fn is_pumpkin(state: BlockState) -> bool {
    state.is_block(Block::CARVED_PUMPKIN) || state.is_block(Block::JACK_O_LANTERN)
}

static SNOW_GOLEM_BASE: LazyLock<BlockPattern> = LazyLock::new(|| {
    BlockPatternBuilder::start()
        .aisle(&[" ", "#", "#"])
        .where_('#', |b, level| b.is_block(level, Block::SNOW_BLOCK))
        .build()
});

static SNOW_GOLEM_FULL: LazyLock<BlockPattern> = LazyLock::new(|| {
    BlockPatternBuilder::start()
        .aisle(&["^", "#", "#"])
        .where_('^', |b, level| b.get_state(level).is_some_and(is_pumpkin))
        .where_('#', |b, level| b.is_block(level, Block::SNOW_BLOCK))
        .build()
});

static IRON_GOLEM_BASE: LazyLock<BlockPattern> = LazyLock::new(|| {
    BlockPatternBuilder::start()
        .aisle(&["~ ~", "###", "~#~"])
        .where_('#', |b, level| b.is_block(level, Block::IRON_BLOCK))
        .where_('~', |b, level| {
            b.get_state(level).is_some_and(|s| s.is_air())
        })
        .build()
});

static IRON_GOLEM_FULL: LazyLock<BlockPattern> = LazyLock::new(|| {
    BlockPatternBuilder::start()
        .aisle(&["~^~", "###", "~#~"])
        .where_('^', |b, level| b.get_state(level).is_some_and(is_pumpkin))
        .where_('#', |b, level| b.is_block(level, Block::IRON_BLOCK))
        .where_('~', |b, level| {
            b.get_state(level).is_some_and(|s| s.is_air())
        })
        .build()
});

fn is_copper(state: BlockState) -> bool {
    state.is_tag("copper")
}

static COPPER_GOLEM_BASE: LazyLock<BlockPattern> = LazyLock::new(|| {
    BlockPatternBuilder::start()
        .aisle(&[" ", "#"])
        .where_('#', |b, level| b.get_state(level).is_some_and(is_copper))
        .build()
});

static COPPER_GOLEM_FULL: LazyLock<BlockPattern> = LazyLock::new(|| {
    BlockPatternBuilder::start()
        .aisle(&["^", "#"])
        .where_('^', |b, level| b.get_state(level).is_some_and(is_pumpkin))
        .where_('#', |b, level| b.get_state(level).is_some_and(is_copper))
        .build()
});

/// `CarvedPumpkinBlock.canSpawnGolem(level, topPos)`.
pub fn can_spawn_golem(level: &dyn LevelReader, top_pos: BlockPos) -> bool {
    SNOW_GOLEM_BASE.find(level, top_pos).is_some()
        || IRON_GOLEM_BASE.find(level, top_pos).is_some()
        || COPPER_GOLEM_BASE.find(level, top_pos).is_some()
}

/// `EntityType.create(level, TRIGGERED)` followed by the state vanilla sets through a setter (`tag` is loaded into the new
/// entity like the saved form of that setter).
fn create_golem(
    level: &ServerLevel,
    ty: EntityType,
    tag: Option<CompoundTag>,
) -> Option<Box<dyn Entity>> {
    let mut golem = pattern_blocks::create_triggered(level, ty)?;
    if let Some(tag) = tag
        && let Err(message) = golem.load(&tag)
    {
        log::warn!("Exception configuring summoned {}: {message}", ty.name());
        return None;
    }
    Some(golem)
}

/// `getWeatherStateFromPattern(copperGolemMatch)` for the matched copper block.
fn weather_state_of(block: Block) -> WeatherState {
    weathering_copper::age(block)
        .or_else(|| weathering_copper::wax_off(block).and_then(weathering_copper::age))
        .unwrap_or(WeatherState::Unaffected)
}

/// `trySpawnGolem(level, topPos)`.
fn try_spawn_golem(level: &mut ServerLevel, top_pos: BlockPos) {
    if let Some(mut snow) = SNOW_GOLEM_FULL.find(&*level, top_pos)
        && let Some(golem) = create_golem(level, EntityType::SNOW_GOLEM, None)
    {
        let spawn_pos = snow.get_block(0, 2, 0).get_pos();
        pattern_blocks::spawn_golem_in_world(level, &mut snow, golem, spawn_pos);
        return;
    }
    if let Some(mut iron) = IRON_GOLEM_FULL.find(&*level, top_pos) {
        let mut tag = CompoundTag::new();
        tag.put_boolean("PlayerCreated", true);
        if let Some(golem) = create_golem(level, EntityType::IRON_GOLEM, Some(tag)) {
            let spawn_pos = iron.get_block(1, 2, 0).get_pos();
            pattern_blocks::spawn_golem_in_world(level, &mut iron, golem, spawn_pos);
            return;
        }
    }
    if let Some(mut copper) = COPPER_GOLEM_FULL.find(&*level, top_pos) {
        // `copperGolem.spawn(weatherState)` is `setWeatherState` + `playSpawnSound`; the state goes in as its saved form.
        // Not implemented: `CopperGolem.playSpawnSound` (the `CopperGolem` entity does not exist yet).
        let mut tag = CompoundTag::new();
        tag.put_string(
            "weather_state",
            match weather_state_of(
                copper
                    .get_block(0, 1, 0)
                    .get_state(&*level)
                    .expect("the copper block was matched")
                    .block(),
            ) {
                WeatherState::Unaffected => "unaffected",
                WeatherState::Exposed => "exposed",
                WeatherState::Weathered => "weathered",
                WeatherState::Oxidized => "oxidized",
            },
        );
        if let Some(golem) = create_golem(level, EntityType::COPPER_GOLEM, Some(tag)) {
            let spawn_pos = copper.get_block(0, 0, 0).get_pos();
            pattern_blocks::spawn_golem_in_world(level, &mut copper, golem, spawn_pos);
            replace_copper_block_with_chest(level, &mut copper);
        }
    }
}

/// `replaceCopperBlockWithChest(level, match)`.
fn replace_copper_block_with_chest(level: &mut ServerLevel, m: &mut BlockPatternMatch) {
    let copper_block = m.get_block(0, 1, 0);
    let copper_pos = copper_block.get_pos();
    let copper = copper_block
        .get_state(&*level)
        .expect("the copper block was matched")
        .block();
    let facing = m
        .get_block(0, 0, 0)
        .get_state(&*level)
        .expect("the pumpkin was matched")
        .get(props::HORIZONTAL_FACING);
    let chest = get_from_copper_block(copper, facing, &*level, copper_pos);
    level.set_block_flags(copper_pos, chest, 2);
}

impl BlockBehaviour for CarvedPumpkinBlock {
    fn on_place(
        &self,
        state: BlockState,
        level: &mut ServerLevel,
        pos: BlockPos,
        old_state: BlockState,
        _moved_by_piston: bool,
    ) {
        if !old_state.is_block(state.block()) {
            try_spawn_golem(level, pos);
        }
    }
    fn get_state_for_placement(
        &self,
        block: Block,
        _level: &mut ServerLevel,
        context: &BlockPlaceContext,
    ) -> Option<BlockState> {
        Some(block.default_state().set(
            props::HORIZONTAL_FACING,
            context.get_horizontal_direction().get_opposite(),
        ))
    }
    fn rotate(&self, state: BlockState, rotation: Rotation) -> BlockState {
        HorizontalDirectionalBlock.rotate(state, rotation)
    }
    fn mirror(&self, state: BlockState, mirror: Mirror) -> BlockState {
        HorizontalDirectionalBlock.mirror(state, mirror)
    }
}

register_block_class!("CarvedPumpkinBlock", |_| Box::new(CarvedPumpkinBlock));

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn copper_block_stage_comes_from_its_unwaxed_block() {
        use WeatherState::*;
        let stages = [
            ("copper_block", Unaffected),
            ("exposed_copper", Exposed),
            ("weathered_copper", Weathered),
            ("oxidized_copper", Oxidized),
        ];
        for (name, stage) in stages {
            for prefix in ["", "waxed_"] {
                let block = Block::by_name(&format!("minecraft:{prefix}{name}")).unwrap();
                assert_eq!(weather_state_of(block), stage, "{prefix}{name}");
            }
        }
    }
}
