//! `net.minecraft.world.entity.EntityFluidInteraction`: per-fluid heights / eye containment of an entity's fluid
//! interaction box and the water/lava current accumulators (`FLUIDS_WITH_CURRENT`).

use super::entity::Entity;
use crate::server::level::ServerLevel;
use crate::world::level::BlockGetter;
use crate::world::level::block::BlockStateExt;
use crate::world::level::material::FluidStateExt;
use mc_core::math::mth;
use mc_core::phys::Vec3;
use mc_core::pos::BlockPos;
use mc_data::block::BlockState;
use mc_data::fluid::Fluid;
use mc_data::tags::{self, TagSet};
use std::sync::LazyLock;

/// `FluidTags.WATER`.
pub const TAG_WATER: &str = "water";
/// `FluidTags.LAVA`.
pub const TAG_LAVA: &str = "lava";
/// `FluidTags.ENTITY_FLOATABLE`.
pub const TAG_ENTITY_FLOATABLE: &str = "entity_floatable";

static WATER_TAG: LazyLock<&'static TagSet<Fluid>> =
    LazyLock::new(|| tags::get::<Fluid>(TAG_WATER).expect("fluid tag water"));
static LAVA_TAG: LazyLock<&'static TagSet<Fluid>> =
    LazyLock::new(|| tags::get::<Fluid>(TAG_LAVA).expect("fluid tag lava"));

fn water_tag() -> &'static TagSet<Fluid> {
    *WATER_TAG
}

fn lava_tag() -> &'static TagSet<Fluid> {
    *LAVA_TAG
}

/// `fluid.is(tag)`.
pub fn fluid_is_tag(fluid: Fluid, tag: &str) -> bool {
    tags::get::<Fluid>(tag).is_some_and(|t| t.contains(fluid))
}

/// `EntityFluidInteraction.CurrentAccumulator`.
#[derive(Clone, Debug)]
struct CurrentAccumulator {
    height: f64,
    accumulated_current: Vec3,
    current_count: i32,
}

impl Default for CurrentAccumulator {
    fn default() -> Self {
        Self {
            height: 0.0,
            accumulated_current: Vec3::ZERO,
            current_count: 0,
        }
    }
}

impl CurrentAccumulator {
    fn reset(&mut self) {
        *self = Self::default();
    }

    fn accumulate(&mut self, flow: Vec3) {
        self.accumulated_current = self.accumulated_current.add(flow);
        self.current_count += 1;
    }

    /// `applyTo(entity, scale)`; `is_player` selects the averaged impulse.
    fn impulse(&self, is_player: bool, old_movement: Vec3, scale: f64) -> Option<Vec3> {
        if self.current_count == 0 || self.accumulated_current.length_sqr() < f64::from(1.0E-5f32) {
            return None;
        }
        let mut impulse = if is_player {
            self.accumulated_current
                .scale(1.0 / f64::from(self.current_count))
        } else {
            self.accumulated_current.normalize()
        };
        impulse = impulse.scale(scale);
        if old_movement.x.abs() < 0.003
            && old_movement.z.abs() < 0.003
            && impulse.length() < 0.0045000000000000005
        {
            impulse = impulse.normalize().scale(0.0045000000000000005);
        }
        Some(impulse)
    }
}

/// `EntityFluidInteraction.Tracker`.
#[derive(Clone, Debug)]
struct Tracker {
    fluid_type: Fluid,
    height: f64,
    eyes_inside: bool,
}

/// `EntityFluidInteraction` (accumulators for the water and lava tags only, like `FLUIDS_WITH_CURRENT`).
#[derive(Clone, Debug, Default)]
pub struct EntityFluidInteraction {
    fluid_trackers: Vec<Tracker>,
    water: CurrentAccumulator,
    lava: CurrentAccumulator,
}

/// `Mth.floor`-section of a block coordinate (`SectionPos.blockToSectionCoord`).
fn section_coord(v: i32) -> i32 {
    v >> 4
}

/// `EntityFluidInteraction.hasFluidAndLoaded`.
fn has_fluid_and_loaded(
    level: &ServerLevel,
    x0: i32,
    y0: i32,
    z0: i32,
    x1: i32,
    y1: i32,
    z1: i32,
) -> bool {
    let (section_x0, section_y0, section_z0) =
        (section_coord(x0), section_coord(y0), section_coord(z0));
    let (section_x1, section_y1, section_z1) =
        (section_coord(x1), section_coord(y1), section_coord(z1));
    let mut has_fluid = false;
    for chunk_z in section_z0..=section_z1 {
        for chunk_x in section_x0..=section_x1 {
            let Some(chunk) = level.chunk_now(chunk_x, chunk_z) else {
                return false;
            };
            let sections = &chunk.data.access.sections;
            for section_y in section_y0..=section_y1 {
                let index = chunk
                    .data
                    .access
                    .level_height
                    .section_index_from_section_y(section_y);
                if index >= 0 && (index as usize) < sections.len() {
                    has_fluid |= sections[index as usize].has_fluid();
                }
            }
        }
    }
    has_fluid
}

impl EntityFluidInteraction {
    /// `update(entity, ignoreCurrent)`: returns whether the box touches any fluid.
    pub fn update(
        &mut self,
        entity: &dyn Entity,
        level: &ServerLevel,
        ignore_current: bool,
    ) -> bool {
        self.fluid_trackers.retain_mut(|t| {
            if t.height == 0.0 && !t.eyes_inside {
                false
            } else {
                t.height = 0.0;
                t.eyes_inside = false;
                true
            }
        });
        self.water.reset();
        self.lava.reset();
        let Some(bx) = entity.get_fluid_interaction_box(level) else {
            return false;
        };
        let (x0, y0, z0) = (
            mth::floor_f64(bx.min_x),
            mth::floor_f64(bx.min_y),
            mth::floor_f64(bx.min_z),
        );
        let (x1, y1, z1) = (
            mth::ceil_f64(bx.max_x) - 1,
            mth::ceil_f64(bx.max_y) - 1,
            mth::ceil_f64(bx.max_z) - 1,
        );
        if !has_fluid_and_loaded(level, x0 - 1, y0, z0 - 1, x1 + 1, y1, z1 + 1) {
            return false;
        }
        let b = entity.base();
        let entity_y = b.bounding_box().min_y;
        let (eye_block_x, eye_block_z) = (b.get_block_x(), b.get_block_z());
        let eye_y = b.get_eye_y();
        let mut last_fluid_type: Option<Fluid> = None;
        // index of the tracker of `last_fluid_type`, and which accumulator applies to it
        let mut tracker_index = 0usize;
        let mut current: Option<bool> = None; // Some(true) = water, Some(false) = lava
        for x in x0..=x1 {
            for y in y0..=y1 {
                for z in z0..=z1 {
                    let pos = BlockPos::new(x, y, z);
                    let fluid_state = level.get_fluid_state(pos);
                    if fluid_state.is_empty() {
                        continue;
                    }
                    let fluid_bottom = f64::from(y);
                    let fluid_top = fluid_bottom + f64::from(fluid_state.get_height(level, pos));
                    if fluid_top < bx.min_y {
                        continue;
                    }
                    let fluid_type = fluid_state.get_type();
                    if last_fluid_type != Some(fluid_type) {
                        last_fluid_type = Some(fluid_type);
                        tracker_index = match self
                            .fluid_trackers
                            .iter()
                            .position(|t| t.fluid_type == fluid_type)
                        {
                            Some(i) => i,
                            None => {
                                self.fluid_trackers.push(Tracker {
                                    fluid_type,
                                    height: 0.0,
                                    eyes_inside: false,
                                });
                                self.fluid_trackers.len() - 1
                            }
                        };
                        if !ignore_current {
                            current = if water_tag().contains(fluid_type) {
                                Some(true)
                            } else if lava_tag().contains(fluid_type) {
                                Some(false)
                            } else {
                                None
                            };
                        }
                    }
                    let tracker = &mut self.fluid_trackers[tracker_index];
                    if x == eye_block_x && z == eye_block_z && eye_y >= fluid_bottom {
                        let fluid_top_for_camera = fluid_bottom
                            + f64::from(get_height_for_camera(fluid_state, level, pos));
                        if eye_y <= fluid_top_for_camera {
                            tracker.eyes_inside = true;
                        }
                    }
                    tracker.height = tracker.height.max(fluid_top - entity_y);
                    if let Some(is_water) = current {
                        let mut flow = fluid_state.get_flow(level, pos);
                        let acc = if is_water {
                            &mut self.water
                        } else {
                            &mut self.lava
                        };
                        acc.height = tracker.height.max(acc.height);
                        if acc.height < 0.4 {
                            flow = flow.scale(acc.height);
                        }
                        acc.accumulate(flow);
                    }
                }
            }
        }
        last_fluid_type.is_some()
    }

    /// `applyCurrentTo(fluid, entity, scale)`: the impulse to add to the entity's delta movement, if any.
    pub fn current_impulse(
        &self,
        water: bool,
        is_player: bool,
        old_movement: Vec3,
        scale: f64,
    ) -> Option<Vec3> {
        (if water { &self.water } else { &self.lava }).impulse(is_player, old_movement, scale)
    }

    /// `getFluidHeight(fluid)`.
    pub fn get_fluid_height(&self, tag: &str) -> f64 {
        let Some(set) = tags::get::<Fluid>(tag) else {
            return 0.0;
        };
        let mut height = 0.0;
        for tracker in &self.fluid_trackers {
            if tracker.height > height && set.contains(tracker.fluid_type) {
                height = tracker.height;
            }
        }
        height
    }

    /// `isInFluid(fluid)`.
    pub fn is_in_fluid(&self, tag: &str) -> bool {
        self.get_fluid_height(tag) > 0.0
    }

    /// `isEyeInFluid(fluid)`.
    pub fn is_eye_in_fluid(&self, tag: &str) -> bool {
        let Some(set) = tags::get::<Fluid>(tag) else {
            return false;
        };
        self.fluid_trackers
            .iter()
            .any(|t| t.eyes_inside && set.contains(t.fluid_type))
    }

    /// Fast path of `getFluidHeight(FluidTags.WATER)` / `LAVA` (the cached `[water, lava]` mirror of `EntityData`).
    pub fn water_lava_heights(&self) -> [f64; 2] {
        let max_of = |set: &TagSet<Fluid>| {
            self.fluid_trackers
                .iter()
                .filter(|t| set.contains(t.fluid_type))
                .fold(0.0f64, |h, t| if t.height > h { t.height } else { h })
        };
        [max_of(water_tag()), max_of(lava_tag())]
    }

    /// Fast path of `isEyeInFluid(FluidTags.WATER)` / `LAVA`.
    pub fn water_lava_eyes(&self) -> [bool; 2] {
        let any = |set: &TagSet<Fluid>| {
            self.fluid_trackers
                .iter()
                .any(|t| t.eyes_inside && set.contains(t.fluid_type))
        };
        [any(water_tag()), any(lava_tag())]
    }
}

/// `FluidState.getHeightForCamera(level, pos)`.
fn get_height_for_camera(
    fluid_state: mc_data::fluid::FluidState,
    level: &ServerLevel,
    pos: BlockPos,
) -> f32 {
    use crate::world::level::block::BlockStateExt;
    use mc_core::direction::Direction;
    if fluid_state.is_source() {
        let above = pos.above();
        let state: BlockState = level.get_block_state(above);
        if state.is_face_sturdy_at(level, above, Direction::Down) {
            return 1.0;
        }
    }
    fluid_state.get_height(level, pos)
}
