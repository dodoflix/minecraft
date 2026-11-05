//! `net.minecraft.world.level.block.MultifaceSpreader` (used by glow lichen, sculk veins and resin clumps).
use super::multiface_block;
use super::prelude::*;
use mc_core::random::RandomSource;

/// `MultifaceSpreader.SpreadPos`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SpreadPos {
    pub pos: BlockPos,
    pub face: Direction,
}

/// `MultifaceSpreader.SpreadType`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpreadType {
    SamePosition,
    SamePlane,
    WrapAround,
}

impl SpreadType {
    /// `getSpreadPos(pos, spreadDirection, fromFace)`.
    pub fn get_spread_pos(self, pos: BlockPos, spread_direction: Direction, from_face: Direction) -> SpreadPos {
        match self {
            SpreadType::SamePosition => SpreadPos { pos, face: spread_direction },
            SpreadType::SamePlane => SpreadPos { pos: pos.relative(spread_direction), face: from_face },
            SpreadType::WrapAround => SpreadPos { pos: pos.relative(spread_direction).relative(from_face), face: spread_direction.get_opposite() },
        }
    }
}

/// `MultifaceSpreader.DEFAULT_SPREAD_ORDER`.
pub const DEFAULT_SPREAD_ORDER: [SpreadType; 3] = [SpreadType::SamePosition, SpreadType::SamePlane, SpreadType::WrapAround];

/// `MultifaceSpreader.SpreadConfig` (defaults included).
pub trait SpreadConfig: Send + Sync {
    fn get_state_for_placement(&self, old_state: BlockState, level: &dyn BlockGetter, placement_pos: BlockPos, placement_direction: Direction) -> Option<BlockState>;
    fn can_spread_into(&self, level: &dyn BlockGetter, source_pos: BlockPos, spread_pos: SpreadPos) -> bool;
    fn get_spread_types(&self) -> &[SpreadType] {
        &DEFAULT_SPREAD_ORDER
    }
    fn has_face(&self, state: BlockState, face: Direction) -> bool {
        multiface_block::has_face(state, face)
    }
    fn is_other_block_valid_as_source(&self, _state: BlockState) -> bool {
        false
    }
    fn can_spread_from(&self, state: BlockState, face: Direction) -> bool {
        self.is_other_block_valid_as_source(state) || self.has_face(state, face)
    }
    fn place_block(&self, level: &mut dyn LevelAccessor, spread_pos: SpreadPos, old_state: BlockState, post_process: bool) -> bool {
        match self.get_state_for_placement(old_state, &*level, spread_pos.pos, spread_pos.face) {
            Some(spread_state) => {
                if post_process {
                    level.mark_for_post_processing(spread_pos.pos);
                }
                level.set_block(spread_pos.pos, spread_state, 2, 512)
            }
            None => false,
        }
    }
}

/// `MultifaceSpreader.DefaultSpreaderConfig`.
pub struct DefaultSpreaderConfig {
    pub block: Block,
}

impl DefaultSpreaderConfig {
    /// `stateCanBeReplaced(level, sourcePos, placementPos, placementDirection, existingState)`.
    pub fn state_can_be_replaced(&self, existing_state: BlockState) -> bool {
        existing_state.is_air()
            || existing_state.is_block(self.block)
            || (existing_state.is_block(Block::WATER) && existing_state.get_fluid_state().is_source())
    }
}

impl SpreadConfig for DefaultSpreaderConfig {
    fn get_state_for_placement(&self, old_state: BlockState, level: &dyn BlockGetter, placement_pos: BlockPos, placement_direction: Direction) -> Option<BlockState> {
        multiface_block::get_state_for_placement(self.block, old_state, level, placement_pos, placement_direction)
    }
    fn can_spread_into(&self, level: &dyn BlockGetter, _source_pos: BlockPos, spread_pos: SpreadPos) -> bool {
        let existing_state = level.get_block_state(spread_pos.pos);
        self.state_can_be_replaced(existing_state) && multiface_block::is_valid_state_for_placement(self.block, level, existing_state, spread_pos.pos, spread_pos.face)
    }
}

pub struct MultifaceSpreader {
    config: Box<dyn SpreadConfig>,
}

impl MultifaceSpreader {
    /// `new MultifaceSpreader(multifaceBlock)`.
    pub fn new(block: Block) -> Self {
        Self::with_config(Box::new(DefaultSpreaderConfig { block }))
    }

    /// `new MultifaceSpreader(config)`.
    pub fn with_config(config: Box<dyn SpreadConfig>) -> Self {
        Self { config }
    }

    pub fn config(&self) -> &dyn SpreadConfig {
        &*self.config
    }

    /// `canSpreadInAnyDirection`.
    pub fn can_spread_in_any_direction(&self, state: BlockState, level: &dyn BlockGetter, pos: BlockPos, starting_face: Direction) -> bool {
        Direction::ALL
            .into_iter()
            .any(|d| self.get_spread_from_face_toward_direction(state, level, pos, starting_face, d, &|l, p, s| self.config.can_spread_into(l, p, s)).is_some())
    }

    /// `spreadFromRandomFaceTowardRandomDirection`.
    pub fn spread_from_random_face_toward_random_direction(
        &self,
        state: BlockState,
        level: &mut dyn LevelAccessor,
        pos: BlockPos,
        random: &mut dyn RandomSource,
    ) -> Option<SpreadPos> {
        for face_direction in Direction::all_shuffled(random) {
            if self.config.can_spread_from(state, face_direction)
                && let Some(spread) = self.spread_from_face_toward_random_direction(state, level, pos, face_direction, random, false)
            {
                return Some(spread);
            }
        }
        None
    }

    /// `spreadFromRandomFaceTowardRandomDirection(state, level, pos, level.getRandom())`: the same, drawing from the
    /// level's own random (which cannot be passed alongside the `&mut` level).
    pub fn spread_from_random_face_toward_random_direction_with_level_random(&self, state: BlockState, level: &mut dyn LevelAccessor, pos: BlockPos) -> Option<SpreadPos> {
        for face_direction in Direction::all_shuffled(level.random()) {
            if self.config.can_spread_from(state, face_direction) {
                for spread_direction in Direction::all_shuffled(level.random()) {
                    if let Some(spread) = self.spread_from_face_toward_direction(state, level, pos, face_direction, spread_direction, false) {
                        return Some(spread);
                    }
                }
            }
        }
        None
    }

    /// `spreadAll`.
    pub fn spread_all(&self, state: BlockState, level: &mut dyn LevelAccessor, pos: BlockPos, post_process: bool) -> i64 {
        let mut sum = 0;
        for face_direction in Direction::ALL {
            if self.config.can_spread_from(state, face_direction) {
                sum += self.spread_from_face_toward_all_directions(state, level, pos, face_direction, post_process);
            }
        }
        sum
    }

    /// `spreadFromFaceTowardRandomDirection`.
    pub fn spread_from_face_toward_random_direction(
        &self,
        state: BlockState,
        level: &mut dyn LevelAccessor,
        pos: BlockPos,
        starting_face: Direction,
        random: &mut dyn RandomSource,
        post_process: bool,
    ) -> Option<SpreadPos> {
        for spread_direction in Direction::all_shuffled(random) {
            if let Some(spread) = self.spread_from_face_toward_direction(state, level, pos, starting_face, spread_direction, post_process) {
                return Some(spread);
            }
        }
        None
    }

    fn spread_from_face_toward_all_directions(&self, state: BlockState, level: &mut dyn LevelAccessor, pos: BlockPos, starting_face: Direction, post_process: bool) -> i64 {
        let mut count = 0;
        for spread_direction in Direction::ALL {
            if self.spread_from_face_toward_direction(state, level, pos, starting_face, spread_direction, post_process).is_some() {
                count += 1;
            }
        }
        count
    }

    /// `spreadFromFaceTowardDirection`.
    pub fn spread_from_face_toward_direction(
        &self,
        state: BlockState,
        level: &mut dyn LevelAccessor,
        pos: BlockPos,
        from_face: Direction,
        spread_direction: Direction,
        post_process: bool,
    ) -> Option<SpreadPos> {
        let spread_pos = self.get_spread_from_face_toward_direction(state, &*level, pos, from_face, spread_direction, &|l, p, s| self.config.can_spread_into(l, p, s))?;
        self.spread_to_face(level, spread_pos, post_process)
    }

    /// `getSpreadFromFaceTowardDirection`.
    pub fn get_spread_from_face_toward_direction(
        &self,
        state: BlockState,
        level: &dyn BlockGetter,
        pos: BlockPos,
        starting_face: Direction,
        spread_direction: Direction,
        can_spread_into: &dyn Fn(&dyn BlockGetter, BlockPos, SpreadPos) -> bool,
    ) -> Option<SpreadPos> {
        if spread_direction.get_axis() == starting_face.get_axis() {
            return None;
        }
        if self.config.is_other_block_valid_as_source(state) || (self.config.has_face(state, starting_face) && !self.config.has_face(state, spread_direction)) {
            for ty in self.config.get_spread_types() {
                let spread_pos = ty.get_spread_pos(pos, spread_direction, starting_face);
                if can_spread_into(level, pos, spread_pos) {
                    return Some(spread_pos);
                }
            }
        }
        None
    }

    /// `spreadToFace`.
    pub fn spread_to_face(&self, level: &mut dyn LevelAccessor, spread_pos: SpreadPos, post_process: bool) -> Option<SpreadPos> {
        let old_state = level.get_block_state(spread_pos.pos);
        if self.config.place_block(level, spread_pos, old_state, post_process) { Some(spread_pos) } else { None }
    }
}
