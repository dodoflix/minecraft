//! `MapItemSavedData` (`data/minecraft/maps/<id>.dat`) with its `HoldingPlayer`s and `MapPatch`.

use super::map_decoration::{MapDecoration, MapDecorationType, by_name, track_count};
use super::map_markers::{MapBanner, MapFrame};
use crate::impl_saved_data;
use crate::server::players::java_hash_map::JavaHashMap;
use crate::util::codec::{self, DataResult, DecodeResult};
use crate::world::entity::EntityId;
use crate::world::item::ItemStack;
use crate::world::level::saveddata::{SavedData, SavedDataType};
use crate::world::level::{BlockGetter, DimensionKey};
use indexmap::IndexMap;
use mc_core::ident::Identifier;
use mc_core::pos::BlockPos;
use mc_nbt::{CompoundTag, ListTag, Tag};
use mc_protocol::components::{MapDecorations, MapId};
use mc_protocol::packets::play::{ClientboundMapItemDataPacket, MapItemSavedDataMapPatch};
use mc_text::Component;

const MAP_SIZE: usize = 128;
pub const MAX_SCALE: i8 = 4;
pub const TRACKED_DECORATION_LIMIT: i32 = 256;

/// What `tickCarriedBy` reads from a `Player`: the vanilla code walks the real player objects, the Rust entities are
/// owned by their level, so the caller hands in these facts.
#[derive(Clone, Debug)]
pub struct PlayerView {
    pub id: EntityId,
    /// `getPlainTextName()`.
    pub name: String,
    /// `level().dimension()`.
    pub dimension: DimensionKey,
    pub x: f64,
    pub z: f64,
    pub y_rot: f32,
    /// `isRemoved()`.
    pub removed: bool,
    /// `getInventory().contains(mapMatcher)`.
    pub has_map: bool,
    /// `hasMapInvisibilityItemEquipped`.
    pub map_invisibility: bool,
}

/// The item frame a map hangs in (`tickCarriedBy`'s `placedInFrame`).
#[derive(Clone, Copy, Debug)]
pub struct FrameInfo {
    /// `getPos()`.
    pub pos: BlockPos,
    /// `getDirection().get2DDataValue()`.
    pub direction_2d: i32,
    /// `getId()`.
    pub entity_id: i32,
}

/// What `tickCarriedBy` needs from the surroundings of the ticking player.
pub struct MapTickEnv<'a> {
    /// `Level.getGameTime()` of the ticking player's level.
    pub game_time: i64,
    /// `MinecraftServer.tickCount`, to age out the snapshots of players that are not in the ticking level.
    pub tick_count: i32,
    /// The ticking player.
    pub ticking: PlayerView,
    /// The live view of another holding player in the ticking level (`None` when it is not in it); the argument is
    /// `mapMatcher`, the predicate of `getInventory().contains(..)`.
    pub other: &'a dyn Fn(EntityId, &dyn Fn(&ItemStack) -> bool) -> Option<PlayerView>,
}

/// `MapItemSavedData.HoldingPlayer`.
#[derive(Clone, Debug)]
pub struct HoldingPlayer {
    pub player_id: EntityId,
    dirty_data: bool,
    min_dirty_x: i32,
    min_dirty_y: i32,
    max_dirty_x: i32,
    max_dirty_y: i32,
    dirty_decorations: bool,
    tick: i32,
    pub step: i32,
    /// The last view of the player seen live (its own `tickCarriedBy`), used while it is in another level.
    last_view: Option<(PlayerView, i32)>,
}

impl HoldingPlayer {
    fn new(player_id: EntityId) -> Self {
        HoldingPlayer {
            player_id,
            dirty_data: true,
            min_dirty_x: 0,
            min_dirty_y: 0,
            max_dirty_x: 127,
            max_dirty_y: 127,
            dirty_decorations: true,
            tick: 0,
            step: 0,
            last_view: None,
        }
    }

    fn mark_colors_dirty(&mut self, x: i32, y: i32) {
        if self.dirty_data {
            self.min_dirty_x = self.min_dirty_x.min(x);
            self.min_dirty_y = self.min_dirty_y.min(y);
            self.max_dirty_x = self.max_dirty_x.max(x);
            self.max_dirty_y = self.max_dirty_y.max(y);
        } else {
            self.dirty_data = true;
            self.min_dirty_x = x;
            self.min_dirty_y = y;
            self.max_dirty_x = x;
            self.max_dirty_y = y;
        }
    }
}

pub struct MapItemSavedData {
    dirty: bool,
    pub center_x: i32,
    pub center_z: i32,
    pub dimension: DimensionKey,
    tracking_position: bool,
    unlimited_tracking: bool,
    pub scale: i8,
    pub colors: Vec<u8>,
    pub locked: bool,
    carried_by: Vec<HoldingPlayer>,
    /// `Maps.newHashMap()`: iteration order is `String.hashCode` order (it is the saved list order).
    banner_markers: JavaHashMap<MapBanner>,
    /// `Maps.newLinkedHashMap()`.
    decorations: IndexMap<String, MapDecoration>,
    frame_markers: JavaHashMap<MapFrame>,
    tracked_decoration_count: i32,
}

impl_saved_data!(MapItemSavedData);

fn frame_key(id: i32) -> String {
    format!("frame-{id}")
}

fn clamp_map_coordinate(delta_from_center: f32) -> i8 {
    if delta_from_center <= -63.0 {
        -128
    } else if delta_from_center >= 63.0 {
        127
    } else {
        ((delta_from_center * 2.0f32 + 0.5f32) as i32) as i8
    }
}

fn is_inside_map(xd: f32, yd: f32) -> bool {
    xd >= -63.0 && yd >= -63.0 && xd <= 63.0 && yd <= 63.0
}

impl MapItemSavedData {
    fn raw(
        center_x: i32,
        center_z: i32,
        scale: i8,
        tracking_position: bool,
        unlimited_tracking: bool,
        locked: bool,
        dimension: DimensionKey,
    ) -> Self {
        MapItemSavedData {
            dirty: false,
            center_x,
            center_z,
            dimension,
            tracking_position,
            unlimited_tracking,
            scale,
            colors: vec![0; MAP_SIZE * MAP_SIZE],
            locked,
            carried_by: Vec::new(),
            banner_markers: JavaHashMap::new(),
            decorations: IndexMap::new(),
            frame_markers: JavaHashMap::new(),
            tracked_decoration_count: 0,
        }
    }

    /// `createFresh(originX, originY, scale, trackingPosition, unlimitedTracking, dimension)`.
    pub fn create_fresh(
        origin_x: f64,
        origin_z: f64,
        scale: i8,
        tracking_position: bool,
        unlimited_tracking: bool,
        dimension: DimensionKey,
    ) -> Self {
        let size = 128 * (1i32 << scale);
        let area_x = ((origin_x + 64.0) / f64::from(size)).floor() as i32;
        let area_z = ((origin_z + 64.0) / f64::from(size)).floor() as i32;
        let x = area_x.wrapping_mul(size) + size / 2 - 64;
        let z = area_z.wrapping_mul(size) + size / 2 - 64;
        Self::raw(
            x,
            z,
            scale,
            tracking_position,
            unlimited_tracking,
            false,
            dimension,
        )
    }

    /// `locked()`: a locked copy (the frame markers are not copied).
    pub fn locked(&self) -> MapItemSavedData {
        let mut result = Self::raw(
            self.center_x,
            self.center_z,
            self.scale,
            self.tracking_position,
            self.unlimited_tracking,
            true,
            self.dimension.clone(),
        );
        for (key, banner) in self.banner_markers.iter() {
            result.banner_markers.insert(key.to_owned(), banner.clone());
        }
        result.decorations = self.decorations.clone();
        result.tracked_decoration_count = self.tracked_decoration_count;
        result.colors.copy_from_slice(&self.colors);
        result
    }

    /// `scaled()`.
    pub fn scaled(&self) -> MapItemSavedData {
        Self::create_fresh(
            f64::from(self.center_x),
            f64::from(self.center_z),
            (i32::from(self.scale) + 1).clamp(0, 4) as i8,
            self.tracking_position,
            self.unlimited_tracking,
            self.dimension.clone(),
        )
    }

    fn map_matcher(map_stack: &ItemStack) -> impl Fn(&ItemStack) -> bool + use<> {
        let map_id = map_stack.get(&mc_protocol::components::MAP_ID).copied();
        let item = map_stack.item();
        move |stack: &ItemStack| {
            !stack.is_empty()
                && stack.item() == item
                && map_id == stack.get(&mc_protocol::components::MAP_ID).copied()
        }
    }

    /// The matcher a caller uses to compute `PlayerView::has_map` for `map_stack`.
    pub fn matcher_for(map_stack: &ItemStack) -> impl Fn(&ItemStack) -> bool + use<> {
        Self::map_matcher(map_stack)
    }

    /// `tickCarriedBy(tickingPlayer, itemStack, placedInFrame)`.
    pub fn tick_carried_by(
        &mut self,
        env: &MapTickEnv<'_>,
        item_stack: &ItemStack,
        placed_in_frame: Option<FrameInfo>,
    ) {
        let ticking = &env.ticking;
        if !self.carried_by.iter().any(|h| h.player_id == ticking.id) {
            self.carried_by.push(HoldingPlayer::new(ticking.id));
        }
        let matcher = Self::map_matcher(item_stack);
        if !ticking.has_map {
            self.remove_decoration(&ticking.name);
        }

        let mut i = 0;
        // Java removes from the list inside an indexed loop: the element after a removed one is skipped.
        while i < self.carried_by.len() {
            let holder_id = self.carried_by[i].player_id;
            let view = if holder_id == ticking.id {
                Some(ticking.clone())
            } else if let Some(live) = (env.other)(holder_id, &matcher) {
                Some(live)
            } else {
                // not in the ticking level: its own level refreshes the snapshot every tick it holds the map
                self.carried_by[i]
                    .last_view
                    .as_ref()
                    .filter(|(_, tick)| env.tick_count.wrapping_sub(*tick) <= 1)
                    .map(|(v, _)| v.clone())
            };
            if let Some(v) = &view {
                self.carried_by[i].last_view = Some((v.clone(), env.tick_count));
            }
            let (name, removed, has_map, in_dimension, invisible) = match &view {
                Some(v) => (
                    v.name.clone(),
                    v.removed,
                    v.has_map,
                    v.dimension == self.dimension,
                    v.map_invisibility,
                ),
                None => (
                    self.carried_by[i]
                        .last_view
                        .as_ref()
                        .map(|(v, _)| v.name.clone())
                        .unwrap_or_default(),
                    true,
                    false,
                    false,
                    false,
                ),
            };
            if !removed && (placed_in_frame.is_some() || has_map) {
                if placed_in_frame.is_none() && in_dimension && self.tracking_position {
                    let v = view.as_ref().expect("a live player here");
                    self.add_decoration(
                        by_name("player"),
                        Some(env.game_time),
                        &name,
                        v.x,
                        v.z,
                        f64::from(v.y_rot),
                        None,
                    );
                }
            } else {
                self.carried_by.remove(i);
                self.remove_decoration(&name);
            }
            if holder_id != ticking.id && invisible {
                self.remove_decoration(&name);
            }
            i += 1;
        }

        if let Some(frame) = placed_in_frame.filter(|_| self.tracking_position) {
            let pos = frame.pos;
            if let Some(existing) = self.frame_markers.get(&MapFrame::frame_id(pos)).cloned()
                && frame.entity_id != existing.entity_id
                && self.frame_markers.contains_key(&existing.get_id())
            {
                self.remove_decoration(&frame_key(existing.entity_id));
            }
            let map_frame = MapFrame {
                pos,
                rotation: frame.direction_2d * 90,
                entity_id: frame.entity_id,
            };
            self.add_decoration(
                by_name("frame"),
                Some(env.game_time),
                &frame_key(frame.entity_id),
                f64::from(pos.x()),
                f64::from(pos.z()),
                f64::from(frame.direction_2d * 90),
                None,
            );
            let old = self
                .frame_markers
                .insert(map_frame.get_id(), map_frame.clone());
            if old.as_ref() != Some(&map_frame) {
                self.set_dirty();
            }
        }

        let static_decorations = item_stack
            .get(&mc_protocol::components::MAP_DECORATIONS)
            .cloned()
            .unwrap_or_default();
        if !static_decorations
            .decorations
            .iter()
            .all(|(id, _)| self.decorations.contains_key(id))
        {
            for (id, entry) in &static_decorations.decorations {
                if !self.decorations.contains_key(id) {
                    self.add_decoration(
                        entry.ty,
                        Some(env.game_time),
                        id,
                        entry.x,
                        entry.z,
                        f64::from(entry.rotation),
                        None,
                    );
                }
            }
        }
    }

    fn remove_decoration(&mut self, key: &str) {
        if let Some(decoration) = self.decorations.shift_remove(key)
            && track_count(decoration.ty)
        {
            self.tracked_decoration_count -= 1;
        }
        self.set_decorations_dirty();
    }

    /// `addTargetDecoration(itemStack, position, key, decorationType)`.
    pub fn add_target_decoration(
        stack: &mut ItemStack,
        position: BlockPos,
        key: &str,
        decoration_type: MapDecorationType,
    ) {
        let entry = mc_protocol::components::MapDecorationEntry {
            ty: decoration_type,
            x: f64::from(position.x()),
            z: f64::from(position.z()),
            rotation: 180.0,
        };
        let mut decorations: MapDecorations = stack
            .get(&mc_protocol::components::MAP_DECORATIONS)
            .cloned()
            .unwrap_or_default();
        // `MapDecorations.withDecoration`: a copy of the LinkedHashMap with the entry put.
        match decorations.decorations.iter_mut().find(|(k, _)| k == key) {
            Some((_, existing)) => *existing = entry,
            None => decorations.decorations.push((key.to_owned(), entry)),
        }
        stack.set(mc_protocol::components::MAP_DECORATIONS, decorations);
    }

    #[allow(clippy::too_many_arguments)]
    fn add_decoration(
        &mut self,
        ty: MapDecorationType,
        game_time: Option<i64>,
        key: &str,
        x_pos: f64,
        z_pos: f64,
        y_rot: f64,
        name: Option<Component>,
    ) {
        let scaling = 1i32 << self.scale;
        let x_delta = (x_pos - f64::from(self.center_x)) as f32 / scaling as f32;
        let y_delta = (z_pos - f64::from(self.center_z)) as f32 / scaling as f32;
        match self.calculate_decoration_location_and_type(ty, game_time, y_rot, x_delta, y_delta) {
            None => self.remove_decoration(key),
            Some((ty, x, y, rot)) => {
                let new_decoration = MapDecoration::new(ty, x, y, rot, name);
                let previous = self
                    .decorations
                    .insert(key.to_owned(), new_decoration.clone());
                if previous.as_ref() != Some(&new_decoration) {
                    if let Some(previous) = &previous
                        && track_count(previous.ty)
                    {
                        self.tracked_decoration_count -= 1;
                    }
                    if track_count(ty) {
                        self.tracked_decoration_count += 1;
                    }
                    self.set_decorations_dirty();
                }
            }
        }
    }

    fn calculate_decoration_location_and_type(
        &self,
        ty: MapDecorationType,
        game_time: Option<i64>,
        y_rot: f64,
        x_delta: f32,
        y_delta: f32,
    ) -> Option<(MapDecorationType, i8, i8, i8)> {
        let clamped_x = clamp_map_coordinate(x_delta);
        let clamped_y = clamp_map_coordinate(y_delta);
        if ty == by_name("player") {
            let rot = self.calculate_rotation(game_time, y_rot);
            if is_inside_map(x_delta, y_delta) {
                Some((ty, clamped_x, clamped_y, rot))
            } else {
                self.decoration_type_for_player_outside_map(x_delta, y_delta)
                    .map(|t| (t, clamped_x, clamped_y, rot))
            }
        } else if !is_inside_map(x_delta, y_delta) && !self.unlimited_tracking {
            None
        } else {
            Some((
                ty,
                clamped_x,
                clamped_y,
                self.calculate_rotation(game_time, y_rot),
            ))
        }
    }

    fn calculate_rotation(&self, game_time: Option<i64>, y_rot: f64) -> i8 {
        match game_time {
            Some(time) if self.dimension.to_string() == "minecraft:the_nether" => {
                let s = (time / 10) as i32;
                (s.wrapping_mul(s)
                    .wrapping_mul(34187121)
                    .wrapping_add(s.wrapping_mul(121))
                    >> 15
                    & 15) as i8
            }
            _ => {
                let adjusted = if y_rot < 0.0 {
                    y_rot - 8.0
                } else {
                    y_rot + 8.0
                };
                ((adjusted * 16.0 / 360.0) as i32) as i8
            }
        }
    }

    fn decoration_type_for_player_outside_map(
        &self,
        x_delta: f32,
        y_delta: f32,
    ) -> Option<MapDecorationType> {
        if x_delta.abs() < 320.0 && y_delta.abs() < 320.0 {
            Some(by_name("player_off_map"))
        } else if self.unlimited_tracking {
            Some(by_name("player_off_limits"))
        } else {
            None
        }
    }

    /// `getUpdatePacket(id, player)`.
    pub fn get_update_packet(
        &mut self,
        id: MapId,
        player: EntityId,
    ) -> Option<ClientboundMapItemDataPacket> {
        let index = self.carried_by.iter().position(|h| h.player_id == player)?;
        self.next_update_packet(index, id)
    }

    fn next_update_packet(
        &mut self,
        index: usize,
        id: MapId,
    ) -> Option<ClientboundMapItemDataPacket> {
        let holder = &mut self.carried_by[index];
        let patch = if holder.dirty_data {
            holder.dirty_data = false;
            let (start_x, start_y) = (holder.min_dirty_x, holder.min_dirty_y);
            let width = holder.max_dirty_x + 1 - holder.min_dirty_x;
            let height = holder.max_dirty_y + 1 - holder.min_dirty_y;
            let mut map_colors = vec![0u8; (width * height) as usize];
            for x in 0..width {
                for y in 0..height {
                    map_colors[(x + y * width) as usize] =
                        self.colors[(start_x + x + (start_y + y) * 128) as usize];
                }
            }
            Some(MapItemSavedDataMapPatch {
                start_x: start_x as u8,
                start_y: start_y as u8,
                width: width as u8,
                height: height as u8,
                map_colors,
            })
        } else {
            None
        };
        let decorations = if holder.dirty_decorations && {
            let t = holder.tick;
            holder.tick = holder.tick.wrapping_add(1);
            t % 5 == 0
        } {
            holder.dirty_decorations = false;
            Some(
                self.decorations
                    .values()
                    .map(MapDecoration::to_packet)
                    .collect::<Vec<_>>(),
            )
        } else {
            None
        };
        if decorations.is_none() && patch.is_none() {
            None
        } else {
            Some(ClientboundMapItemDataPacket {
                map_id: mc_protocol::packets::play::MapId { id: id.0 },
                scale: self.scale,
                locked: self.locked,
                decorations,
                color_patch: patch,
            })
        }
    }

    fn set_colors_dirty(&mut self, x: i32, y: i32) {
        self.set_dirty();
        for holder in &mut self.carried_by {
            holder.mark_colors_dirty(x, y);
        }
    }

    fn set_decorations_dirty(&mut self) {
        for holder in &mut self.carried_by {
            holder.dirty_decorations = true;
        }
    }

    /// `getHoldingPlayer(player)`.
    pub fn get_holding_player(&mut self, player: EntityId) -> &mut HoldingPlayer {
        let index = match self.carried_by.iter().position(|h| h.player_id == player) {
            Some(i) => i,
            None => {
                self.carried_by.push(HoldingPlayer::new(player));
                self.carried_by.len() - 1
            }
        };
        &mut self.carried_by[index]
    }

    /// `toggleBanner(level, pos)`.
    pub fn toggle_banner(
        &mut self,
        level: &dyn BlockGetter,
        game_time: i64,
        pos: BlockPos,
    ) -> bool {
        let x_pos = f64::from(pos.x()) + 0.5;
        let z_pos = f64::from(pos.z()) + 0.5;
        let scale = f64::from(1i32 << self.scale);
        let xd = (x_pos - f64::from(self.center_x)) / scale;
        let yd = (z_pos - f64::from(self.center_z)) / scale;
        if xd >= -63.0 && yd >= -63.0 && xd <= 63.0 && yd <= 63.0 {
            let Some(banner) = MapBanner::from_world(level, pos) else {
                return false;
            };
            let id = banner.get_id();
            if self.banner_markers.get(&id) == Some(&banner) {
                self.banner_markers.remove(&id);
                self.remove_decoration(&id);
                self.set_dirty();
                return true;
            }
            if !self.is_tracked_count_over_limit(256) {
                self.banner_markers.insert(id.clone(), banner.clone());
                self.add_decoration(
                    banner.get_decoration(),
                    Some(game_time),
                    &id,
                    x_pos,
                    z_pos,
                    180.0,
                    banner.name.clone(),
                );
                self.set_dirty();
                return true;
            }
        }
        false
    }

    /// `checkBanners(level, x, z)`.
    pub fn check_banners(&mut self, level: &dyn BlockGetter, x: i32, z: i32) {
        let gone: Vec<MapBanner> = self
            .banner_markers
            .values()
            .filter(|expected| expected.pos.x() == x && expected.pos.z() == z)
            .filter(|expected| {
                MapBanner::from_world(level, expected.pos).as_ref() != Some(*expected)
            })
            .cloned()
            .collect();
        for expected in gone {
            self.banner_markers.remove(&expected.get_id());
            self.remove_decoration(&expected.get_id());
            self.set_dirty();
        }
    }

    pub fn get_banners(&self) -> impl Iterator<Item = &MapBanner> {
        self.banner_markers.values()
    }

    /// `removedFromFrame(pos, entityID)`.
    pub fn removed_from_frame(&mut self, pos: BlockPos, entity_id: i32) {
        self.remove_decoration(&frame_key(entity_id));
        self.frame_markers.remove(&MapFrame::frame_id(pos));
        self.set_dirty();
    }

    /// `updateColor(x, y, newColor)`.
    pub fn update_color(&mut self, x: i32, y: i32, new_color: u8) -> bool {
        if self.colors[(x + y * 128) as usize] != new_color {
            self.set_color(x, y, new_color);
            true
        } else {
            false
        }
    }

    /// `setColor(x, y, newColor)`.
    pub fn set_color(&mut self, x: i32, y: i32, new_color: u8) {
        self.colors[(x + y * 128) as usize] = new_color;
        self.set_colors_dirty(x, y);
    }

    pub fn get_decorations(&self) -> impl Iterator<Item = &MapDecoration> {
        self.decorations.values()
    }

    /// `isTrackedCountOverLimit(limit)`.
    pub fn is_tracked_count_over_limit(&self, limit: i32) -> bool {
        self.tracked_decoration_count > limit
    }

    pub fn is_tracking_position(&self) -> bool {
        self.tracking_position
    }

    pub fn is_unlimited_tracking(&self) -> bool {
        self.unlimited_tracking
    }

    /// `MapItemSavedData.CODEC` decode.
    pub fn decode(tag: &Tag) -> DecodeResult<MapItemSavedData> {
        let m = codec::compound(tag)?;
        let dimension = codec::identifier(codec::field(m, "dimension")?)?;
        let center_x = codec::int(codec::field(m, "xCenter")?)?;
        let center_z = codec::int(codec::field(m, "zCenter")?)?;
        let scale = codec::optional(m, "scale", |t| codec::int(t).map(|v| v as i8))?.unwrap_or(0);
        let colors: Vec<u8> = match codec::field(m, "colors")? {
            Tag::ByteArray(v) => v.iter().map(|&b| b as u8).collect(),
            other => codec::list(other)?
                .iter()
                .map(|t| codec::int(t).map(|v| v as u8))
                .collect::<DataResult<_>>()?,
        };
        let tracking_position =
            codec::optional(m, "trackingPosition", codec::boolean)?.unwrap_or(true);
        let unlimited_tracking =
            codec::optional(m, "unlimitedTracking", codec::boolean)?.unwrap_or(false);
        let locked = codec::optional(m, "locked", codec::boolean)?.unwrap_or(false);
        let banners = match m.get("banners") {
            Some(t) => codec::list(t)?
                .iter()
                .map(MapBanner::decode)
                .collect::<DataResult<Vec<_>>>()?,
            None => Vec::new(),
        };
        let frames = match m.get("frames") {
            Some(t) => codec::list(t)?
                .iter()
                .map(MapFrame::decode)
                .collect::<DataResult<Vec<_>>>()?,
            None => Vec::new(),
        };

        let mut data = Self::raw(
            center_x,
            center_z,
            i32::from(scale).clamp(0, 4) as i8,
            tracking_position,
            unlimited_tracking,
            locked,
            dimension,
        );
        if colors.len() == MAP_SIZE * MAP_SIZE {
            data.colors = colors;
        }
        for banner in banners {
            data.banner_markers.insert(banner.get_id(), banner.clone());
            data.add_decoration(
                banner.get_decoration(),
                None,
                &banner.get_id(),
                f64::from(banner.pos.x()),
                f64::from(banner.pos.z()),
                180.0,
                banner.name.clone(),
            );
        }
        for frame in frames {
            data.frame_markers.insert(frame.get_id(), frame.clone());
            data.add_decoration(
                by_name("frame"),
                None,
                &frame_key(frame.entity_id),
                f64::from(frame.pos.x()),
                f64::from(frame.pos.z()),
                f64::from(frame.rotation),
                None,
            );
        }
        Ok(data)
    }

    /// `MapItemSavedData.CODEC` encode.
    pub fn encode(&self) -> DataResult<Tag> {
        let mut m = CompoundTag::new();
        m.put_string("dimension", self.dimension.to_string());
        m.put_int("xCenter", self.center_x);
        m.put_int("zCenter", self.center_z);
        if self.scale != 0 {
            m.put_byte("scale", self.scale);
        }
        m.put_byte_array("colors", self.colors.iter().map(|&b| b as i8).collect());
        if !self.tracking_position {
            m.put_boolean("trackingPosition", false);
        }
        if self.unlimited_tracking {
            m.put_boolean("unlimitedTracking", true);
        }
        if self.locked {
            m.put_boolean("locked", true);
        }
        if !self.banner_markers.is_empty() {
            let mut list = ListTag::new();
            for banner in self.banner_markers.values() {
                list.push(banner.encode()?);
            }
            m.put("banners", Tag::List(list));
        }
        if !self.frame_markers.is_empty() {
            let mut list = ListTag::new();
            for frame in self.frame_markers.values() {
                list.push(frame.encode());
            }
            m.put("frames", Tag::List(list));
        }
        Ok(Tag::Compound(m))
    }

    /// `MapItemSavedData.type(id)`.
    pub fn type_(id: MapId) -> SavedDataType<MapItemSavedData> {
        SavedDataType::new(
            Identifier::parse(&format!("minecraft:maps/{}", id.0)).expect("valid id"),
            || panic!("Should never create an empty map saved data"),
            MapItemSavedData::decode,
            MapItemSavedData::encode,
        )
    }
}
