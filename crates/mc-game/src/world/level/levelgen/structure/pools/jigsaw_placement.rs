//! `JigsawPlacement`: assembles the pieces of a jigsaw structure from its start pool (`addPieces`) with the `Placer`'s
//! free-space tracking.

use super::alias::PoolAliasLookup;
use super::dimension_padding::DimensionPadding;
use super::jigsaw_junction::JigsawJunction;
use super::structure_pool_element::StructurePoolElement;
use super::structure_template_pool::{Projection, StructureTemplatePool, empty_pool_id};
use crate::world::level::levelgen::codec::*;
use crate::world::level::levelgen::env::GenEnv;
use crate::world::level::levelgen::holder::Holder;
use crate::world::level::levelgen::registries::TypedRegistry;
use crate::world::level::levelgen::structure::pool_element_structure_piece::PoolElementStructurePiece;
use crate::world::level::levelgen::structure::structure::{GenerationContext, GenerationStub};
use crate::world::level::levelgen::structure::templatesystem::LiquidSettings;
use crate::world::level::levelgen::structure::templatesystem::structure_template::{JigsawBlockInfo, JointType, jigsaw_front_facing};
use mc_core::bounding_box::BoundingBox;
use mc_core::direction::Direction;
use mc_core::ident::Identifier;
use mc_core::phys::AABB;
use mc_core::phys::shapes::{BooleanOp, Shapes, VoxelShape};
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_core::rotation::Rotation;
use mc_world::chunk::LevelHeight;
use mc_world::heightmap::HeightmapType;
use serde_json::Value;
use std::cell::RefCell;
use std::collections::{BTreeMap, VecDeque};
use std::rc::Rc;
use std::sync::Arc;

const UNSET_HEIGHT: i32 = i32::MIN;

/// `JigsawStructure.MaxDistance(horizontal, vertical)`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MaxDistance {
    pub horizontal: i32,
    pub vertical: i32,
}

/// `JigsawStructure.MAX_TOTAL_STRUCTURE_RANGE`.
pub const MAX_TOTAL_STRUCTURE_RANGE: i32 = 128;
/// `DimensionType.Y_SIZE`.
const Y_SIZE: i32 = (1 << 12) - 32;

impl MaxDistance {
    /// `new MaxDistance(value)`.
    pub const fn new(value: i32) -> MaxDistance {
        MaxDistance { horizontal: value, vertical: value }
    }

    /// `MaxDistance.CODEC`: `Codec.either(FULL_CODEC, HORIZONTAL_VALUE_CODEC)`.
    pub fn from_json(value: &Value) -> LoadResult<MaxDistance> {
        let first = Fields::new(value).and_then(|f| {
            Ok(MaxDistance {
                horizontal: f.required("horizontal", |v| parse_int_range(v, 1, MAX_TOTAL_STRUCTURE_RANGE))?,
                vertical: f.optional("vertical", |v| parse_int_range(v, 1, Y_SIZE))?.unwrap_or(Y_SIZE),
            })
        });
        let first = match first {
            Ok(distance) => return Ok(distance),
            Err(e) => e,
        };
        match parse_int_range(value, 1, MAX_TOTAL_STRUCTURE_RANGE) {
            Ok(horizontal) => Ok(MaxDistance::new(horizontal)),
            Err(second) => Err(LoadError::new(format!("Failed to parse either. First: {first}; Second: {second}"))),
        }
    }
}

/// `JigsawBlock.getTopFacing(state)`.
fn jigsaw_top_facing(state: mc_data::block::BlockState) -> Direction {
    let name = state.get_property_str("orientation").expect("jigsaw orientation");
    mc_core::direction::FrontAndTop::by_name(name).expect("valid jigsaw orientation").top()
}

/// `JigsawBlock.canAttach(source, target)`.
pub fn can_attach(source: &JigsawBlockInfo, target: &JigsawBlockInfo) -> bool {
    let source_front = jigsaw_front_facing(source.state);
    let target_front = jigsaw_front_facing(target.state);
    let source_top = jigsaw_top_facing(source.state);
    let target_top = jigsaw_top_facing(target.state);
    let rollable = source.joint_type == JointType::Rollable;
    source_front == target_front.get_opposite() && (rollable || source_top == target_top) && (target.name.is_none() || target.name.as_ref() == Some(&source.target))
}

/// `AABB.of(boundingBox)`.
fn aabb_of(bb: &BoundingBox) -> AABB {
    AABB::of_box(bb.min_x(), bb.min_y(), bb.min_z(), bb.max_x(), bb.max_y(), bb.max_z())
}

/// `addPieces(context, startPool, startJigsaw, maxDepth, position, doExpansionHack, projectStartToHeightmap,
/// maxDistanceFromCenter, poolAliasLookup, dimensionPadding, liquidSettings)`.
#[allow(clippy::too_many_arguments)]
pub fn add_pieces<'a>(
    context: &mut GenerationContext<'_>,
    start_pool: &Holder<StructureTemplatePool>,
    start_jigsaw: Option<&Identifier>,
    max_depth: i32,
    position: BlockPos,
    do_expansion_hack: bool,
    project_start_to_heightmap: Option<HeightmapType>,
    max_distance_from_center: MaxDistance,
    pool_alias_lookup: PoolAliasLookup,
    dimension_padding: DimensionPadding,
    liquid_settings: LiquidSettings,
) -> Option<GenerationStub<'a>> {
    let env = context.env;
    let manager = &env.structure_template_manager;
    let height_accessor = context.height_accessor;
    let pools = &env.structures.template_pools;
    let center_rotation = Rotation::get_random(&mut context.random);
    let center_pool: Arc<StructureTemplatePool> = start_pool
        .id()
        .and_then(|key| pools.get(&pool_alias_lookup.lookup(key)))
        .map_or_else(|| start_pool.value().clone(), |h| h.value().clone());
    let center_element = center_pool.get_random_template(&mut context.random);
    if center_element.is_empty_element() {
        return None;
    }
    let anchored_position = match start_jigsaw {
        Some(target_jigsaw_id) => match get_random_named_jigsaw(&*center_element, target_jigsaw_id, position, center_rotation, manager, &mut context.random) {
            Some(anchor) => anchor,
            None => {
                log::error!(
                    "No starting jigsaw {target_jigsaw_id} found in start pool {}",
                    start_pool.id().map_or_else(|| "<unregistered>".to_owned(), ToString::to_string)
                );
                return None;
            }
        },
        None => position,
    };
    let local_anchor_position = anchored_position.subtract(&position);
    let adjusted_position = position.subtract(&local_anchor_position);
    let mut center_piece = PoolElementStructurePiece::new(
        manager.clone(),
        center_element.clone(),
        adjusted_position,
        center_element.get_ground_level_delta(),
        center_rotation,
        center_element.get_bounding_box(manager, adjusted_position, center_rotation),
        liquid_settings,
    );
    let bb = crate::world::level::levelgen::structure::piece::StructurePiece::get_bounding_box(&center_piece);
    let center_x = (bb.max_x().wrapping_add(bb.min_x())) / 2;
    let center_z = (bb.max_z().wrapping_add(bb.min_z())) / 2;
    let bottom_y = match project_start_to_heightmap {
        None => adjusted_position.y(),
        Some(heightmap) => {
            if !context.could_structure_exist_in_column(center_x, center_z, height_accessor.min_y, height_accessor.max_y()) {
                return None;
            }
            position.y().wrapping_add(env.generator.get_first_free_height(center_x, center_z, heightmap, height_accessor, &env.random_state))
        }
    };
    let old_absolute_ground_y = bb.min_y().wrapping_add(center_piece.get_ground_level_delta());
    crate::world::level::levelgen::structure::piece::StructurePiece::move_by(&mut center_piece, 0, bottom_y.wrapping_sub(old_absolute_ground_y), 0);
    let center_bb = crate::world::level::levelgen::structure::piece::StructurePiece::get_bounding_box(&center_piece);
    if is_start_too_close_to_world_height_limits(height_accessor, &dimension_padding, &center_bb) {
        log::debug!("Center piece {center_element} with bounding box {center_bb:?} does not fit dimension padding {dimension_padding:?}");
        return None;
    }
    let center_y = bottom_y.wrapping_add(local_anchor_position.y());
    Some(GenerationStub::new(BlockPos::new(center_x, center_y, center_z), move |builder, context| {
        let mut pieces = vec![center_piece];
        if max_depth > 0 {
            let aabb = AABB::new(
                (center_x - max_distance_from_center.horizontal) as f64,
                (center_y - max_distance_from_center.vertical).max(height_accessor.min_y + dimension_padding.bottom) as f64,
                (center_z - max_distance_from_center.horizontal) as f64,
                (center_x + max_distance_from_center.horizontal + 1) as f64,
                (center_y + max_distance_from_center.vertical + 1).min(height_accessor.max_y() + 1 - dimension_padding.top) as f64,
                (center_z + max_distance_from_center.horizontal + 1) as f64,
            );
            let shape = Shapes::join(&Shapes::create(aabb), &Shapes::create(aabb_of(&center_bb)), BooleanOp::ONLY_FIRST);
            add_pieces_inner(context.env, max_depth, do_expansion_hack, height_accessor, &mut context.random, &mut pieces, shape, &pool_alias_lookup, liquid_settings);
            for piece in pieces {
                builder.add_piece(Box::new(piece));
            }
        }
    }))
}

/// `isStartTooCloseToWorldHeightLimits(heightAccessor, dimensionPadding, centerPieceBb)`.
fn is_start_too_close_to_world_height_limits(height_accessor: LevelHeight, dimension_padding: &DimensionPadding, center_piece_bb: &BoundingBox) -> bool {
    if dimension_padding.is_zero_constant() {
        return false;
    }
    let min_y_with_padding = height_accessor.min_y + dimension_padding.bottom;
    let max_y_with_padding = height_accessor.max_y() - dimension_padding.top;
    center_piece_bb.min_y() < min_y_with_padding || center_piece_bb.max_y() > max_y_with_padding
}

/// `getRandomNamedJigsaw(element, targetJigsawId, position, rotation, structureTemplateManager, random)`.
fn get_random_named_jigsaw(
    element: &dyn StructurePoolElement,
    target_jigsaw_id: &Identifier,
    position: BlockPos,
    rotation: Rotation,
    manager: &crate::world::level::levelgen::structure::templatesystem::StructureTemplateManager,
    random: &mut dyn RandomSource,
) -> Option<BlockPos> {
    element.get_shuffled_jigsaw_blocks(manager, position, rotation, random).into_iter().find(|jigsaw| jigsaw.name.as_ref() == Some(target_jigsaw_id)).map(|jigsaw| jigsaw.pos)
}

/// `addPieces(randomState, maxDepth, doExpansionHack, chunkGenerator, structureTemplateManager, heightAccessor, random,
/// pools, centerPiece, pieces, shape, poolAliasLookup, liquidSettings)`: the `pieces` list already holds the center piece.
#[allow(clippy::too_many_arguments)]
fn add_pieces_inner(
    env: &GenEnv,
    max_depth: i32,
    do_expansion_hack: bool,
    height_accessor: LevelHeight,
    random: &mut dyn RandomSource,
    pieces: &mut Vec<PoolElementStructurePiece>,
    shape: VoxelShape,
    pool_alias_lookup: &PoolAliasLookup,
    liquid_settings: LiquidSettings,
) {
    let mut placer = Placer { pools: &env.structures.template_pools, max_depth, env, pieces, random, placing: SequencedPriorityQueue::default() };
    placer.try_placing_children(0, &Rc::new(RefCell::new(Some(shape))), 0, do_expansion_hack, height_accessor, pool_alias_lookup, liquid_settings);
    while let Some(state) = placer.placing.next() {
        placer.try_placing_children(state.piece, &state.free, state.depth, do_expansion_hack, height_accessor, pool_alias_lookup, liquid_settings);
    }
}

/// `MutableObject<VoxelShape>`: shared and replaced in place by every piece placed against it (`null` until set).
type FreeShape = Rc<RefCell<Option<VoxelShape>>>;

/// `JigsawPlacement.PieceState(piece, free, depth)`; the piece is an index into the placer's list.
struct PieceState {
    piece: usize,
    free: FreeShape,
    depth: i32,
}

/// `SequencedPriorityIterator`: highest priority first, FIFO within a priority.
#[derive(Default)]
struct SequencedPriorityQueue {
    queues: BTreeMap<i32, VecDeque<PieceState>>,
}

impl SequencedPriorityQueue {
    fn add(&mut self, data: PieceState, priority: i32) {
        self.queues.entry(priority).or_default().push_back(data);
    }

    fn next(&mut self) -> Option<PieceState> {
        let mut entry = self.queues.last_entry()?;
        let result = entry.get_mut().pop_front();
        if entry.get().is_empty() {
            entry.remove();
        }
        result
    }
}

struct Placer<'a> {
    pools: &'a TypedRegistry<StructureTemplatePool>,
    max_depth: i32,
    env: &'a GenEnv,
    pieces: &'a mut Vec<PoolElementStructurePiece>,
    random: &'a mut dyn RandomSource,
    placing: SequencedPriorityQueue,
}

impl Placer<'_> {
    #[allow(clippy::too_many_arguments)]
    fn try_placing_children(
        &mut self,
        source_piece_index: usize,
        context_free: &FreeShape,
        depth: i32,
        do_expansion_hack: bool,
        height_accessor: LevelHeight,
        pool_alias_lookup: &PoolAliasLookup,
        liquid_settings: LiquidSettings,
    ) {
        use crate::world::level::levelgen::structure::piece::StructurePiece;
        let manager = &self.env.structure_template_manager;
        let (source_element, source_box_position, source_rotation, source_bb, source_ground_level_delta) = {
            let piece = &self.pieces[source_piece_index];
            (piece.get_element().clone(), piece.get_position(), piece.get_rotation(), piece.get_bounding_box(), piece.get_ground_level_delta())
        };
        let source_projection = source_element.get_projection();
        let source_rigid = source_projection == Projection::Rigid;
        let source_free: FreeShape = Rc::new(RefCell::new(None));
        let source_box_y = source_bb.min_y();

        'source_jigsaws: for source_jigsaw in source_element.get_shuffled_jigsaw_blocks(manager, source_box_position, source_rotation, self.random) {
            let source_direction = jigsaw_front_facing(source_jigsaw.state);
            let source_jigsaw_pos = source_jigsaw.pos;
            let target_jigsaw_pos = source_jigsaw_pos.relative(source_direction);
            let source_jigsaw_local_y = source_jigsaw_pos.y().wrapping_sub(source_box_y);
            let mut source_jigsaw_base_height = UNSET_HEIGHT;
            let pool_name = pool_alias_lookup.lookup(&source_jigsaw.pool);
            let Some(target_pool) = self.pools.get(&pool_name) else {
                continue;
            };
            let is_empty_pool = |holder: &Holder<StructureTemplatePool>| holder.id() == Some(&empty_pool_id());
            if target_pool.value().size() == 0 && !is_empty_pool(target_pool) {
                continue;
            }
            let fallback = target_pool.value().get_fallback();
            if fallback.value().size() == 0 && !is_empty_pool(fallback) {
                continue;
            }
            let attach_inside_source = source_bb.is_inside(&target_jigsaw_pos);
            let children_free = if attach_inside_source {
                if source_free.borrow().is_none() {
                    *source_free.borrow_mut() = Some(Shapes::create(aabb_of(&source_bb)));
                }
                &source_free
            } else {
                context_free
            };

            let mut target_pieces: Vec<Arc<dyn StructurePoolElement>> = Vec::new();
            if depth != self.max_depth {
                target_pieces.extend(target_pool.value().get_shuffled_templates(self.random));
            }
            target_pieces.extend(fallback.value().get_shuffled_templates(self.random));
            let placement_priority = source_jigsaw.placement_priority;

            for target_element in target_pieces {
                if target_element.is_empty_element() {
                    break;
                }
                for target_rotation in Rotation::get_shuffled(self.random) {
                    let target_jigsaws = target_element.get_shuffled_jigsaw_blocks(manager, BlockPos::ZERO, target_rotation, self.random);
                    let hack_box = target_element.get_bounding_box(manager, BlockPos::ZERO, target_rotation);
                    let expand_to = if do_expansion_hack && hack_box.get_y_span() <= 16 {
                        target_jigsaws
                            .iter()
                            .map(|target_jigsaw| {
                                if !hack_box.is_inside(&target_jigsaw.pos.relative(jigsaw_front_facing(target_jigsaw.state))) {
                                    return 0;
                                }
                                let child_pool_name = pool_alias_lookup.lookup(&target_jigsaw.pool);
                                let child_pool = self.pools.get(&child_pool_name);
                                let child_pool_size = child_pool.map_or(0, |p| p.value().get_max_size(manager));
                                let child_fallback_size = child_pool.map_or(0, |p| p.value().get_fallback().value().get_max_size(manager));
                                child_pool_size.max(child_fallback_size)
                            })
                            .max()
                            .unwrap_or(0)
                    } else {
                        0
                    };

                    for target_jigsaw in &target_jigsaws {
                        if !can_attach(&source_jigsaw, target_jigsaw) {
                            continue;
                        }
                        let target_jigsaw_local_pos = target_jigsaw.pos;
                        let raw_target_box_pos = target_jigsaw_pos.subtract(&target_jigsaw_local_pos);
                        let raw_target_bb = target_element.get_bounding_box(manager, raw_target_box_pos, target_rotation);
                        let raw_target_y = raw_target_bb.min_y();
                        let target_projection = target_element.get_projection();
                        let target_rigid = target_projection == Projection::Rigid;
                        let target_jigsaw_local_y = target_jigsaw_local_pos.y();
                        let delta_y = source_jigsaw_local_y.wrapping_sub(target_jigsaw_local_y).wrapping_add(jigsaw_front_facing(source_jigsaw.state).get_step_y());
                        let target_box_y = if source_rigid && target_rigid {
                            source_box_y.wrapping_add(delta_y)
                        } else {
                            if source_jigsaw_base_height == UNSET_HEIGHT {
                                source_jigsaw_base_height = self.first_free_height(source_jigsaw_pos, height_accessor);
                            }
                            source_jigsaw_base_height.wrapping_sub(target_jigsaw_local_y)
                        };
                        let y_offset = target_box_y.wrapping_sub(raw_target_y);
                        let mut target_bb = raw_target_bb.moved(0, y_offset, 0);
                        let target_box_position = raw_target_box_pos.offset(0, y_offset, 0);
                        if expand_to > 0 {
                            let new_size = (expand_to + 1).max(target_bb.max_y() - target_bb.min_y());
                            target_bb.encapsulate_pos(&BlockPos::new(target_bb.min_x(), target_bb.min_y() + new_size, target_bb.min_z()));
                        }

                        let free = children_free.borrow().clone().expect("free shape is set");
                        if Shapes::join_is_not_empty(&free, &Shapes::create(aabb_of(&target_bb).deflate_scalar(0.25)), BooleanOp::ONLY_SECOND) {
                            continue;
                        }
                        *children_free.borrow_mut() = Some(Shapes::join_unoptimized(&free, &Shapes::create(aabb_of(&target_bb)), BooleanOp::ONLY_FIRST));
                        let target_ground_level_delta =
                            if target_rigid { source_ground_level_delta.wrapping_sub(delta_y) } else { target_element.get_ground_level_delta() };
                        let mut target_piece = PoolElementStructurePiece::new(
                            manager.clone(),
                            target_element.clone(),
                            target_box_position,
                            target_ground_level_delta,
                            target_rotation,
                            target_bb,
                            liquid_settings,
                        );
                        let junction_y = if source_rigid {
                            source_box_y.wrapping_add(source_jigsaw_local_y)
                        } else if target_rigid {
                            target_box_y.wrapping_add(target_jigsaw_local_y)
                        } else {
                            if source_jigsaw_base_height == UNSET_HEIGHT {
                                source_jigsaw_base_height = self.first_free_height(source_jigsaw_pos, height_accessor);
                            }
                            source_jigsaw_base_height.wrapping_add(delta_y / 2)
                        };
                        self.pieces[source_piece_index].add_junction(JigsawJunction::new(
                            target_jigsaw_pos.x(),
                            junction_y.wrapping_sub(source_jigsaw_local_y).wrapping_add(source_ground_level_delta),
                            target_jigsaw_pos.z(),
                            delta_y,
                            target_projection,
                        ));
                        target_piece.add_junction(JigsawJunction::new(
                            source_jigsaw_pos.x(),
                            junction_y.wrapping_sub(target_jigsaw_local_y).wrapping_add(target_ground_level_delta),
                            source_jigsaw_pos.z(),
                            delta_y.wrapping_neg(),
                            source_projection,
                        ));
                        self.pieces.push(target_piece);
                        if depth + 1 <= self.max_depth {
                            let state = PieceState { piece: self.pieces.len() - 1, free: children_free.clone(), depth: depth + 1 };
                            self.placing.add(state, placement_priority);
                        }
                        continue 'source_jigsaws;
                    }
                }
            }
        }
    }

    /// `chunkGenerator.getFirstFreeHeight(x, z, WORLD_SURFACE_WG, heightAccessor, randomState)`.
    fn first_free_height(&self, pos: BlockPos, height_accessor: LevelHeight) -> i32 {
        self.env.generator.get_first_free_height(pos.x(), pos.z(), HeightmapType::WorldSurfaceWg, height_accessor, &self.env.random_state)
    }
}
