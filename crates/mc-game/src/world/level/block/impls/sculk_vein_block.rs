//! `net.minecraft.world.level.block.SculkVeinBlock` (extends `MultifaceSpreadeableBlock`, implements `SculkBehaviour`; the
//! shape logic is `MultifaceBlock`'s).
use super::multiface_block::{self, get_face_property, has_any_face, has_face};
use super::multiface_spreader::{DEFAULT_SPREAD_ORDER, DefaultSpreaderConfig, MultifaceSpreader, SpreadConfig, SpreadPos, SpreadType};
use super::prelude::*;
use crate::world::level::block::sculk_behaviour::SculkBehaviour;
use crate::world::level::block::sculk_spreader::{ChargeCursor, SculkSpreader};
use mc_core::math::mth;
use mc_core::random::RandomSource;
use mc_data::fluid::Fluid;
use mc_data::SoundEvent;
use mc_protocol::packets::play::level::SoundSource;
use std::sync::LazyLock;

/// `SculkVeinBlock.SculkVeinSpreaderConfig` (extends `DefaultSpreaderConfig`).
struct SculkVeinSpreaderConfig {
    default: DefaultSpreaderConfig,
    spread_types: &'static [SpreadType],
}

impl SculkVeinSpreaderConfig {
    fn new(spread_types: &'static [SpreadType]) -> Self {
        Self { default: DefaultSpreaderConfig { block: Block::SCULK_VEIN }, spread_types }
    }

    /// `stateCanBeReplaced(level, sourcePos, placementPos, placementDirection, existingState)`.
    fn state_can_be_replaced(&self, level: &dyn BlockGetter, source_pos: BlockPos, placement_pos: BlockPos, placement_direction: Direction, existing_state: BlockState) -> bool {
        let against_state = level.get_block_state(placement_pos.relative(placement_direction));
        if against_state.is_block(Block::SCULK) || against_state.is_block(Block::SCULK_CATALYST) || against_state.is_block(Block::MOVING_PISTON) {
            return false;
        }
        if source_pos.dist_manhattan(&placement_pos) == 2 {
            let neighbour_pos = source_pos.relative(placement_direction.get_opposite());
            if level.get_block_state(neighbour_pos).is_face_sturdy_at(level, neighbour_pos, placement_direction) {
                return false;
            }
        }
        let fluid_state = existing_state.get_fluid_state();
        if !fluid_state.is_empty() && fluid_state.fluid() != Fluid::WATER {
            return false;
        }
        !existing_state.is_tag("fire") && (existing_state.can_be_replaced() || self.default.state_can_be_replaced(existing_state))
    }
}

impl SpreadConfig for SculkVeinSpreaderConfig {
    fn get_state_for_placement(&self, old_state: BlockState, level: &dyn BlockGetter, placement_pos: BlockPos, placement_direction: Direction) -> Option<BlockState> {
        self.default.get_state_for_placement(old_state, level, placement_pos, placement_direction)
    }
    fn can_spread_into(&self, level: &dyn BlockGetter, source_pos: BlockPos, spread_pos: SpreadPos) -> bool {
        let existing_state = level.get_block_state(spread_pos.pos);
        self.state_can_be_replaced(level, source_pos, spread_pos.pos, spread_pos.face, existing_state)
            && multiface_block::is_valid_state_for_placement(Block::SCULK_VEIN, level, existing_state, spread_pos.pos, spread_pos.face)
    }
    fn get_spread_types(&self) -> &[SpreadType] {
        self.spread_types
    }
    fn is_other_block_valid_as_source(&self, state: BlockState) -> bool {
        !state.is_block(Block::SCULK_VEIN)
    }
}

static VEIN_SPREADER: LazyLock<MultifaceSpreader> = LazyLock::new(|| MultifaceSpreader::with_config(Box::new(SculkVeinSpreaderConfig::new(&DEFAULT_SPREAD_ORDER))));
static SAME_SPACE_SPREADER: LazyLock<MultifaceSpreader> =
    LazyLock::new(|| MultifaceSpreader::with_config(Box::new(SculkVeinSpreaderConfig::new(&[SpreadType::SamePosition]))));

/// `SculkVeinBlock.getSpreader()`.
pub fn vein_spreader() -> &'static MultifaceSpreader {
    &VEIN_SPREADER
}

/// `SculkVeinBlock.getSameSpaceSpreader()`.
pub fn same_space_spreader() -> &'static MultifaceSpreader {
    &SAME_SPACE_SPREADER
}

/// `SculkVeinBlock.regrow(level, pos, existing, faces)`.
pub fn regrow(level: &mut dyn LevelAccessor, pos: BlockPos, existing: BlockState, faces: &[Direction]) -> bool {
    let mut has_at_least_one_face = false;
    let mut new_state = Block::SCULK_VEIN.default_state();
    for &face in faces {
        if multiface_block::can_attach_to_pos(&*level, pos, face) {
            new_state = new_state.set(get_face_property(face), true);
            has_at_least_one_face = true;
        }
    }
    if !has_at_least_one_face {
        return false;
    }
    if !existing.get_fluid_state().is_empty() {
        new_state = new_state.set(props::WATERLOGGED, true);
    }
    level.set_block_and_update(pos, new_state);
    true
}

/// `SculkVeinBlock.hasSubstrateAccess(level, state, pos)`.
pub fn has_substrate_access(level: &dyn LevelAccessor, state: BlockState, pos: BlockPos) -> bool {
    state.is_block(Block::SCULK_VEIN)
        && Direction::ALL.into_iter().any(|direction| has_face(state, direction) && level.get_block_state(pos.relative(direction)).is_tag("sculk_replaceable"))
}

/// `SculkVeinBlock`'s `SculkBehaviour`.
pub struct SculkVeinBehaviour;

pub static BEHAVIOUR: SculkVeinBehaviour = SculkVeinBehaviour;

/// `SculkVeinBlock.attemptPlaceSculk`.
fn attempt_place_sculk(spreader: &SculkSpreader, level: &mut dyn LevelAccessor, pos: BlockPos, random: &mut dyn RandomSource) -> bool {
    let state = level.get_block_state(pos);
    let replace_tag = spreader.replaceable_blocks();
    for support in Direction::all_shuffled(random) {
        if has_face(state, support) {
            let support_pos = pos.relative(support);
            let support_state = level.get_block_state(support_pos);
            if support_state.is_tag(replace_tag) {
                let default_sculk = Block::SCULK.default_state();
                level.set_block_and_update(support_pos, default_sculk);
                level.push_entities_up(support_state, default_sculk, support_pos);
                level.play_sound(None, support_pos, SoundEvent::BLOCK_SCULK_SPREAD, SoundSource::Blocks, 1.0, 1.0);
                vein_spreader().spread_all(default_sculk, level, support_pos, spreader.is_world_generation());
                let skip = support.get_opposite();
                for vein_blocks in Direction::ALL {
                    if vein_blocks != skip {
                        let vein_pos = support_pos.relative(vein_blocks);
                        let possible_vein_block = level.get_block_state(vein_pos);
                        if possible_vein_block.is_block(Block::SCULK_VEIN) {
                            BEHAVIOUR.on_discharged(level, possible_vein_block, vein_pos, random);
                        }
                    }
                }
                return true;
            }
        }
    }
    false
}

impl SculkBehaviour for SculkVeinBehaviour {
    fn on_discharged(&self, level: &mut dyn LevelAccessor, mut state: BlockState, pos: BlockPos, _random: &mut dyn RandomSource) {
        if !state.is_block(Block::SCULK_VEIN) {
            return;
        }
        for dir in Direction::ALL {
            let side_property = get_face_property(dir);
            if state.get(side_property) && level.get_block_state(pos.relative(dir)).is_block(Block::SCULK) {
                state = state.set(side_property, false);
            }
        }
        if !has_any_face(state) {
            state = if level.get_fluid_state(pos).is_empty() { Block::AIR } else { Block::WATER }.default_state();
        }
        level.set_block_and_update(pos, state);
    }

    fn attempt_use_charge(
        &self,
        cursor: &ChargeCursor,
        level: &mut dyn LevelAccessor,
        _origin_pos: BlockPos,
        random: &mut dyn RandomSource,
        spreader: &SculkSpreader,
        spread_veins: bool,
    ) -> i32 {
        if spread_veins && attempt_place_sculk(spreader, level, cursor.get_pos(), random) {
            cursor.get_charge() - 1
        } else if random.next_int_bounded(spreader.charge_decay_rate()) == 0 {
            mth::floor_f32(cursor.get_charge() as f32 * 0.5)
        } else {
            cursor.get_charge()
        }
    }
}
