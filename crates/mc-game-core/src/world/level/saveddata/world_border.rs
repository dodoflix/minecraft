//! `net.minecraft.world.level.border`: `WorldBorder` (saved as `dimensions/<d>/data/minecraft/world_border.dat`),
//! `WorldBorder.Settings`, `BorderStatus`.
//!
//! Listeners: vanilla keeps `BorderChangeListener`s that receive `(border, ...)` callbacks. A listener cannot borrow
//! the border while the border is being mutated, so every change is recorded as a [`BorderEvent`] (in vanilla's call
//! order) which the owner drains with [`WorldBorder::take_events`] right after the mutation.

use mc_core::ident::Identifier;
use mc_core::math::mth::{abs_max_f64, clamp_f64, lerp_f64};
use mc_core::phys::shapes::{BooleanOp, Shapes, VoxelShape};
use mc_core::phys::{AABB, Vec3};
use mc_core::pos::{BlockPos, ChunkPos};
use mc_nbt::{CompoundTag, Tag};

use super::{SavedData, SavedDataType};
use crate::util::codec::{self, DecodeResult};

/// `WorldBorder.MAX_SIZE` (`5.999997E7F` widened).
pub const MAX_SIZE: f64 = 5.999997E7_f32 as f64;
/// `WorldBorder.MAX_CENTER_COORDINATE`.
pub const MAX_CENTER_COORDINATE: f64 = 2.9999984E7;
/// `1.0E-5F` as used for the max-edge epsilon.
const EPSILON: f64 = 1.0E-5_f32 as f64;

/// `BorderStatus`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BorderStatus {
    Growing,
    Shrinking,
    Stationary,
}

impl BorderStatus {
    pub fn get_color(self) -> i32 {
        match self {
            BorderStatus::Growing => 4259712,
            BorderStatus::Shrinking => 16724016,
            BorderStatus::Stationary => 2138367,
        }
    }
}

/// One recorded `BorderChangeListener` call.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BorderEvent {
    SetSize(f64),
    LerpSize {
        from: f64,
        to: f64,
        ticks: i64,
        game_time: i64,
    },
    SetCenter {
        x: f64,
        z: f64,
    },
    SetWarningTime(i32),
    SetWarningBlocks(i32),
    SetDamagePerBlock(f64),
    SetSafeZone(f64),
}

/// `WorldBorder.Settings`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Settings {
    pub center_x: f64,
    pub center_z: f64,
    pub damage_per_block: f64,
    pub safe_zone: f64,
    pub warning_blocks: i32,
    pub warning_time: i32,
    pub size: f64,
    pub lerp_time: i64,
    pub lerp_target: f64,
}

impl Settings {
    pub const DEFAULT: Settings = Settings {
        center_x: 0.0,
        center_z: 0.0,
        damage_per_block: 0.2,
        safe_zone: 5.0,
        warning_blocks: 5,
        warning_time: 300,
        size: MAX_SIZE,
        lerp_time: 0,
        lerp_target: 0.0,
    };

    /// `Settings.CODEC` decode (`center_*` are `doubleRange(-2.9999984E7, 2.9999984E7)`; every field is required).
    pub fn decode(tag: &Tag) -> codec::DataResult<Settings> {
        let m = codec::compound(tag)?;
        let center = |name: &str| -> codec::DataResult<f64> {
            let v = codec::double(codec::field(m, name)?)?;
            if (-MAX_CENTER_COORDINATE..=MAX_CENTER_COORDINATE).contains(&v) {
                Ok(v)
            } else {
                Err(format!(
                    "Value {v} outside of range [-2.9999984E7:2.9999984E7]"
                ))
            }
        };
        Ok(Settings {
            center_x: center("center_x")?,
            center_z: center("center_z")?,
            damage_per_block: codec::double(codec::field(m, "damage_per_block")?)?,
            safe_zone: codec::double(codec::field(m, "safe_zone")?)?,
            warning_blocks: codec::int(codec::field(m, "warning_blocks")?)?,
            warning_time: codec::int(codec::field(m, "warning_time")?)?,
            size: codec::double(codec::field(m, "size")?)?,
            lerp_time: codec::long(codec::field(m, "lerp_time")?)?,
            lerp_target: codec::double(codec::field(m, "lerp_target")?)?,
        })
    }

    pub fn encode(&self) -> Tag {
        let mut m = CompoundTag::new();
        m.put("center_x", Tag::double(self.center_x));
        m.put("center_z", Tag::double(self.center_z));
        m.put("damage_per_block", Tag::double(self.damage_per_block));
        m.put("safe_zone", Tag::double(self.safe_zone));
        m.put_int("warning_blocks", self.warning_blocks);
        m.put_int("warning_time", self.warning_time);
        m.put("size", Tag::double(self.size));
        m.put_long("lerp_time", self.lerp_time);
        m.put("lerp_target", Tag::double(self.lerp_target));
        Tag::Compound(m)
    }
}

#[derive(Debug, Clone)]
enum BorderExtent {
    Static {
        size: f64,
        min_x: f64,
        min_z: f64,
        max_x: f64,
        max_z: f64,
        shape: VoxelShape,
    },
    Moving {
        from: f64,
        to: f64,
        lerp_end: i64,
        lerp_begin: i64,
        lerp_duration: f64,
        lerp_progress: i64,
        size: f64,
        previous_size: f64,
    },
}

fn border_shape(min_x: f64, min_z: f64, max_x: f64, max_z: f64) -> VoxelShape {
    Shapes::join(
        &Shapes::infinity(),
        &Shapes::box_(
            min_x.floor(),
            f64::NEG_INFINITY,
            min_z.floor(),
            max_x.ceil(),
            f64::INFINITY,
            max_z.ceil(),
        ),
        BooleanOp::ONLY_FIRST,
    )
}

/// `WorldBorder`.
#[derive(Debug, Clone)]
pub struct WorldBorder {
    dirty: bool,
    settings: Settings,
    initialized: bool,
    damage_per_block: f64,
    safe_zone: f64,
    warning_time: i32,
    warning_blocks: i32,
    center_x: f64,
    center_z: f64,
    absolute_max_size: i32,
    extent: BorderExtent,
    events: Vec<BorderEvent>,
}

impl SavedData for WorldBorder {
    fn is_dirty(&self) -> bool {
        self.dirty
    }
    fn set_dirty_to(&mut self, dirty: bool) {
        self.dirty = dirty;
    }
}

impl Default for WorldBorder {
    fn default() -> Self {
        WorldBorder::new(Settings::DEFAULT)
    }
}

impl WorldBorder {
    pub fn new(settings: Settings) -> WorldBorder {
        let mut border = WorldBorder {
            dirty: false,
            settings,
            initialized: false,
            damage_per_block: 0.2,
            safe_zone: 5.0,
            warning_time: 15,
            warning_blocks: 5,
            center_x: 0.0,
            center_z: 0.0,
            absolute_max_size: 29999984,
            extent: BorderExtent::Moving {
                from: 0.0,
                to: 0.0,
                lerp_end: 0,
                lerp_begin: 0,
                lerp_duration: 0.0,
                lerp_progress: 0,
                size: 0.0,
                previous_size: 0.0,
            },
            events: Vec::new(),
        };
        border.extent = border.static_extent(MAX_SIZE);
        border
    }

    /// `new StaticBorderExtent(size)` against the current center / absolute max size.
    fn static_extent(&self, size: f64) -> BorderExtent {
        let max = f64::from(self.absolute_max_size);
        let min_x = clamp_f64(self.center_x - size / 2.0, -max, max);
        let min_z = clamp_f64(self.center_z - size / 2.0, -max, max);
        let max_x = clamp_f64(self.center_x + size / 2.0, -max, max);
        let max_z = clamp_f64(self.center_z + size / 2.0, -max, max);
        BorderExtent::Static {
            size,
            min_x,
            min_z,
            max_x,
            max_z,
            shape: border_shape(min_x, min_z, max_x, max_z),
        }
    }

    fn moving_extent(from: f64, to: f64, duration: i64, game_time: i64) -> BorderExtent {
        let mut e = BorderExtent::Moving {
            from,
            to,
            lerp_duration: duration as f64,
            lerp_progress: duration,
            lerp_begin: game_time,
            lerp_end: game_time.wrapping_add(duration),
            size: 0.0,
            previous_size: 0.0,
        };
        if let BorderExtent::Moving {
            size,
            previous_size,
            ..
        } = &mut e
        {
            let s = Self::moving_size(from, to, duration as f64, duration);
            *size = s;
            *previous_size = s;
        }
        e
    }

    fn moving_size(from: f64, to: f64, lerp_duration: f64, lerp_progress: i64) -> f64 {
        let progress = (lerp_duration - lerp_progress as f64) / lerp_duration;
        if progress < 1.0 {
            lerp_f64(progress, from, to)
        } else {
            to
        }
    }

    /// `take_events`: the `BorderChangeListener` calls since the last drain, in order.
    pub fn take_events(&mut self) -> Vec<BorderEvent> {
        std::mem::take(&mut self.events)
    }

    // --- bounds ---

    pub fn is_within_bounds_block(&self, pos: &BlockPos) -> bool {
        self.is_within_bounds(f64::from(pos.x()), f64::from(pos.z()))
    }

    pub fn is_within_bounds_vec3(&self, pos: &Vec3) -> bool {
        self.is_within_bounds(pos.x, pos.z)
    }

    pub fn is_within_bounds_chunk(&self, pos: &ChunkPos) -> bool {
        self.is_within_bounds(
            f64::from(pos.get_min_block_x()),
            f64::from(pos.get_min_block_z()),
        ) && self.is_within_bounds(
            f64::from(pos.get_max_block_x()),
            f64::from(pos.get_max_block_z()),
        )
    }

    pub fn is_within_bounds_aabb(&self, aabb: &AABB) -> bool {
        self.is_within_bounds_box(
            aabb.min_x,
            aabb.min_z,
            aabb.max_x - EPSILON,
            aabb.max_z - EPSILON,
        )
    }

    fn is_within_bounds_box(&self, min_x: f64, min_z: f64, max_x: f64, max_z: f64) -> bool {
        self.is_within_bounds(min_x, min_z) && self.is_within_bounds(max_x, max_z)
    }

    pub fn is_within_bounds(&self, x: f64, z: f64) -> bool {
        self.is_within_bounds_margin(x, z, 0.0)
    }

    pub fn is_within_bounds_margin(&self, x: f64, z: f64, margin: f64) -> bool {
        x >= self.get_min_x() - margin
            && x < self.get_max_x() + margin
            && z >= self.get_min_z() - margin
            && z < self.get_max_z() + margin
    }

    pub fn clamp_to_bounds_block(&self, position: &BlockPos) -> BlockPos {
        self.clamp_to_bounds(
            f64::from(position.x()),
            f64::from(position.y()),
            f64::from(position.z()),
        )
    }

    pub fn clamp_to_bounds_vec3(&self, position: &Vec3) -> BlockPos {
        self.clamp_to_bounds(position.x, position.y, position.z)
    }

    pub fn clamp_to_bounds(&self, x: f64, y: f64, z: f64) -> BlockPos {
        let v = self.clamp_vec3_to_bound(x, y, z);
        BlockPos::containing(v.x, v.y, v.z)
    }

    pub fn clamp_vec3_to_bound(&self, x: f64, y: f64, z: f64) -> Vec3 {
        Vec3 {
            x: clamp_f64(x, self.get_min_x(), self.get_max_x() - EPSILON),
            y,
            z: clamp_f64(z, self.get_min_z(), self.get_max_z() - EPSILON),
        }
    }

    /// `getDistanceToBorder(entity)` for an entity at `(x, z)`.
    pub fn get_distance_to_border(&self, x: f64, z: f64) -> f64 {
        let from_north = z - self.get_min_z();
        let from_south = self.get_max_z() - z;
        let from_west = x - self.get_min_x();
        let from_east = self.get_max_x() - x;
        let mut min = mc_core::math::java::min_f64(from_west, from_east);
        min = mc_core::math::java::min_f64(min, from_north);
        mc_core::math::java::min_f64(min, from_south)
    }

    /// `isInsideCloseToBorder(source, boundingBox)` for a source at `(source_x, source_z)`.
    pub fn is_inside_close_to_border(
        &self,
        source_x: f64,
        source_z: f64,
        bounding_box: &AABB,
    ) -> bool {
        let bb_max = mc_core::math::java::max_f64(
            abs_max_f64(bounding_box.get_xsize(), bounding_box.get_zsize()),
            1.0,
        );
        self.get_distance_to_border(source_x, source_z) < bb_max * 2.0
            && self.is_within_bounds_margin(source_x, source_z, bb_max)
    }

    pub fn get_collision_shape(&self) -> VoxelShape {
        match &self.extent {
            BorderExtent::Static { shape, .. } => shape.clone(),
            BorderExtent::Moving { .. } => border_shape(
                self.get_min_x_at(0.0),
                self.get_min_z_at(0.0),
                self.get_max_x_at(0.0),
                self.get_max_z_at(0.0),
            ),
        }
    }

    pub fn get_status(&self) -> BorderStatus {
        match &self.extent {
            BorderExtent::Static { .. } => BorderStatus::Stationary,
            BorderExtent::Moving { from, to, .. } => {
                if to < from {
                    BorderStatus::Shrinking
                } else {
                    BorderStatus::Growing
                }
            }
        }
    }

    fn moving_edge(
        &self,
        delta_partial_tick: f32,
        sign: f64,
        center: f64,
        previous_size: f64,
        size: f64,
    ) -> f64 {
        let max = f64::from(self.absolute_max_size);
        clamp_f64(
            center + sign * lerp_f64(f64::from(delta_partial_tick), previous_size, size) / 2.0,
            -max,
            max,
        )
    }

    pub fn get_min_x_at(&self, delta_partial_tick: f32) -> f64 {
        match &self.extent {
            BorderExtent::Static { min_x, .. } => *min_x,
            BorderExtent::Moving {
                previous_size,
                size,
                ..
            } => self.moving_edge(
                delta_partial_tick,
                -1.0,
                self.center_x,
                *previous_size,
                *size,
            ),
        }
    }

    pub fn get_min_z_at(&self, delta_partial_tick: f32) -> f64 {
        match &self.extent {
            BorderExtent::Static { min_z, .. } => *min_z,
            BorderExtent::Moving {
                previous_size,
                size,
                ..
            } => self.moving_edge(
                delta_partial_tick,
                -1.0,
                self.center_z,
                *previous_size,
                *size,
            ),
        }
    }

    pub fn get_max_x_at(&self, delta_partial_tick: f32) -> f64 {
        match &self.extent {
            BorderExtent::Static { max_x, .. } => *max_x,
            BorderExtent::Moving {
                previous_size,
                size,
                ..
            } => self.moving_edge(
                delta_partial_tick,
                1.0,
                self.center_x,
                *previous_size,
                *size,
            ),
        }
    }

    pub fn get_max_z_at(&self, delta_partial_tick: f32) -> f64 {
        match &self.extent {
            BorderExtent::Static { max_z, .. } => *max_z,
            BorderExtent::Moving {
                previous_size,
                size,
                ..
            } => self.moving_edge(
                delta_partial_tick,
                1.0,
                self.center_z,
                *previous_size,
                *size,
            ),
        }
    }

    pub fn get_min_x(&self) -> f64 {
        self.get_min_x_at(0.0)
    }
    pub fn get_min_z(&self) -> f64 {
        self.get_min_z_at(0.0)
    }
    pub fn get_max_x(&self) -> f64 {
        self.get_max_x_at(0.0)
    }
    pub fn get_max_z(&self) -> f64 {
        self.get_max_z_at(0.0)
    }

    pub fn get_center_x(&self) -> f64 {
        self.center_x
    }
    pub fn get_center_z(&self) -> f64 {
        self.center_z
    }

    pub fn set_center(&mut self, x: f64, z: f64) {
        self.center_x = x;
        self.center_z = z;
        // `extent.onCenterChange()`: static extents rebuild their box.
        if let BorderExtent::Static { size, .. } = self.extent {
            self.extent = self.static_extent(size);
        }
        self.set_dirty();
        self.events.push(BorderEvent::SetCenter { x, z });
    }

    pub fn get_size(&self) -> f64 {
        match &self.extent {
            BorderExtent::Static { size, .. } | BorderExtent::Moving { size, .. } => *size,
        }
    }

    pub fn get_lerp_time(&self) -> i64 {
        match &self.extent {
            BorderExtent::Static { .. } => 0,
            BorderExtent::Moving { lerp_progress, .. } => *lerp_progress,
        }
    }

    pub fn get_lerp_target(&self) -> f64 {
        match &self.extent {
            BorderExtent::Static { size, .. } => *size,
            BorderExtent::Moving { to, .. } => *to,
        }
    }

    pub fn get_lerp_speed(&self) -> f64 {
        match &self.extent {
            BorderExtent::Static { .. } => 0.0,
            BorderExtent::Moving {
                from,
                to,
                lerp_end,
                lerp_begin,
                ..
            } => (from - to).abs() / (lerp_end.wrapping_sub(*lerp_begin)) as f64,
        }
    }

    pub fn set_size(&mut self, size: f64) {
        self.extent = self.static_extent(size);
        self.set_dirty();
        self.events.push(BorderEvent::SetSize(size));
    }

    pub fn lerp_size_between(&mut self, from: f64, to: f64, ticks: i64, game_time: i64) {
        self.extent = if from == to {
            self.static_extent(to)
        } else {
            Self::moving_extent(from, to, ticks, game_time)
        };
        self.set_dirty();
        self.events.push(BorderEvent::LerpSize {
            from,
            to,
            ticks,
            game_time,
        });
    }

    pub fn set_absolute_max_size(&mut self, absolute_max_size: i32) {
        self.absolute_max_size = absolute_max_size;
        if let BorderExtent::Static { size, .. } = self.extent {
            self.extent = self.static_extent(size);
        }
    }

    pub fn get_absolute_max_size(&self) -> i32 {
        self.absolute_max_size
    }

    pub fn get_safe_zone(&self) -> f64 {
        self.safe_zone
    }

    pub fn set_safe_zone(&mut self, safe_zone: f64) {
        self.safe_zone = safe_zone;
        self.set_dirty();
        self.events.push(BorderEvent::SetSafeZone(safe_zone));
    }

    pub fn get_damage_per_block(&self) -> f64 {
        self.damage_per_block
    }

    pub fn set_damage_per_block(&mut self, damage_per_block: f64) {
        self.damage_per_block = damage_per_block;
        self.set_dirty();
        self.events
            .push(BorderEvent::SetDamagePerBlock(damage_per_block));
    }

    pub fn get_warning_time(&self) -> i32 {
        self.warning_time
    }

    pub fn set_warning_time(&mut self, warning_time: i32) {
        self.warning_time = warning_time;
        self.set_dirty();
        self.events.push(BorderEvent::SetWarningTime(warning_time));
    }

    pub fn get_warning_blocks(&self) -> i32 {
        self.warning_blocks
    }

    pub fn set_warning_blocks(&mut self, warning_blocks: i32) {
        self.warning_blocks = warning_blocks;
        self.set_dirty();
        self.events
            .push(BorderEvent::SetWarningBlocks(warning_blocks));
    }

    /// `tick()`: advances a lerping border by one tick (marks the data dirty while it moves).
    pub fn tick(&mut self) {
        if let BorderExtent::Moving {
            from,
            to,
            lerp_duration,
            lerp_progress,
            size,
            previous_size,
            ..
        } = &mut self.extent
        {
            *lerp_progress -= 1;
            *previous_size = *size;
            *size = Self::moving_size(*from, *to, *lerp_duration, *lerp_progress);
            let finished = *lerp_progress <= 0;
            let to = *to;
            self.set_dirty();
            if finished {
                self.extent = self.static_extent(to);
            }
        }
    }

    /// `applyInitialSettings(gameTime)`: pushes the loaded settings through the setters (once).
    pub fn apply_initial_settings(&mut self, game_time: i64) {
        if !self.initialized {
            let s = self.settings;
            self.set_center(s.center_x, s.center_z);
            self.set_damage_per_block(s.damage_per_block);
            self.set_safe_zone(s.safe_zone);
            self.set_warning_blocks(s.warning_blocks);
            self.set_warning_time(s.warning_time);
            if s.lerp_time > 0 {
                self.lerp_size_between(s.size, s.lerp_target, s.lerp_time, game_time);
            } else {
                self.set_size(s.size);
            }
            self.initialized = true;
        }
    }

    /// `new Settings(worldBorder)`: the current state.
    pub fn current_settings(&self) -> Settings {
        Settings {
            center_x: self.center_x,
            center_z: self.center_z,
            damage_per_block: self.damage_per_block,
            safe_zone: self.safe_zone,
            warning_blocks: self.warning_blocks,
            warning_time: self.warning_time,
            size: self.get_size(),
            lerp_time: self.get_lerp_time(),
            lerp_target: self.get_lerp_target(),
        }
    }

    /// `WorldBorder.TYPE` (`minecraft:world_border`).
    #[expect(
        clippy::result_large_err,
        reason = "DecodeResult<T> carries the partial value by value (DataResult.error(msg, partial)); the SavedDataType decode fn pointer fixes the signature"
    )]
    pub fn type_() -> SavedDataType<WorldBorder> {
        SavedDataType::new(
            Identifier::parse("minecraft:world_border").expect("static id"),
            WorldBorder::default,
            |tag| -> DecodeResult<WorldBorder> { Ok(WorldBorder::new(Settings::decode(tag)?)) },
            |border| Ok(border.current_settings().encode()),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_and_static_box() {
        let b = WorldBorder::default();
        assert_eq!((b.get_min_x(), b.get_max_x()), (-29999984.0, 29999984.0));
        assert_eq!(b.get_status(), BorderStatus::Stationary);
        assert!(b.is_within_bounds(0.0, 0.0));
        assert!(
            !b.is_within_bounds(29999984.0, 0.0),
            "max edge is exclusive"
        );
    }

    #[test]
    fn decode_range_check() {
        let bad = mc_nbt::snbt::parse(
            "{center_x:3.0E7d,center_z:0.0d,damage_per_block:0.2d,lerp_target:0.0d,lerp_time:0L,safe_zone:5.0d,size:100.0d,warning_blocks:5,warning_time:300}",
        )
        .unwrap();
        assert!(Settings::decode(&bad).is_err());
    }
}
