//! `SculkSpreader`: the charge cursors that spread sculk (world generation `sculk_patch` and the sculk catalyst).
//!
//! shortcut: the particle level events of `updateCursors` (`levelEvent(3006, ..)`) are emitted in insertion order of the
//! charged positions, Java iterates an `Object2IntOpenHashMap`; no consumer reads them outside a server level, make the
//! order match when a catalyst block entity drives this spreader.

use super::sculk_behaviour::{self, SculkBehaviour};
use crate::world::level::block::BlockStateExt;
use crate::world::level::block::impls::{multiface_block, sculk_vein_block};
use crate::world::level::{BlockGetter, LevelAccessor};
use mc_core::direction::Direction;
use mc_core::math::mth;
use mc_core::pos::{BlockPos, Vec3i};
use mc_core::random::RandomSource;
use mc_data::SoundEvent;
use mc_protocol::packets::play::level::SoundSource;
use std::collections::HashMap;
use std::sync::LazyLock;

pub const MAX_GROWTH_RATE_RADIUS: i32 = 24;
pub const MAX_CHARGE: i32 = 1000;
pub const MAX_DECAY_FACTOR: f32 = 0.5;
const MAX_CURSORS: usize = 32;
pub const SHRIEKER_PLACEMENT_RATE: i32 = 11;
pub const MAX_CURSOR_DISTANCE: i32 = 1024;

pub struct SculkSpreader {
    is_world_generation: bool,
    /// The `TagKey<Block>` of the replaceable blocks (tag path).
    replaceable_blocks: &'static str,
    growth_spawn_cost: i32,
    no_growth_radius: i32,
    charge_decay_rate: i32,
    additional_decay_rate: i32,
    cursors: Vec<ChargeCursor>,
}

impl SculkSpreader {
    pub fn new(
        is_world_generation: bool,
        replaceable_blocks: &'static str,
        growth_spawn_cost: i32,
        no_growth_radius: i32,
        charge_decay_rate: i32,
        additional_decay_rate: i32,
    ) -> SculkSpreader {
        SculkSpreader { is_world_generation, replaceable_blocks, growth_spawn_cost, no_growth_radius, charge_decay_rate, additional_decay_rate, cursors: Vec::new() }
    }

    /// `createLevelSpreader()`.
    pub fn create_level_spreader() -> SculkSpreader {
        SculkSpreader::new(false, "sculk_replaceable", 10, 4, 10, 5)
    }

    /// `createWorldGenSpreader()`.
    pub fn create_world_gen_spreader() -> SculkSpreader {
        SculkSpreader::new(true, "sculk_replaceable_world_gen", 50, 1, 5, 10)
    }

    pub fn replaceable_blocks(&self) -> &'static str {
        self.replaceable_blocks
    }

    pub fn growth_spawn_cost(&self) -> i32 {
        self.growth_spawn_cost
    }

    pub fn no_growth_radius(&self) -> i32 {
        self.no_growth_radius
    }

    pub fn charge_decay_rate(&self) -> i32 {
        self.charge_decay_rate
    }

    pub fn additional_decay_rate(&self) -> i32 {
        self.additional_decay_rate
    }

    pub fn is_world_generation(&self) -> bool {
        self.is_world_generation
    }

    pub fn get_cursors(&self) -> &[ChargeCursor] {
        &self.cursors
    }

    pub fn clear(&mut self) {
        self.cursors.clear();
    }

    /// `addCursors(startPos, charge)`.
    pub fn add_cursors(&mut self, start_pos: BlockPos, mut charge: i32) {
        while charge > 0 {
            let current_charge = charge.min(MAX_CHARGE);
            self.add_cursor(ChargeCursor::new(start_pos, current_charge));
            charge -= current_charge;
        }
    }

    fn add_cursor(&mut self, cursor: ChargeCursor) {
        if self.cursors.len() < MAX_CURSORS {
            self.cursors.push(cursor);
        }
    }

    /// `updateCursors(level, originPos, random, spreadVeins)`.
    pub fn update_cursors(&mut self, level: &mut dyn LevelAccessor, origin_pos: BlockPos, random: &mut dyn RandomSource, spread_veins: bool) {
        if self.cursors.is_empty() {
            return;
        }
        let mut processed_cursors: Vec<ChargeCursor> = Vec::new();
        // position -> index into `processed_cursors` (`mergeableCursors`)
        let mut mergeable_cursors: HashMap<BlockPos, usize> = HashMap::new();
        let mut charge_map: indexmap::IndexMap<BlockPos, i32> = indexmap::IndexMap::new();

        for mut cursor in std::mem::take(&mut self.cursors) {
            if !cursor.is_pos_unreasonable(origin_pos) {
                cursor.update(level, origin_pos, random, self, spread_veins);
                if cursor.charge <= 0 {
                    level.level_event(None, 3006, cursor.get_pos(), 0);
                } else {
                    let pos = cursor.get_pos();
                    let entry = charge_map.entry(pos).or_insert(0);
                    *entry = entry.wrapping_add(cursor.charge);
                    match mergeable_cursors.get(&pos).copied() {
                        None => {
                            mergeable_cursors.insert(pos, processed_cursors.len());
                            processed_cursors.push(cursor);
                        }
                        Some(existing) if !self.is_world_generation && cursor.charge + processed_cursors[existing].charge <= MAX_CHARGE => {
                            processed_cursors[existing].merge_with(&mut cursor);
                        }
                        Some(existing) => {
                            let existing_charge = processed_cursors[existing].charge;
                            let replace = cursor.charge < existing_charge;
                            if replace {
                                mergeable_cursors.insert(pos, processed_cursors.len());
                            }
                            processed_cursors.push(cursor);
                        }
                    }
                }
            }
        }

        for (pos, charge) in charge_map {
            let faces = mergeable_cursors.get(&pos).and_then(|i| processed_cursors[*i].get_facing_data());
            if charge > 0
                && let Some(faces) = faces
            {
                let num_particles = (f64::from(charge).ln_1p() / f64::from(2.3f32)) as i32 + 1;
                let data = (num_particles << 6) + multiface_block::pack(faces);
                level.level_event(None, 3006, pos, data);
            }
        }

        self.cursors = processed_cursors;
    }
}

/// `SculkSpreader.ChargeCursor`.
#[derive(Clone, Debug)]
pub struct ChargeCursor {
    pos: BlockPos,
    charge: i32,
    update_delay: i32,
    decay_delay: i32,
    /// `Set<Direction> facings` (an enum set: ordinal order), `None` = `null`.
    facings: Option<Vec<Direction>>,
}

/// `ChargeCursor.NON_CORNER_NEIGHBOURS`.
static NON_CORNER_NEIGHBOURS: LazyLock<Vec<Vec3i>> = LazyLock::new(|| {
    BlockPos::between_closed(&BlockPos::new(-1, -1, -1), &BlockPos::new(1, 1, 1))
        .filter(|p| (p.x() == 0 || p.y() == 0 || p.z() == 0) && *p != BlockPos::ZERO)
        .map(|p| Vec3i::new(p.x(), p.y(), p.z()))
        .collect()
});

impl ChargeCursor {
    pub fn new(pos: BlockPos, charge: i32) -> ChargeCursor {
        ChargeCursor { pos, charge, update_delay: 0, decay_delay: 1, facings: None }
    }

    pub fn get_pos(&self) -> BlockPos {
        self.pos
    }

    fn is_pos_unreasonable(&self, origin_pos: BlockPos) -> bool {
        self.pos.dist_chessboard(&origin_pos) > MAX_CURSOR_DISTANCE
    }

    pub fn get_charge(&self) -> i32 {
        self.charge
    }

    pub fn get_decay_delay(&self) -> i32 {
        self.decay_delay
    }

    pub fn get_facing_data(&self) -> Option<&[Direction]> {
        self.facings.as_deref()
    }

    fn should_update(&self, level: &dyn LevelAccessor, pos: BlockPos, is_world_gen: bool) -> bool {
        if self.charge <= 0 {
            false
        } else if is_world_gen {
            true
        } else {
            level.should_tick_blocks_at_pos(pos)
        }
    }

    /// `update(level, originPos, random, spreader, spreadVeins)`.
    pub fn update(&mut self, level: &mut dyn LevelAccessor, origin_pos: BlockPos, random: &mut dyn RandomSource, spreader: &SculkSpreader, spread_veins: bool) {
        if !self.should_update(&*level, origin_pos, spreader.is_world_generation) {
            return;
        }
        if self.update_delay > 0 {
            self.update_delay -= 1;
            return;
        }
        let mut current_state = level.get_block_state(self.pos);
        let mut sculk_behaviour = get_block_behaviour(current_state.block());
        if spread_veins && sculk_behaviour.attempt_spread_vein(level, self.pos, current_state, self.facings.as_deref(), spreader.is_world_generation()) {
            if sculk_behaviour.can_change_block_state_on_spread() {
                current_state = level.get_block_state(self.pos);
                sculk_behaviour = get_block_behaviour(current_state.block());
            }
            level.play_sound(None, self.pos, SoundEvent::BLOCK_SCULK_SPREAD, SoundSource::Blocks, 1.0, 1.0);
        }

        self.charge = sculk_behaviour.attempt_use_charge(self, level, origin_pos, random, spreader, spread_veins);
        if self.charge <= 0 {
            sculk_behaviour.on_discharged(level, current_state, self.pos, random);
        } else {
            let transfer_pos = get_valid_movement_pos(level, self.pos, random, origin_pos, spreader);
            if let Some(transfer_pos) = transfer_pos {
                sculk_behaviour.on_discharged(level, current_state, self.pos, random);
                self.pos = transfer_pos;
                current_state = level.get_block_state(transfer_pos);
            } else if spreader.is_world_generation() {
                sculk_behaviour.on_discharged(level, current_state, self.pos, random);
                self.charge = 0;
                return;
            }

            if sculk_behaviour::sculk_behaviour_of(current_state.block()).is_some() {
                self.facings = Some(multiface_block::available_faces(current_state));
            }

            self.decay_delay = sculk_behaviour.update_decay_delay(self.decay_delay);
            self.update_delay = i32::from(sculk_behaviour.get_sculk_spread_delay());
        }
    }

    fn merge_with(&mut self, other: &mut ChargeCursor) {
        self.charge += other.charge;
        other.charge = 0;
        self.update_delay = self.update_delay.min(other.update_delay);
    }
}

/// `getBlockBehaviour(state)`: the block's `SculkBehaviour` or `SculkBehaviour.DEFAULT`.
fn get_block_behaviour(block: mc_data::block::Block) -> &'static dyn SculkBehaviour {
    sculk_behaviour::sculk_behaviour_of(block).unwrap_or(&sculk_behaviour::DEFAULT)
}

fn get_valid_movement_pos(
    level: &dyn LevelAccessor,
    pos: BlockPos,
    random: &mut dyn RandomSource,
    origin_pos: BlockPos,
    spreader: &SculkSpreader,
) -> Option<BlockPos> {
    let mut sculk_position = pos;
    let mut offsets: Vec<Vec3i> = NON_CORNER_NEIGHBOURS.clone();
    crate::world::level::levelgen::feature::tree::shuffle(&mut offsets, random);

    for offset in offsets {
        let neighbour = pos.offset_vec(&offset);
        if can_move_to_pos(origin_pos, neighbour, spreader) {
            let transferee = level.get_block_state(neighbour);
            if sculk_behaviour::sculk_behaviour_of(transferee.block()).is_some() && is_movement_unobstructed(level, pos, neighbour) {
                sculk_position = neighbour;
                if sculk_vein_block::has_substrate_access(level, transferee, neighbour) {
                    break;
                }
            }
        }
    }
    if sculk_position == pos { None } else { Some(sculk_position) }
}

fn can_move_to_pos(origin: BlockPos, target: BlockPos, spreader: &SculkSpreader) -> bool {
    if !spreader.is_world_generation() {
        true
    } else {
        let distance_sq = mth::square_i32(origin.x() - target.x()) + mth::square_i32(origin.z() - target.z());
        distance_sq <= 144
    }
}

fn is_movement_unobstructed(level: &dyn LevelAccessor, from: BlockPos, to: BlockPos) -> bool {
    if from.dist_manhattan(&to) == 1 {
        return true;
    }
    let delta = to.subtract(&from);
    let direction_x = if delta.x() < 0 { Direction::West } else { Direction::East };
    let direction_y = if delta.y() < 0 { Direction::Down } else { Direction::Up };
    let direction_z = if delta.z() < 0 { Direction::North } else { Direction::South };
    if delta.x() == 0 {
        is_unobstructed(level, from, direction_y) || is_unobstructed(level, from, direction_z)
    } else if delta.y() == 0 {
        is_unobstructed(level, from, direction_x) || is_unobstructed(level, from, direction_z)
    } else {
        is_unobstructed(level, from, direction_x) || is_unobstructed(level, from, direction_y)
    }
}

fn is_unobstructed(level: &dyn LevelAccessor, from: BlockPos, direction: Direction) -> bool {
    let test_pos = from.relative(direction);
    !level.get_block_state(test_pos).is_face_sturdy_at(level, test_pos, direction.get_opposite())
}
