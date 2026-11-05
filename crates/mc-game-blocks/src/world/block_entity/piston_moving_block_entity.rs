//! `PistonMovingBlockEntity` (`block.piston` package): the block that is in transit between two positions while a piston
//! pushes/pulls it (`minecraft:moving_piston`). Moves entities standing in/on it, becomes the moved block when done.

use mc_game_core::block_entity_base;
use mc_game_core::world::block_entity::{BLOCK_ENTITY_TYPES, BlockEntity, BlockEntityData, BlockEntityRegistration};
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::entity::{Entity, MoverType};
use mc_game_core::world::level::block::piston::{entity_push_reaction, get_movement_area};
use mc_game_core::world::level::block::{BlockStateExt, helpers};
use mc_game_core::world::level::{BlockGetter, LevelAccessor};
use linkme::distributed_slice;
use mc_core::direction::{Axis, Direction};
use mc_core::math::mth;
use mc_core::phys::shapes::{Shapes, VoxelShape};
use mc_core::phys::{AABB, Vec3};
use mc_core::pos::BlockPos;
use mc_data::BlockEntityType;
use mc_data::block::{Block, BlockState};
use mc_data::enums::{PistonType, PushReaction};
use mc_data::props;
use mc_nbt::CompoundTag;
use std::cell::{Cell, RefCell};

thread_local! {
    /// `PistonMovingBlockEntity.NOCLIP`.
    static NOCLIP: Cell<Option<Direction>> = const { Cell::new(None) };
    /// The block entity whose `tick` is running is out of its chunk (take-out pattern), but `getCollisionShape` of its
    /// `MovingPistonBlock` must still see it while it moves entities: its shape data is parked here meanwhile.
    static TICKING: RefCell<Option<(BlockPos, PistonShapeData)>> = const { RefCell::new(None) };
}

/// The fields `PistonMovingBlockEntity.getCollisionShape` reads.
#[derive(Clone, Copy)]
pub struct PistonShapeData {
    moved_state: BlockState,
    direction: Direction,
    extending: bool,
    is_source_piston: bool,
    progress: f32,
}

impl PistonShapeData {
    fn get_extended_progress(&self, progress: f32) -> f32 {
        if self.extending { progress - 1.0 } else { 1.0 - progress }
    }

    fn get_movement_direction(&self) -> Direction {
        if self.extending { self.direction } else { self.direction.get_opposite() }
    }

    /// `getCollisionShape(level, pos)`.
    pub fn get_collision_shape(&self, level: &dyn BlockGetter, pos: BlockPos) -> VoxelShape {
        let piston_head_shape = if !self.extending && self.is_source_piston && helpers::is_instance_of(self.moved_state, "PistonBaseBlock") {
            self.moved_state.set(props::EXTENDED, true).get_collision_shape(level, pos)
        } else {
            Shapes::empty()
        };

        let no_clip_direction = NOCLIP.with(Cell::get);
        if f64::from(self.progress) < 1.0 && no_clip_direction == Some(self.get_movement_direction()) {
            return piston_head_shape;
        }
        let block_state = if self.is_source_piston {
            Block::PISTON_HEAD
                .default_state()
                .set(props::FACING, self.direction)
                .set(props::SHORT, self.extending != (1.0f32 - self.progress < 0.25f32))
        } else {
            self.moved_state
        };
        let extended_progress = self.get_extended_progress(self.progress);
        let dx = f64::from(self.direction.get_step_x() as f32 * extended_progress);
        let dy = f64::from(self.direction.get_step_y() as f32 * extended_progress);
        let dz = f64::from(self.direction.get_step_z() as f32 * extended_progress);
        Shapes::or(&piston_head_shape, &block_state.get_collision_shape(level, pos).move_(dx, dy, dz))
    }
}

/// `MovingPistonBlock.getBlockEntity(level, pos).getCollisionShape(level, pos)` (empty without a block entity).
pub fn moving_piston_collision_shape(level: &dyn BlockGetter, pos: BlockPos) -> VoxelShape {
    if let Some(be) = level.get_block_entity(pos).and_then(|be| be.as_any().downcast_ref::<PistonMovingBlockEntity>()) {
        return be.shape_data().get_collision_shape(level, pos);
    }
    let ticking = TICKING.with(|t| t.borrow().filter(|(p, _)| *p == pos).map(|(_, d)| d));
    match ticking {
        Some(data) => data.get_collision_shape(level, pos),
        None => Shapes::empty(),
    }
}

pub struct PistonMovingBlockEntity {
    base: BlockEntityData,
    moved_state: BlockState,
    direction: Direction,
    extending: bool,
    is_source_piston: bool,
    progress: f32,
    progress_o: f32,
    last_ticked: i64,
}

impl PistonMovingBlockEntity {
    /// `PistonMovingBlockEntity(pos, state)` (loading: `loadAdditional` fills the rest).
    pub fn new(pos: BlockPos, state: BlockState) -> Self {
        Self {
            base: BlockEntityData::new(BlockEntityType::PISTON, pos, state),
            moved_state: Block::AIR.default_state(),
            direction: Direction::Down,
            extending: false,
            is_source_piston: false,
            progress: 0.0,
            progress_o: 0.0,
            last_ticked: 0,
        }
    }

    /// `MovingPistonBlock.newMovingBlockEntity(pos, state, movedState, direction, extending, isSourcePiston)`.
    pub fn new_moving(pos: BlockPos, state: BlockState, moved_state: BlockState, direction: Direction, extending: bool, is_source_piston: bool) -> Self {
        Self { moved_state, direction, extending, is_source_piston, ..Self::new(pos, state) }
    }

    fn shape_data(&self) -> PistonShapeData {
        PistonShapeData { moved_state: self.moved_state, direction: self.direction, extending: self.extending, is_source_piston: self.is_source_piston, progress: self.progress }
    }

    pub fn is_extending(&self) -> bool {
        self.extending
    }

    pub fn get_direction(&self) -> Direction {
        self.direction
    }

    pub fn is_source_piston(&self) -> bool {
        self.is_source_piston
    }

    /// `getProgress(a)`.
    pub fn get_progress(&self, a: f32) -> f32 {
        let a = if a > 1.0 { 1.0 } else { a };
        mth::lerp_f32(a, self.progress_o, self.progress)
    }

    fn get_extended_progress(&self, progress: f32) -> f32 {
        self.shape_data().get_extended_progress(progress)
    }

    pub fn get_moved_state(&self) -> BlockState {
        self.moved_state
    }

    pub fn get_last_ticked(&self) -> i64 {
        self.last_ticked
    }

    pub fn get_movement_direction(&self) -> Direction {
        self.shape_data().get_movement_direction()
    }

    pub fn get_push_direction(&self) -> Direction {
        self.get_movement_direction()
    }

    /// `getCollisionRelatedBlockState()`.
    fn get_collision_related_block_state(&self) -> BlockState {
        if !self.extending && self.is_source_piston && helpers::is_instance_of(self.moved_state, "PistonBaseBlock") {
            Block::PISTON_HEAD
                .default_state()
                .set(props::SHORT, self.progress > 0.25)
                .set(props::PISTON_TYPE, if self.moved_state.is_block(Block::STICKY_PISTON) { PistonType::Sticky } else { PistonType::Default })
                .set(props::FACING, self.moved_state.get(props::FACING))
        } else {
            self.moved_state
        }
    }

    fn is_sticky_for_entities(&self) -> bool {
        self.moved_state.is_block(Block::HONEY_BLOCK)
    }

    /// `getCollisionShape(level, pos)`.
    pub fn get_collision_shape(&self, level: &dyn BlockGetter, pos: BlockPos) -> VoxelShape {
        self.shape_data().get_collision_shape(level, pos)
    }

    /// `finalTick()`: the block arrives immediately.
    pub fn final_tick(&mut self, level: &mut ServerLevel) {
        if self.progress_o < 1.0 {
            self.progress = 1.0;
            self.progress_o = self.progress;
            let pos = self.get_block_pos();
            level.remove_block_entity(pos);
            self.set_removed();
            if BlockGetter::get_block_state(level, pos).is_block(Block::MOVING_PISTON) {
                let new_state = if self.is_source_piston {
                    helpers::air()
                } else {
                    helpers::update_from_neighbour_shapes(self.moved_state, level, pos)
                };
                level.set_block_and_update(pos, new_state);
                let orientation = level.initial_orientation(Some(self.get_push_direction()), None);
                level.neighbor_changed(pos, new_state.block(), orientation);
            }
        }
    }

    /// `PistonMovingBlockEntity.tick(level, pos, state, entity)` (server side).
    fn tick(&mut self, level: &mut ServerLevel) {
        let pos = self.get_block_pos();
        self.last_ticked = level.game_time();
        self.progress_o = self.progress;
        if self.progress_o >= 1.0 {
            level.remove_block_entity(pos);
            self.set_removed();
            if BlockGetter::get_block_state(level, pos).is_block(Block::MOVING_PISTON) {
                let mut new_state = helpers::update_from_neighbour_shapes(self.moved_state, level, pos);
                if new_state.is_air() {
                    level.set_block_flags(pos, self.moved_state, 340);
                    helpers::update_or_destroy(self.moved_state, new_state, level, pos, 3);
                } else {
                    if new_state.has_property(props::WATERLOGGED) && new_state.get(props::WATERLOGGED) {
                        new_state = new_state.set(props::WATERLOGGED, false);
                    }
                    level.set_block_flags(pos, new_state, 67);
                    let orientation = level.initial_orientation(Some(self.get_push_direction()), None);
                    level.neighbor_changed(pos, new_state.block(), orientation);
                }
            }
        } else {
            let new_progress = self.progress + 0.5;
            // `getCollisionShape` of this block must still see this entity (it is out of its chunk while it ticks)
            TICKING.with(|t| *t.borrow_mut() = Some((pos, self.shape_data())));
            self.move_collided_entities(level, pos, new_progress);
            self.move_stuck_entities(level, pos, new_progress);
            TICKING.with(|t| *t.borrow_mut() = None);
            self.progress = new_progress;
            if self.progress >= 1.0 {
                self.progress = 1.0;
            }
        }
    }

    fn move_by_position_and_progress(&self, pos: BlockPos, aabb: &AABB) -> AABB {
        let current_position = self.get_extended_progress(self.progress);
        aabb.move_(
            f64::from(pos.x() as f32 + current_position * self.direction.get_step_x() as f32),
            f64::from(pos.y() as f32 + current_position * self.direction.get_step_y() as f32),
            f64::from(pos.z() as f32 + current_position * self.direction.get_step_z() as f32),
        )
    }

    /// `moveCollidedEntities(level, pos, newProgress, self)`.
    fn move_collided_entities(&self, level: &mut ServerLevel, pos: BlockPos, new_progress: f32) {
        let movement = self.get_movement_direction();
        let delta_progress = f64::from(new_progress - self.progress);
        let shape = self.get_collision_related_block_state().get_collision_shape(&*level, pos);
        if shape.is_empty() {
            return;
        }
        let aabb = self.move_by_position_and_progress(pos, &shape.bounds());
        let area = get_movement_area(&aabb, movement, delta_progress).minmax(&aabb);
        let entities = level.get_entities(None, &area, |e| !e.is_spectator());
        if entities.is_empty() {
            return;
        }
        let shape_aabbs = shape.to_aabbs();
        let cause_bounce = self.moved_state.is_block(Block::SLIME_BLOCK);

        for id in entities {
            level.with_entity(id, |level, entity| {
                if entity_push_reaction(&*entity) == PushReaction::IgnoreEntity {
                    return;
                }
                if cause_bounce && entity.can_simulate_movement() {
                    let delta_movement = entity.base().delta_movement();
                    let (mut dx, mut dy, mut dz) = (delta_movement.x, delta_movement.y, delta_movement.z);
                    match movement.get_axis() {
                        Axis::X => dx = f64::from(movement.get_step_x()),
                        Axis::Y => dy = f64::from(movement.get_step_y()),
                        Axis::Z => dz = f64::from(movement.get_step_z()),
                    }
                    entity.base_mut().set_delta_movement_xyz(dx, dy, dz);
                }

                let mut delta = 0.0f64;
                for shape_aabb in &shape_aabbs {
                    let moving_aabb = get_movement_area(&self.move_by_position_and_progress(pos, shape_aabb), movement, delta_progress);
                    let entity_aabb = entity.get_bounding_box();
                    if moving_aabb.intersects_aabb(&entity_aabb) {
                        delta = delta.max(get_movement(&moving_aabb, movement, &entity_aabb));
                        if delta >= delta_progress {
                            break;
                        }
                    }
                }

                if !(delta <= 0.0) {
                    delta = delta.min(delta_progress) + 0.01;
                    move_entity_by_piston(level, movement, entity, delta, movement);
                    if !self.extending && self.is_source_piston {
                        fix_entity_within_piston_base(level, pos, entity, movement, delta_progress);
                    }
                }
            });
        }
    }

    /// `moveStuckEntities(level, pos, newProgress, self)`.
    fn move_stuck_entities(&self, level: &mut ServerLevel, pos: BlockPos, new_progress: f32) {
        if !self.is_sticky_for_entities() {
            return;
        }
        let movement = self.get_movement_direction();
        if !movement.get_axis().is_horizontal() {
            return;
        }
        let sticky_top = self.moved_state.get_collision_shape(&*level, pos).max(Axis::Y);
        let aabb = self.move_by_position_and_progress(pos, &AABB::new(0.0, sticky_top, 0.0, 1.0, 1.500001, 1.0));
        let delta_progress = f64::from(new_progress - self.progress);
        let entities = level.get_entities(None, &aabb, |entity| matches_sticky_criteria(&aabb, entity, pos));
        for id in entities {
            level.with_entity(id, |level, entity| move_entity_by_piston(level, movement, entity, delta_progress, movement));
        }
    }
}

fn matches_sticky_criteria(aabb: &AABB, entity: &dyn Entity, pos: BlockPos) -> bool {
    entity_push_reaction(entity) == PushReaction::PushPull
        && entity.base().on_ground()
        && (entity.base().main_supporting_block_pos == Some(pos)
            || (entity.position().x >= aabb.min_x && entity.position().x <= aabb.max_x && entity.position().z >= aabb.min_z && entity.position().z <= aabb.max_z))
}

/// `getMovement(aabbToBeOutsideOf, movement, aabb)`.
fn get_movement(aabb_to_be_outside_of: &AABB, movement: Direction, aabb: &AABB) -> f64 {
    match movement {
        Direction::East => aabb_to_be_outside_of.max_x - aabb.min_x,
        Direction::West => aabb.max_x - aabb_to_be_outside_of.min_x,
        Direction::Up => aabb_to_be_outside_of.max_y - aabb.min_y,
        Direction::Down => aabb.max_y - aabb_to_be_outside_of.min_y,
        Direction::South => aabb_to_be_outside_of.max_z - aabb.min_z,
        Direction::North => aabb.max_z - aabb_to_be_outside_of.min_z,
    }
}

/// `moveEntityByPiston(pistonDirection, entity, delta, movement)`.
fn move_entity_by_piston(level: &mut ServerLevel, piston_direction: Direction, entity: &mut dyn Entity, delta: f64, movement: Direction) {
    NOCLIP.with(|n| n.set(Some(piston_direction)));
    let previous_pos = entity.position();
    entity.r#move(level, MoverType::Piston, Vec3::new(delta * f64::from(movement.get_step_x()), delta * f64::from(movement.get_step_y()), delta * f64::from(movement.get_step_z())));
    entity.apply_effects_from_blocks_between(level, previous_pos, entity.position());
    entity.base_mut().movement_this_tick.pop_back();
    NOCLIP.with(|n| n.set(None));
}

/// `fixEntityWithinPistonBase(pos, entity, direction, deltaProgress)`.
fn fix_entity_within_piston_base(level: &mut ServerLevel, pos: BlockPos, entity: &mut dyn Entity, direction: Direction, delta_progress: f64) {
    let entity_aabb = entity.get_bounding_box();
    let box_ = Shapes::block().bounds().move_block_pos(&pos);
    if entity_aabb.intersects_aabb(&box_) {
        let opposite = direction.get_opposite();
        let mut delta = get_movement(&box_, opposite, &entity_aabb) + 0.01;
        let delta_intersected = get_movement(&box_, opposite, &entity_aabb.intersect(&box_)) + 0.01;
        if (delta - delta_intersected).abs() < 0.01 {
            delta = delta.min(delta_progress) + 0.01;
            move_entity_by_piston(level, direction, entity, delta, opposite);
        }
    }
}

/// `input.read("blockState", BlockState.CODEC).orElse(AIR)`: bare block id or `{id, properties}`.
fn read_moved_state(tag: &CompoundTag) -> BlockState {
    use mc_game_core::world::level::levelgen::structure::templatesystem::structure_template::read_block_state;
    if let Some(name) = tag.get_string("blockState") {
        let mut compound = CompoundTag::new();
        compound.put_string("id", name);
        return read_block_state(&compound);
    }
    tag.get_compound("blockState").map(read_block_state).unwrap_or_else(helpers::air)
}

impl BlockEntity for PistonMovingBlockEntity {
    block_entity_base!();

    fn load_additional(&mut self, tag: &CompoundTag) {
        self.moved_state = read_moved_state(tag);
        self.direction = tag.get_byte("facing").map_or(Direction::Down, |facing| Direction::from_3d_data_value(i32::from(facing)));
        self.progress = tag.get_float_or("progress", 0.0);
        self.progress_o = self.progress;
        self.extending = tag.get_boolean_or("extending", false);
        self.is_source_piston = tag.get_boolean_or("source", false);
    }

    fn save_additional(&self, tag: &mut CompoundTag) {
        // `BlockState.CODEC`: the default state is written as the bare block id
        if self.moved_state == self.moved_state.block().default_state() {
            tag.put_string("blockState", self.moved_state.block().name());
        } else {
            tag.put("blockState", mc_game_core::world::level::levelgen::structure::templatesystem::structure_template::write_block_state(self.moved_state));
        }
        tag.put_byte("facing", self.direction.get_3d_data_value() as i8);
        tag.put_float("progress", self.progress_o);
        tag.put_boolean("extending", self.extending);
        tag.put_boolean("source", self.is_source_piston);
    }

    /// `getUpdateTag`: `saveCustomOnly` (there is no `getUpdatePacket`: clients get it with the chunk).
    fn get_update_tag(&self) -> CompoundTag {
        let mut tag = CompoundTag::new();
        self.save_additional(&mut tag);
        tag
    }

    fn is_ticking(&self) -> bool {
        true
    }

    fn server_tick(&mut self, level: &mut ServerLevel) {
        self.tick(level);
    }

    fn pre_remove_side_effects(&mut self, level: &mut ServerLevel, _pos: BlockPos, _state: BlockState) {
        self.final_tick(level);
    }
}

/// Seam of the crate split: these were inherent `ServerLevel` methods, which cannot be defined outside `mc-game-core`.
pub trait MovingPistonLevelExt {
    fn moving_piston_info(&self, pos: BlockPos) -> Option<MovingPistonInfo>;
    fn final_tick_moving_piston(&mut self, pos: BlockPos);
}

impl MovingPistonLevelExt for ServerLevel {
    /// `level.getBlockEntity(pos) instanceof PistonMovingBlockEntity`: its `(extending, direction, progress(0), lastTicked)`.
    fn moving_piston_info(&self, pos: BlockPos) -> Option<MovingPistonInfo> {
        let be = self.get_block_entity(pos)?.as_any().downcast_ref::<PistonMovingBlockEntity>()?;
        Some(MovingPistonInfo { extending: be.extending, direction: be.direction, progress: be.get_progress(0.0), last_ticked: be.last_ticked })
    }

    /// `pistonMovingBlockEntity.finalTick()` for the entity at `pos` (if it is one).
    fn final_tick_moving_piston(&mut self, pos: BlockPos) {
        self.with_block_entity(pos, |level, be| {
            if let Some(piston) = be.as_any_mut().downcast_mut::<PistonMovingBlockEntity>() {
                piston.final_tick(level);
            }
        });
    }
}

/// What `PistonBaseBlock` reads from a `PistonMovingBlockEntity`.
#[derive(Clone, Copy)]
pub struct MovingPistonInfo {
    pub extending: bool,
    pub direction: Direction,
    pub progress: f32,
    pub last_ticked: i64,
}

#[distributed_slice(BLOCK_ENTITY_TYPES)]
static PISTON: BlockEntityRegistration = BlockEntityRegistration { ty: BlockEntityType::PISTON, create: |pos, state| Box::new(PistonMovingBlockEntity::new(pos, state)) };
