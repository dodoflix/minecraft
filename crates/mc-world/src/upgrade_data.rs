//! `net.minecraft.world.level.chunk.UpgradeData`: data attached to chunks converted from older versions (block
//! indices whose shapes must be re-evaluated, which neighbouring chunk sides need an update pass, and the ticks that
//! overreached into this chunk), plus the `upgrade(chunk)` pass itself with vanilla's `BlockFixers`.
//!
//! The pass needs the chunk and its `Level`; both are reached through the [`UpgradeChunk`] trait, which lists exactly
//! the calls `UpgradeData.java` makes on `LevelChunk` / `LevelAccessor` (block behavior such as `BlockState.updateShape`
//! stays with the implementor). Vanilla code that would throw an exception (`setValue` with a value the property cannot
//! hold, `List.get` out of range, `PalettedContainer.get` out of range) returns an [`UpgradeError`] instead.

use crate::ticks::{SavedTick, TickPriority};
use mc_core::direction::{Direction, Direction8};
use mc_core::pos::block_pos::BlockPos;
use mc_core::pos::chunk_pos::ChunkPos;
use mc_core::pos::section_pos::SectionPos;
use mc_data::block::{BLOCK_COUNT, Block, BlockState};
use mc_data::enums::ChestType;
use mc_data::fluid::Fluid;
use mc_data::props;
use mc_nbt::CompoundTag;
use std::collections::HashSet;
use std::fmt;
use std::sync::LazyLock;

const TAG_INDICES: &str = "Indices";

/// An exception vanilla's `UpgradeData` code would throw (the message is the Java exception's).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UpgradeError(pub String);

impl fmt::Display for UpgradeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for UpgradeError {}

/// What `UpgradeData` needs from the `LevelChunk` being upgraded and its level (`LevelAccessor`).
pub trait UpgradeChunk {
    /// `chunk.getPos()`.
    fn get_pos(&self) -> ChunkPos;
    /// `level.getMinY()`.
    fn get_min_y(&self) -> i32;
    /// `level.getMaxY()`.
    fn get_max_y(&self) -> i32;
    /// `chunk.getSection(section_index).getStates().get(index)`; `None` where that call throws (`index` outside 0..4096).
    fn get_section_state(&self, section_index: usize, index: i32) -> Option<BlockState>;
    /// `level.getBlockState(pos)`.
    fn get_block_state(&self, pos: &BlockPos) -> BlockState;
    /// `level.getFluidState(pos).getType()`.
    fn get_fluid_type(&self, pos: &BlockPos) -> Fluid;
    /// `level.setBlock(pos, state, flags, update_limit)` (the 3-argument overload passes 512).
    fn set_block(
        &mut self,
        pos: &BlockPos,
        state: BlockState,
        flags: i32,
        update_limit: i32,
    ) -> bool;
    /// `level.destroyBlock(pos, drop_resources, null, update_limit)`.
    fn destroy_block(&mut self, pos: &BlockPos, drop_resources: bool, update_limit: i32) -> bool;
    /// `state.updateShape(level, level, pos, direction, neighbour_pos, neighbour_state, level.getRandom())`, i.e. the
    /// block's own shape update (this is the `DEFAULT` fixer; it may schedule ticks and consume level randomness).
    fn update_shape(
        &mut self,
        state: BlockState,
        direction: Direction,
        pos: &BlockPos,
        neighbour_pos: &BlockPos,
        neighbour_state: BlockState,
    ) -> BlockState;
    /// `ChestBlockEntity.swapContents(a, b)` when the block entities at both positions are chests (else nothing).
    fn swap_chest_contents(&mut self, a: &BlockPos, b: &BlockPos);
    /// `level.scheduleTick(pos, block, delay, priority)`.
    fn schedule_block_tick(
        &mut self,
        pos: &BlockPos,
        block: Block,
        delay: i32,
        priority: TickPriority,
    );
    /// `level.scheduleTick(pos, fluid, delay, priority)`.
    fn schedule_fluid_tick(
        &mut self,
        pos: &BlockPos,
        fluid: Fluid,
        delay: i32,
        priority: TickPriority,
    );
}

/// `Block.updateOrDestroy(state, new_state, level, pos, flags)` on a server level.
pub fn update_or_destroy(
    state: BlockState,
    new_state: BlockState,
    level: &mut impl UpgradeChunk,
    pos: &BlockPos,
    flags: i32,
) {
    const UPDATE_LIMIT: i32 = 512;
    if new_state != state {
        if new_state.is_air() {
            level.destroy_block(pos, flags & 32 == 0, UPDATE_LIMIT);
        } else {
            level.set_block(pos, new_state, flags & !32, UPDATE_LIMIT);
        }
    }
}

/// `LEAVES` fixer's thread-local `queue`: per-distance sets of leaf positions, drained by `processChunk`.
#[derive(Default)]
pub struct LeavesQueue {
    queue: Vec<HashSet<BlockPos>>,
}

/// `UpgradeData.BlockFixers` (the `BlockFixer` implementations).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BlockFixer {
    /// Blocks that must keep their state.
    Blacklist,
    Default,
    Chest,
    Leaves,
    StemBlock,
}

/// `UpgradeData.MAP` (indexed by block id; unlisted blocks use `DEFAULT`).
static FIXER_MAP: LazyLock<Vec<BlockFixer>> = LazyLock::new(|| {
    let mut map = vec![BlockFixer::Default; BLOCK_COUNT];
    let mut put = |fixer: BlockFixer, blocks: &[Block]| {
        for b in blocks {
            map[b.id() as usize] = fixer;
        }
    };
    put(
        BlockFixer::Blacklist,
        &[
            Block::OBSERVER,
            Block::NETHER_PORTAL,
            Block::WHITE_CONCRETE_POWDER,
            Block::ORANGE_CONCRETE_POWDER,
            Block::MAGENTA_CONCRETE_POWDER,
            Block::LIGHT_BLUE_CONCRETE_POWDER,
            Block::YELLOW_CONCRETE_POWDER,
            Block::LIME_CONCRETE_POWDER,
            Block::PINK_CONCRETE_POWDER,
            Block::GRAY_CONCRETE_POWDER,
            Block::LIGHT_GRAY_CONCRETE_POWDER,
            Block::CYAN_CONCRETE_POWDER,
            Block::PURPLE_CONCRETE_POWDER,
            Block::BLUE_CONCRETE_POWDER,
            Block::BROWN_CONCRETE_POWDER,
            Block::GREEN_CONCRETE_POWDER,
            Block::RED_CONCRETE_POWDER,
            Block::BLACK_CONCRETE_POWDER,
            Block::ANVIL,
            Block::CHIPPED_ANVIL,
            Block::DAMAGED_ANVIL,
            Block::DRAGON_EGG,
            Block::GRAVEL,
            Block::SAND,
            Block::RED_SAND,
            Block::OAK_SIGN,
            Block::SPRUCE_SIGN,
            Block::BIRCH_SIGN,
            Block::ACACIA_SIGN,
            Block::CHERRY_SIGN,
            Block::JUNGLE_SIGN,
            Block::DARK_OAK_SIGN,
            Block::PALE_OAK_SIGN,
            Block::POPLAR_SIGN,
            Block::OAK_WALL_SIGN,
            Block::SPRUCE_WALL_SIGN,
            Block::BIRCH_WALL_SIGN,
            Block::ACACIA_WALL_SIGN,
            Block::JUNGLE_WALL_SIGN,
            Block::DARK_OAK_WALL_SIGN,
            Block::PALE_OAK_WALL_SIGN,
            Block::POPLAR_WALL_SIGN,
            Block::OAK_HANGING_SIGN,
            Block::SPRUCE_HANGING_SIGN,
            Block::BIRCH_HANGING_SIGN,
            Block::ACACIA_HANGING_SIGN,
            Block::JUNGLE_HANGING_SIGN,
            Block::DARK_OAK_HANGING_SIGN,
            Block::PALE_OAK_HANGING_SIGN,
            Block::POPLAR_HANGING_SIGN,
            Block::OAK_WALL_HANGING_SIGN,
            Block::SPRUCE_WALL_HANGING_SIGN,
            Block::BIRCH_WALL_HANGING_SIGN,
            Block::ACACIA_WALL_HANGING_SIGN,
            Block::JUNGLE_WALL_HANGING_SIGN,
            Block::DARK_OAK_WALL_HANGING_SIGN,
            Block::PALE_OAK_WALL_HANGING_SIGN,
            Block::POPLAR_WALL_HANGING_SIGN,
        ],
    );
    put(BlockFixer::Chest, &[Block::CHEST, Block::TRAPPED_CHEST]);
    put(
        BlockFixer::Leaves,
        &[
            Block::ACACIA_LEAVES,
            Block::CHERRY_LEAVES,
            Block::BIRCH_LEAVES,
            Block::PALE_OAK_LEAVES,
            Block::RED_POPLAR_LEAVES,
            Block::ORANGE_POPLAR_LEAVES,
            Block::YELLOW_POPLAR_LEAVES,
            Block::DARK_OAK_LEAVES,
            Block::JUNGLE_LEAVES,
            Block::OAK_LEAVES,
            Block::SPRUCE_LEAVES,
        ],
    );
    put(
        BlockFixer::StemBlock,
        &[Block::MELON_STEM, Block::PUMPKIN_STEM],
    );
    map
});

fn chest_type_opposite(t: ChestType) -> ChestType {
    match t {
        ChestType::Single => ChestType::Single,
        ChestType::Left => ChestType::Right,
        ChestType::Right => ChestType::Left,
    }
}

fn no_property(what: &str, state: BlockState) -> UpgradeError {
    UpgradeError(format!(
        "java.lang.IllegalArgumentException: {what} on {state}"
    ))
}

impl BlockFixer {
    /// `MAP.getOrDefault(block, DEFAULT)`.
    pub fn for_block(block: Block) -> BlockFixer {
        FIXER_MAP[block.id() as usize]
    }

    /// `updateShape(state, direction, neighbour, level, pos, neighbourPos)`.
    pub fn update_shape(
        self,
        state: BlockState,
        direction: Direction,
        neighbour: BlockState,
        level: &mut impl UpgradeChunk,
        pos: &BlockPos,
        neighbour_pos: &BlockPos,
        leaves: &mut LeavesQueue,
    ) -> Result<BlockState, UpgradeError> {
        match self {
            BlockFixer::Blacklist => Ok(state),
            BlockFixer::Default => Ok(level.update_shape(
                state,
                direction,
                pos,
                neighbour_pos,
                level.get_block_state(neighbour_pos),
            )),
            BlockFixer::Chest => {
                if neighbour.block() == state.block()
                    && direction.get_axis().is_horizontal()
                    && state.get(props::CHEST_TYPE) == ChestType::Single
                    && neighbour.get(props::CHEST_TYPE) == ChestType::Single
                {
                    let facing = state.get(props::HORIZONTAL_FACING);
                    if direction.get_axis() != facing.get_axis()
                        && facing == neighbour.get(props::HORIZONTAL_FACING)
                    {
                        let new_type = if direction == facing.get_clockwise() {
                            ChestType::Left
                        } else {
                            ChestType::Right
                        };
                        level.set_block(
                            neighbour_pos,
                            neighbour.set(props::CHEST_TYPE, chest_type_opposite(new_type)),
                            18,
                            512,
                        );
                        if facing == Direction::North || facing == Direction::East {
                            level.swap_chest_contents(pos, neighbour_pos);
                        }
                        return Ok(state.set(props::CHEST_TYPE, new_type));
                    }
                }
                Ok(state)
            }
            BlockFixer::Leaves => {
                let new_state = level.update_shape(
                    state,
                    direction,
                    pos,
                    neighbour_pos,
                    level.get_block_state(neighbour_pos),
                );
                if state != new_state {
                    let distance = new_state
                        .try_get(props::DISTANCE)
                        .ok_or_else(|| no_property("Cannot get property distance", new_state))?;
                    if leaves.queue.is_empty() {
                        leaves.queue = (0..7).map(|_| HashSet::new()).collect();
                    }
                    let set = usize::try_from(distance)
                        .ok()
                        .and_then(|d| leaves.queue.get_mut(d))
                        .ok_or_else(|| UpgradeError(format!("java.lang.IndexOutOfBoundsException: Index {distance} out of bounds for length 7")))?;
                    set.insert(*pos);
                }
                Ok(state)
            }
            BlockFixer::StemBlock => {
                if state.get(props::AGE_7) == 7 {
                    let pumpkin = state.block() == Block::PUMPKIN_STEM;
                    let fruit = if pumpkin {
                        Block::PUMPKIN
                    } else {
                        Block::MELON
                    };
                    if neighbour.block() == fruit {
                        let attached = if pumpkin {
                            Block::ATTACHED_PUMPKIN_STEM
                        } else {
                            Block::ATTACHED_MELON_STEM
                        };
                        let default = attached.default_state();
                        // `setValue` throws for UP/DOWN (HORIZONTAL_FACING cannot hold them)
                        if !props::HORIZONTAL_FACING.values().contains(&direction) {
                            return Err(no_property(
                                &format!("Cannot set property facing to {}", direction.get_name()),
                                default,
                            ));
                        }
                        return Ok(default.set(props::HORIZONTAL_FACING, direction));
                    }
                }
                Ok(state)
            }
        }
    }

    /// `processChunk(level)` (only the `LEAVES` fixer does anything; it is vanilla's one `CHUNKY_FIXERS` entry).
    fn process_chunk(
        self,
        level: &mut impl UpgradeChunk,
        leaves: &mut LeavesQueue,
    ) -> Result<(), UpgradeError> {
        if self != BlockFixer::Leaves {
            return Ok(());
        }
        let size = leaves.queue.len();
        for neighbor_distance in 2..size {
            let current_distance = neighbor_distance - 1;
            let set: Vec<BlockPos> = leaves.queue[current_distance].iter().copied().collect();
            for pos in set {
                let state = level.get_block_state(&pos);
                let distance = state
                    .try_get(props::DISTANCE)
                    .ok_or_else(|| no_property("Cannot get property distance", state))?;
                if distance >= current_distance as i32 {
                    let new_state = state
                        .try_set(props::DISTANCE, current_distance as i32)
                        .ok_or_else(|| no_property("Cannot set property distance", state))?;
                    level.set_block(&pos, new_state, 18, 512);
                    if neighbor_distance != 7 {
                        for direction in Direction::ALL {
                            let neighbor_pos = pos.relative(direction);
                            let neighbor = level.get_block_state(&neighbor_pos);
                            if neighbor.has_property(props::DISTANCE)
                                && distance > neighbor_distance as i32
                            {
                                leaves.queue[neighbor_distance].insert(neighbor_pos);
                            }
                        }
                    }
                }
            }
        }
        leaves.queue.clear();
        Ok(())
    }
}

/// `UpgradeData.updateState`: the fixer registered for the state's block applied to one neighbour direction.
fn update_state(
    state: BlockState,
    direction: Direction,
    level: &mut impl UpgradeChunk,
    pos: &BlockPos,
    neighbour_pos: &BlockPos,
    leaves: &mut LeavesQueue,
) -> Result<BlockState, UpgradeError> {
    let neighbour = level.get_block_state(neighbour_pos);
    BlockFixer::for_block(state.block()).update_shape(
        state,
        direction,
        neighbour,
        level,
        pos,
        neighbour_pos,
        leaves,
    )
}

/// `UpgradeData`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UpgradeData {
    /// `EnumSet<Direction8>`: bit `ordinal`.
    sides: u8,
    neighbor_block_ticks: Vec<SavedTick<Block>>,
    neighbor_fluid_ticks: Vec<SavedTick<Fluid>>,
    /// Per section (bottom to top): packed in-section coordinates (`y << 8 | z << 4 | x`) needing a shape update.
    index: Vec<Option<Vec<i32>>>,
}

impl UpgradeData {
    /// `UpgradeData.EMPTY` (`EmptyBlockGetter`: no sections).
    pub const EMPTY: UpgradeData = UpgradeData {
        sides: 0,
        neighbor_block_ticks: Vec::new(),
        neighbor_fluid_ticks: Vec::new(),
        index: Vec::new(),
    };

    pub fn empty() -> UpgradeData {
        Self::EMPTY
    }

    /// `UpgradeData(CompoundTag, LevelHeightAccessor)`; the section count is `max_section_y - min_section_y + 1`.
    pub fn parse(tag: &CompoundTag, min_section_y: i32, max_section_y: i32) -> UpgradeData {
        let sections = (max_section_y - min_section_y + 1).max(0) as usize;
        let mut index: Vec<Option<Vec<i32>>> = vec![None; sections];
        if let Some(indices) = tag.get_compound(TAG_INDICES) {
            for (i, slot) in index.iter_mut().enumerate() {
                *slot = indices.get_int_array(&i.to_string()).map(<[i32]>::to_vec);
            }
        }
        let side_int = tag.get_int_or("Sides", 0);
        let sides = (0..Direction8::ALL.len())
            .filter(|d| side_int & (1 << d) != 0)
            .fold(0u8, |acc, d| acc | (1 << d));
        UpgradeData {
            sides,
            // `byNameCodec().orElse(Blocks.AIR / Fluids.EMPTY)`: unknown types decode to air/empty
            neighbor_block_ticks: SavedTick::read_list_or(tag, "neighbor_block_ticks", Block::AIR),
            neighbor_fluid_ticks: SavedTick::read_list_or(
                tag,
                "neighbor_fluid_ticks",
                Fluid::EMPTY,
            ),
            index,
        }
    }

    /// `isEmpty()`: no index arrays (empty ones count as present) and no pending sides.
    pub fn is_empty(&self) -> bool {
        self.index.iter().all(Option::is_none) && self.sides == 0
    }

    /// `write()`.
    pub fn write(&self) -> CompoundTag {
        let mut tag = CompoundTag::new();
        let mut indices = CompoundTag::new();
        for (i, slot) in self.index.iter().enumerate() {
            if let Some(ints) = slot.as_ref().filter(|v| !v.is_empty()) {
                indices.put_int_array(i.to_string(), ints.clone());
            }
        }
        if !indices.is_empty() {
            tag.put(TAG_INDICES, indices);
        }
        tag.put_byte("Sides", self.sides as i8);
        if !self.neighbor_block_ticks.is_empty() {
            tag.put(
                "neighbor_block_ticks",
                SavedTick::write_list(&self.neighbor_block_ticks),
            );
        }
        if !self.neighbor_fluid_ticks.is_empty() {
            tag.put(
                "neighbor_fluid_ticks",
                SavedTick::write_list(&self.neighbor_fluid_ticks),
            );
        }
        tag
    }

    /// `copy()` (deep copy).
    pub fn copy(&self) -> UpgradeData {
        self.clone()
    }

    /// Pending sides as `Direction8`s (`sides`).
    pub fn sides(&self) -> Vec<Direction8> {
        Direction8::ALL
            .into_iter()
            .filter(|d| self.sides & (1 << *d as u8) != 0)
            .collect()
    }

    pub fn neighbor_block_ticks(&self) -> &[SavedTick<Block>] {
        &self.neighbor_block_ticks
    }

    pub fn neighbor_fluid_ticks(&self) -> &[SavedTick<Fluid>] {
        &self.neighbor_fluid_ticks
    }

    /// `upgrade(LevelChunk)`: re-evaluate the indexed blocks, then every pending side, then schedule the neighbour
    /// ticks, then run the chunky fixers (leaf distances).
    pub fn upgrade(&mut self, chunk: &mut impl UpgradeChunk) -> Result<(), UpgradeError> {
        let mut leaves = LeavesQueue::default();
        self.upgrade_inside(chunk, &mut leaves)?;
        for direction8 in Direction8::ALL {
            self.upgrade_sides(chunk, direction8, &mut leaves)?;
        }
        for tick in &self.neighbor_block_ticks {
            let block = if tick.ty == Block::AIR {
                chunk.get_block_state(&tick.pos).block()
            } else {
                tick.ty
            };
            chunk.schedule_block_tick(&tick.pos, block, tick.delay, tick.priority);
        }
        for tick in &self.neighbor_fluid_ticks {
            let fluid = if tick.ty == Fluid::EMPTY {
                chunk.get_fluid_type(&tick.pos)
            } else {
                tick.ty
            };
            chunk.schedule_fluid_tick(&tick.pos, fluid, tick.delay, tick.priority);
        }
        BlockFixer::Leaves.process_chunk(chunk, &mut leaves)
    }

    fn upgrade_sides(
        &mut self,
        chunk: &mut impl UpgradeChunk,
        direction8: Direction8,
        leaves: &mut LeavesQueue,
    ) -> Result<(), UpgradeError> {
        let bit = 1u8 << direction8 as u8;
        if self.sides & bit == 0 {
            return Ok(());
        }
        self.sides &= !bit;
        let directions = direction8.get_directions();
        let east = directions.contains(&Direction::East);
        let west = directions.contains(&Direction::West);
        let south = directions.contains(&Direction::South);
        let north = directions.contains(&Direction::North);
        let singular = directions.len() == 1;
        let chunk_pos = chunk.get_pos();
        let (base_x, base_z) = (chunk_pos.get_min_block_x(), chunk_pos.get_min_block_z());
        let x_whole = !singular || (!north && !south);
        let z_whole = !singular || (!east && !west);
        let x_edge = if west { 0 } else { 15 };
        let z_edge = if north { 0 } else { 15 };
        let min_x = base_x + if x_whole { x_edge } else { 1 };
        let max_x = base_x + if x_whole { x_edge } else { 14 };
        let min_z = base_z + if z_whole { z_edge } else { 1 };
        let max_z = base_z + if z_whole { z_edge } else { 14 };
        for pos in BlockPos::between_closed_xyz(
            min_x,
            chunk.get_min_y(),
            min_z,
            max_x,
            chunk.get_max_y(),
            max_z,
        ) {
            let state = chunk.get_block_state(&pos);
            let mut new_state = state;
            for direction in Direction::ALL {
                let neighbour_pos = pos.relative(direction);
                new_state =
                    update_state(new_state, direction, chunk, &pos, &neighbour_pos, leaves)?;
            }
            update_or_destroy(state, new_state, chunk, &pos, 18);
        }
        Ok(())
    }

    fn upgrade_inside(
        &mut self,
        chunk: &mut impl UpgradeChunk,
        leaves: &mut LeavesQueue,
    ) -> Result<(), UpgradeError> {
        let chunk_pos = chunk.get_pos();
        let min_section_y = SectionPos::block_to_section_coord_i32(chunk.get_min_y());
        for section_index in 0..self.index.len() {
            let upgrade_index = self.index[section_index].take();
            let Some(upgrade_index) = upgrade_index.filter(|v| !v.is_empty()) else {
                continue;
            };
            let bottom_y_in_section = (min_section_y + section_index as i32) << 4;
            for coordinate in upgrade_index {
                let x = coordinate & 15;
                let y = coordinate >> 8 & 15;
                let z = coordinate >> 4 & 15;
                let pos = BlockPos::new(
                    chunk_pos.get_min_block_x() + x,
                    bottom_y_in_section + y,
                    chunk_pos.get_min_block_z() + z,
                );
                let state = chunk
                    .get_section_state(section_index, coordinate)
                    .ok_or_else(|| UpgradeError(format!("java.lang.IllegalArgumentException: The value {coordinate} is not in the specified inclusive range of 0 to 4095")))?;
                let mut new_state = state;
                for direction in Direction::ALL {
                    let neighbour_pos = pos.relative(direction);
                    if SectionPos::block_to_section_coord_i32(pos.x()) == chunk_pos.x
                        && SectionPos::block_to_section_coord_i32(pos.z()) == chunk_pos.z
                    {
                        new_state = update_state(
                            new_state,
                            direction,
                            chunk,
                            &pos,
                            &neighbour_pos,
                            leaves,
                        )?;
                    }
                }
                update_or_destroy(state, new_state, chunk, &pos, 18);
            }
        }
        // vanilla logs "Discarding update data for section ..." for leftovers here; all slots were just consumed
        self.index.iter_mut().for_each(|s| *s = None);
        Ok(())
    }
}
