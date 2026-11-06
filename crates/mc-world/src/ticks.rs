//! Scheduled block/fluid ticks (vanilla `net.minecraft.world.ticks`): `TickPriority`, `SavedTick` (+ its NBT codec),
//! `ScheduledTick`, `ProtoChunkTicks`, `LevelChunkTicks`, `LevelTicks`, `WorldGenTickAccess`, `BlackholeTickAccess`
//! and the `ChunkAccess.PackedTicks` record.
//!
//! Deviations forced by Rust ownership (behaviour is otherwise 1:1, including iteration orders):
//! * `LevelTicks` *owns* the `LevelChunkTicks` containers (vanilla shares them with the chunk and wires an
//!   `onTickAdded` callback into the container). Schedule through [`LevelTicks::schedule`], read through
//!   [`LevelTicks::container`], take a container back with [`LevelTicks::remove_container`].
//! * `LevelTicks`' `tickCheck` predicate is passed to [`LevelTicks::tick`] / [`LevelTicks::collect_ticks`] instead of the
//!   constructor, and `tick` is also split into `collect_ticks` / `poll_collected_tick` / `cleanup_after_tick` so the
//!   per-tick callback may schedule new ticks (vanilla's `output` callback re-enters `schedule`).

mod java_util;

use java_util::{JavaPriorityQueue, Long2LongOpenHashMap};
use mc_core::bounding_box::BoundingBox;
use mc_core::pos::block_pos::BlockPos;
use mc_core::pos::chunk_pos::ChunkPos;
use mc_core::pos::section_pos::SectionPos;
use mc_core::pos::vec3i::Vec3i;
use mc_data::block::Block;
use mc_data::fluid::Fluid;
use mc_data::registry::RegistryId;
use mc_nbt::{CompoundTag, ListTag, Number, Tag};
use std::cmp::Ordering;
use std::collections::{HashMap, HashSet, VecDeque};
use std::hash::Hash;

/// `TickPriority`; the derived `Ord` is the enum's ordinal order (`compareTo`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum TickPriority {
    ExtremelyHigh,
    VeryHigh,
    High,
    Normal,
    Low,
    VeryLow,
    ExtremelyLow,
}

impl TickPriority {
    pub const ALL: [TickPriority; 7] = [
        TickPriority::ExtremelyHigh,
        TickPriority::VeryHigh,
        TickPriority::High,
        TickPriority::Normal,
        TickPriority::Low,
        TickPriority::VeryLow,
        TickPriority::ExtremelyLow,
    ];

    /// `getValue()`: -3 ..= 3.
    pub const fn get_value(self) -> i32 {
        self as i32 - 3
    }

    /// `byValue(int)`: out-of-range values clamp to the nearest extreme.
    pub const fn by_value(value: i32) -> TickPriority {
        if value < -3 {
            TickPriority::ExtremelyHigh
        } else if value > 3 {
            TickPriority::ExtremelyLow
        } else {
            Self::ALL[(value + 3) as usize]
        }
    }
}

/// `SavedTick<T>` record. `ty` is vanilla's `type`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SavedTick<T> {
    pub ty: T,
    pub pos: BlockPos,
    pub delay: i32,
    pub priority: TickPriority,
}

/// `Codec.INT` over `NbtOps`: any numeric tag through `Number.intValue()` (truncation, unlike `CompoundTag.getInt`).
fn codec_int(tag: &Tag) -> Option<i32> {
    Some(match tag.as_number()? {
        Number::Byte(v) => v as i32,
        Number::Short(v) => v as i32,
        Number::Int(v) => v,
        Number::Long(v) => v as i32,
        Number::Float(v) => v as i32,
        Number::Double(v) => v as i32,
    })
}

impl<T> SavedTick<T> {
    /// `SavedTick.probe`.
    pub fn probe(ty: T, pos: BlockPos) -> SavedTick<T> {
        SavedTick {
            ty,
            pos,
            delay: 0,
            priority: TickPriority::Normal,
        }
    }

    /// `SavedTick.filterTickListForChunk`.
    pub fn filter_tick_list_for_chunk(
        saved_ticks: &[SavedTick<T>],
        chunk_pos: ChunkPos,
    ) -> Vec<SavedTick<T>>
    where
        T: Clone,
    {
        let key = chunk_pos.pack();
        saved_ticks
            .iter()
            .filter(|t| ChunkPos::pack_block_pos(&t.pos) == key)
            .cloned()
            .collect()
    }

    /// `unpack(currentTick, currentSubTick)`.
    pub fn unpack(self, current_tick: i64, current_sub_tick: i64) -> ScheduledTick<T> {
        ScheduledTick {
            ty: self.ty,
            pos: self.pos,
            trigger_tick: current_tick.wrapping_add(self.delay as i64),
            priority: self.priority,
            sub_tick_order: current_sub_tick,
        }
    }
}

impl<T: RegistryId> SavedTick<T> {
    /// `typeCodec = registry.byNameCodec()`: a string naming a registry entry (bare names mean `minecraft:`).
    fn decode_type(tag: &Tag) -> Option<T> {
        T::registry().id_of(tag.as_string()?).and_then(T::from_id)
    }

    /// One element of `SavedTick.codec(typeCodec)`: a compound with `i`, `x`, `y`, `z`, `t`, `p`; `decode_type` is the
    /// type codec applied to `i` (a missing `i` is always an error).
    fn from_tag_with(tag: &Tag, decode_type: &impl Fn(&Tag) -> Option<T>) -> Option<SavedTick<T>> {
        let c = tag.as_compound()?;
        let ty = decode_type(c.get("i")?)?;
        let pos = BlockPos::new(
            codec_int(c.get("x")?)?,
            codec_int(c.get("y")?)?,
            codec_int(c.get("z")?)?,
        );
        let delay = codec_int(c.get("t")?)?;
        let priority = TickPriority::by_value(codec_int(c.get("p")?)?);
        Some(SavedTick {
            ty,
            pos,
            delay,
            priority,
        })
    }

    fn parse_list_with(
        list: &ListTag,
        decode_type: impl Fn(&Tag) -> Option<T>,
    ) -> Vec<SavedTick<T>> {
        // `ListCodec` keeps the elements that decode and `CompoundTag.read` (`resultOrPartial`) returns that partial list.
        list.iter()
            .filter_map(|e| Self::from_tag_with(e, &decode_type))
            .collect()
    }

    /// `SavedTick.codec(registry.byNameCodec()).listOf()` as used by `SerializableChunkData`: elements that fail to
    /// decode are skipped (vanilla logs the error and keeps the partial list).
    pub fn parse_list(list: &ListTag) -> Vec<SavedTick<T>> {
        Self::parse_list_with(list, Self::decode_type)
    }

    /// `SavedTick.codec(registry.byNameCodec().orElse(default)).listOf()` as used by `UpgradeData`: an undecodable
    /// type becomes `default`.
    pub fn parse_list_or(list: &ListTag, default: T) -> Vec<SavedTick<T>> {
        Self::parse_list_with(list, |t| Some(Self::decode_type(t).unwrap_or(default)))
    }

    /// `chunkData.read(key, CODEC).orElse(List.of())`.
    pub fn read_list(tag: &CompoundTag, key: &str) -> Vec<SavedTick<T>> {
        tag.get_list(key).map(Self::parse_list).unwrap_or_default()
    }

    /// `tag.read(key, CODEC_WITH_ORELSE)` followed by `ifPresent(addAll)` / `orElse(List.of())`.
    pub fn read_list_or(tag: &CompoundTag, key: &str, default: T) -> Vec<SavedTick<T>> {
        tag.get_list(key)
            .map(|l| Self::parse_list_or(l, default))
            .unwrap_or_default()
    }

    /// Encoding of one tick (`i`, `x`, `y`, `z`, `t`, `p`).
    pub fn to_tag(&self) -> CompoundTag {
        let mut c = CompoundTag::new();
        c.put_string(
            "i",
            T::registry()
                .name_of(self.ty.id())
                .expect("typed registry ids are valid"),
        );
        c.put_int("x", self.pos.x());
        c.put_int("y", self.pos.y());
        c.put_int("z", self.pos.z());
        c.put_int("t", self.delay);
        c.put_int("p", self.priority.get_value());
        c
    }

    /// `codec.listOf()` encoding (`CompoundTag.store`).
    pub fn write_list(ticks: &[SavedTick<T>]) -> ListTag {
        ticks.iter().map(|t| Tag::Compound(t.to_tag())).collect()
    }
}

/// `ScheduledTick<T>` record. `ty` is vanilla's `type`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScheduledTick<T> {
    pub ty: T,
    pub pos: BlockPos,
    pub trigger_tick: i64,
    pub priority: TickPriority,
    pub sub_tick_order: i64,
}

impl<T> ScheduledTick<T> {
    /// The 4-argument constructor (`TickPriority.NORMAL`).
    pub fn new(ty: T, pos: BlockPos, trigger_tick: i64, sub_tick_order: i64) -> Self {
        Self {
            ty,
            pos,
            trigger_tick,
            priority: TickPriority::Normal,
            sub_tick_order,
        }
    }

    /// `ScheduledTick.probe`.
    pub fn probe(ty: T, pos: BlockPos) -> Self {
        Self::new(ty, pos, 0, 0)
    }

    /// `DRAIN_ORDER`: trigger tick, then priority, then sub tick order.
    pub fn drain_order(a: &Self, b: &Self) -> Ordering {
        a.trigger_tick
            .cmp(&b.trigger_tick)
            .then_with(|| Self::intra_tick_drain_order(a, b))
    }

    /// `INTRA_TICK_DRAIN_ORDER`: priority, then sub tick order.
    pub fn intra_tick_drain_order(a: &Self, b: &Self) -> Ordering {
        a.priority
            .cmp(&b.priority)
            .then_with(|| a.sub_tick_order.cmp(&b.sub_tick_order))
    }

    /// `toSavedTick(currentTick)`: the delay is `(int)(triggerTick - currentTick)`.
    pub fn to_saved_tick(self, current_tick: i64) -> SavedTick<T> {
        SavedTick {
            ty: self.ty,
            pos: self.pos,
            delay: self.trigger_tick.wrapping_sub(current_tick) as i32,
            priority: self.priority,
        }
    }
}

/// `UNIQUE_TICK_HASH` identity: type and position.
fn unique_key<T: Copy>(t: &ScheduledTick<T>) -> (T, BlockPos) {
    (t.ty, t.pos)
}

/// `TickAccess<T>`.
pub trait TickAccess<T> {
    fn schedule(&mut self, tick: ScheduledTick<T>);
    fn has_scheduled_tick(&self, pos: &BlockPos, ty: T) -> bool;
    fn count(&self) -> usize;
}

/// `TickContainerAccess<T>`.
pub trait TickContainerAccess<T>: TickAccess<T> {}

/// `LevelTickAccess<T>` (`willTickThisTick` may build a lookup set lazily, hence `&mut self`).
pub trait LevelTickAccess<T>: TickAccess<T> {
    fn will_tick_this_tick(&mut self, pos: &BlockPos, ty: T) -> bool;
}

/// `SerializableTickContainer<T>`.
pub trait SerializableTickContainer<T> {
    fn pack(&self, current_tick: i64) -> Vec<SavedTick<T>>;
}

/// `ChunkAccess.PackedTicks`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PackedTicks {
    pub blocks: Vec<SavedTick<Block>>,
    pub fluids: Vec<SavedTick<Fluid>>,
}

/// `BlackholeTickAccess`: both the container and the level variant (`emptyContainer()` / `emptyLevelList()`).
#[derive(Clone, Copy, Debug, Default)]
pub struct BlackholeTickAccess;

impl BlackholeTickAccess {
    pub fn empty_container() -> BlackholeTickAccess {
        BlackholeTickAccess
    }

    pub fn empty_level_list() -> BlackholeTickAccess {
        BlackholeTickAccess
    }
}

impl<T> TickAccess<T> for BlackholeTickAccess {
    fn schedule(&mut self, _tick: ScheduledTick<T>) {}
    fn has_scheduled_tick(&self, _pos: &BlockPos, _ty: T) -> bool {
        false
    }
    fn count(&self) -> usize {
        0
    }
}

impl<T> TickContainerAccess<T> for BlackholeTickAccess {}

impl<T> LevelTickAccess<T> for BlackholeTickAccess {
    fn will_tick_this_tick(&mut self, _pos: &BlockPos, _ty: T) -> bool {
        false
    }
}

/// The `Function<BlockPos, TickContainerAccess<T>>` of `WorldGenTickAccess`.
pub trait TickContainerGetter<T> {
    fn container(&self, pos: &BlockPos) -> &dyn TickContainerAccess<T>;
    fn container_mut(&mut self, pos: &BlockPos) -> &mut dyn TickContainerAccess<T>;
}

/// `WorldGenTickAccess`: forwards to the container owning the position; never ticks.
pub struct WorldGenTickAccess<G> {
    container_getter: G,
}

impl<G> WorldGenTickAccess<G> {
    pub fn new(container_getter: G) -> Self {
        Self { container_getter }
    }
}

impl<T, G: TickContainerGetter<T>> TickAccess<T> for WorldGenTickAccess<G> {
    fn schedule(&mut self, tick: ScheduledTick<T>) {
        let pos = tick.pos;
        self.container_getter.container_mut(&pos).schedule(tick);
    }

    fn has_scheduled_tick(&self, pos: &BlockPos, ty: T) -> bool {
        self.container_getter
            .container(pos)
            .has_scheduled_tick(pos, ty)
    }

    fn count(&self) -> usize {
        0
    }
}

impl<T, G: TickContainerGetter<T>> LevelTickAccess<T> for WorldGenTickAccess<G> {
    fn will_tick_this_tick(&mut self, _pos: &BlockPos, _ty: T) -> bool {
        false
    }
}

/// `ProtoChunkTicks<T>`: insertion-ordered, de-duplicated by (type, position); delays are not tracked.
#[derive(Clone, Debug)]
pub struct ProtoChunkTicks<T> {
    ticks: Vec<SavedTick<T>>,
    ticks_per_position: HashSet<(T, BlockPos)>,
}

impl<T> Default for ProtoChunkTicks<T> {
    fn default() -> Self {
        Self {
            ticks: Vec::new(),
            ticks_per_position: HashSet::new(),
        }
    }
}

impl<T: Copy + Eq + Hash> ProtoChunkTicks<T> {
    pub fn new() -> Self {
        Self::default()
    }

    fn schedule_saved(&mut self, new_tick: SavedTick<T>) {
        if self.ticks_per_position.insert((new_tick.ty, new_tick.pos)) {
            self.ticks.push(new_tick);
        }
    }

    /// `scheduledTicks()`.
    pub fn scheduled_ticks(&self) -> Vec<SavedTick<T>> {
        self.ticks.clone()
    }

    /// `ProtoChunkTicks.load`.
    pub fn load(ticks: &[SavedTick<T>]) -> Self {
        let mut result = Self::new();
        for t in ticks {
            result.schedule_saved(*t);
        }
        result
    }
}

impl<T: Copy + Eq + Hash> TickAccess<T> for ProtoChunkTicks<T> {
    fn schedule(&mut self, tick: ScheduledTick<T>) {
        self.schedule_saved(SavedTick {
            ty: tick.ty,
            pos: tick.pos,
            delay: 0,
            priority: tick.priority,
        });
    }

    fn has_scheduled_tick(&self, pos: &BlockPos, ty: T) -> bool {
        self.ticks_per_position.contains(&(ty, *pos))
    }

    fn count(&self) -> usize {
        self.ticks.len()
    }
}

impl<T: Copy + Eq + Hash> TickContainerAccess<T> for ProtoChunkTicks<T> {}

impl<T: Copy + Eq + Hash> SerializableTickContainer<T> for ProtoChunkTicks<T> {
    fn pack(&self, _current_tick: i64) -> Vec<SavedTick<T>> {
        self.ticks.clone()
    }
}

/// `LevelChunkTicks<T>`: per-chunk priority queue of [`ScheduledTick`] (a faithful `java.util.PriorityQueue`, so the
/// iteration order of `get_all` / `pack` / `remove_if` matches), plus ticks loaded from disk that are only turned into
/// scheduled ticks by [`LevelChunkTicks::unpack`]. The `onTickAdded` hook is handled by [`LevelTicks`].
#[derive(Clone, Debug)]
pub struct LevelChunkTicks<T> {
    tick_queue: JavaPriorityQueue<ScheduledTick<T>>,
    pending_ticks: Option<Vec<SavedTick<T>>>,
    ticks_per_position: HashSet<(T, BlockPos)>,
}

impl<T: Copy + Eq + Hash> Default for LevelChunkTicks<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: Copy + Eq + Hash> LevelChunkTicks<T> {
    pub fn new() -> Self {
        Self {
            tick_queue: JavaPriorityQueue::new(ScheduledTick::drain_order),
            pending_ticks: None,
            ticks_per_position: HashSet::new(),
        }
    }

    /// `LevelChunkTicks(List<SavedTick<T>> pendingTicks)`.
    pub fn from_pending(pending_ticks: Vec<SavedTick<T>>) -> Self {
        let ticks_per_position = pending_ticks.iter().map(|t| (t.ty, t.pos)).collect();
        Self {
            tick_queue: JavaPriorityQueue::new(ScheduledTick::drain_order),
            pending_ticks: Some(pending_ticks),
            ticks_per_position,
        }
    }

    pub fn peek(&self) -> Option<&ScheduledTick<T>> {
        self.tick_queue.peek()
    }

    pub fn poll(&mut self) -> Option<ScheduledTick<T>> {
        let result = self.tick_queue.poll()?;
        self.ticks_per_position.remove(&unique_key(&result));
        Some(result)
    }

    /// `schedule`; returns whether the tick was added (it is dropped when (type, position) is already scheduled).
    pub fn schedule_tick(&mut self, tick: ScheduledTick<T>) -> bool {
        if self.ticks_per_position.insert(unique_key(&tick)) {
            self.tick_queue.add(tick);
            true
        } else {
            false
        }
    }

    pub fn remove_if(&mut self, mut test: impl FnMut(&ScheduledTick<T>) -> bool) {
        let set = &mut self.ticks_per_position;
        self.tick_queue.remove_if(&mut test, |t| {
            set.remove(&unique_key(&t));
        });
    }

    /// `getAll()`: queue (heap array) order.
    pub fn get_all(&self) -> impl Iterator<Item = &ScheduledTick<T>> {
        self.tick_queue.iter()
    }

    /// `unpack(currentTick)`: pending saved ticks become scheduled ticks with sub tick orders `-n, -n+1, ...`.
    pub fn unpack(&mut self, current_tick: i64) {
        if let Some(pending) = self.pending_ticks.take() {
            let first_sub_tick_order = -(pending.len() as i64);
            for (sub_tick_base, tick) in (first_sub_tick_order..).zip(pending) {
                self.tick_queue
                    .add(tick.unpack(current_tick, sub_tick_base));
            }
        }
    }
}

impl<T: Copy + Eq + Hash> TickAccess<T> for LevelChunkTicks<T> {
    fn schedule(&mut self, tick: ScheduledTick<T>) {
        self.schedule_tick(tick);
    }

    fn has_scheduled_tick(&self, pos: &BlockPos, ty: T) -> bool {
        self.ticks_per_position.contains(&(ty, *pos))
    }

    fn count(&self) -> usize {
        self.tick_queue.len() + self.pending_ticks.as_ref().map_or(0, Vec::len)
    }
}

impl<T: Copy + Eq + Hash> TickContainerAccess<T> for LevelChunkTicks<T> {}

impl<T: Copy + Eq + Hash> SerializableTickContainer<T> for LevelChunkTicks<T> {
    fn pack(&self, current_tick: i64) -> Vec<SavedTick<T>> {
        let mut ticks = Vec::with_capacity(self.tick_queue.len());
        if let Some(pending) = &self.pending_ticks {
            ticks.extend_from_slice(pending);
        }
        let mut sorted: Vec<ScheduledTick<T>> = self.tick_queue.iter().copied().collect();
        sorted.sort_by_key(|t| t.sub_tick_order); // stable, like `List.sort`
        ticks.extend(sorted.into_iter().map(|t| t.to_saved_tick(current_tick)));
        ticks
    }
}

/// Snapshot of a container's head tick while the container sits in `containersToTick` (it is not modified there).
struct QueuedContainer<T> {
    chunk_key: i64,
    next: ScheduledTick<T>,
}

fn container_drain_order<T>(a: &QueuedContainer<T>, b: &QueuedContainer<T>) -> Ordering {
    ScheduledTick::intra_tick_drain_order(&a.next, &b.next)
}

fn for_containers_in_area(area: &BoundingBox) -> impl Iterator<Item = i64> + use<> {
    let x_min = SectionPos::block_to_section_coord_i32(area.min_x());
    let z_min = SectionPos::block_to_section_coord_i32(area.min_z());
    let x_max = SectionPos::block_to_section_coord_i32(area.max_x());
    let z_max = SectionPos::block_to_section_coord_i32(area.max_z());
    (x_min..=x_max).flat_map(move |x| (z_min..=z_max).map(move |z| ChunkPos::pack_xz(x, z)))
}

/// `LevelTicks<T>`: all loaded chunks' tick containers and the per-game-tick drain.
pub struct LevelTicks<T> {
    all_containers: HashMap<i64, LevelChunkTicks<T>>,
    next_tick_for_container: Long2LongOpenHashMap,
    containers_to_tick: JavaPriorityQueue<QueuedContainer<T>>,
    to_run_this_tick: VecDeque<ScheduledTick<T>>,
    already_run_this_tick: Vec<ScheduledTick<T>>,
    to_run_this_tick_set: HashSet<(T, BlockPos)>,
}

impl<T: Copy + Eq + Hash> Default for LevelTicks<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: Copy + Eq + Hash> LevelTicks<T> {
    pub fn new() -> Self {
        Self {
            all_containers: HashMap::new(),
            next_tick_for_container: Long2LongOpenHashMap::default(),
            containers_to_tick: JavaPriorityQueue::new(container_drain_order::<T>),
            to_run_this_tick: VecDeque::new(),
            already_run_this_tick: Vec::new(),
            to_run_this_tick_set: HashSet::new(),
        }
    }

    pub fn add_container(&mut self, pos: ChunkPos, container: LevelChunkTicks<T>) {
        let pos_key = pos.pack();
        if let Some(next) = container.peek() {
            self.next_tick_for_container.put(pos_key, next.trigger_tick);
        }
        self.all_containers.insert(pos_key, container);
    }

    /// `removeContainer`; vanilla only detaches the callback, here the container itself is handed back.
    pub fn remove_container(&mut self, pos: ChunkPos) -> Option<LevelChunkTicks<T>> {
        let key = pos.pack();
        let removed = self.all_containers.remove(&key);
        self.next_tick_for_container.remove(key);
        removed
    }

    pub fn container(&self, pos: ChunkPos) -> Option<&LevelChunkTicks<T>> {
        self.all_containers.get(&pos.pack())
    }

    fn update_container_scheduling(map: &mut Long2LongOpenHashMap, next_tick: &ScheduledTick<T>) {
        map.put(
            ChunkPos::pack_block_pos(&next_tick.pos),
            next_tick.trigger_tick,
        );
    }

    /// `tick(currentTick, maxTicksToProcess, output)` for callers that need no re-entrancy.
    pub fn tick(
        &mut self,
        current_tick: i64,
        max_ticks_to_process: usize,
        tick_check: impl FnMut(i64) -> bool,
        mut output: impl FnMut(BlockPos, T),
    ) {
        self.collect_ticks(current_tick, max_ticks_to_process, tick_check);
        while let Some((pos, ty)) = self.poll_collected_tick() {
            output(pos, ty);
        }
        self.cleanup_after_tick();
    }

    /// `collectTicks`: fills the run queue (`tick_check` is vanilla's `tickCheck.test(chunkPos)`).
    pub fn collect_ticks(
        &mut self,
        current_tick: i64,
        max_ticks_to_process: usize,
        mut tick_check: impl FnMut(i64) -> bool,
    ) {
        // sortContainersToTick
        let all = &self.all_containers;
        let queue = &mut self.containers_to_tick;
        self.next_tick_for_container
            .fast_iterate(|chunk_pos, next_tick| {
                if *next_tick > current_tick {
                    return false;
                }
                let Some(next) = all.get(&chunk_pos).and_then(LevelChunkTicks::peek) else {
                    return true;
                };
                if next.trigger_tick > current_tick {
                    *next_tick = next.trigger_tick;
                    false
                } else if tick_check(chunk_pos) {
                    queue.add(QueuedContainer {
                        chunk_key: chunk_pos,
                        next: *next,
                    });
                    true
                } else {
                    false
                }
            });
        // drainContainers
        while self.to_run_this_tick.len() < max_ticks_to_process {
            let Some(top) = self.containers_to_tick.poll() else {
                break;
            };
            let Some(container) = self.all_containers.get_mut(&top.chunk_key) else {
                continue;
            };
            if let Some(tick) = container.poll() {
                self.to_run_this_tick.push_back(tick);
            }
            // drainFromCurrentContainer
            if self.to_run_this_tick.len() < max_ticks_to_process {
                let next_from_next_container = self.containers_to_tick.peek().map(|q| q.next);
                while self.to_run_this_tick.len() < max_ticks_to_process {
                    let Some(next_from_current) = container.peek().copied() else {
                        break;
                    };
                    if next_from_current.trigger_tick > current_tick
                        || next_from_next_container.is_some_and(|n| {
                            ScheduledTick::intra_tick_drain_order(&next_from_current, &n)
                                == Ordering::Greater
                        })
                    {
                        break;
                    }
                    container.poll();
                    self.to_run_this_tick.push_back(next_from_current);
                }
            }
            if let Some(next_tick) = container.peek().copied() {
                if next_tick.trigger_tick <= current_tick
                    && self.to_run_this_tick.len() < max_ticks_to_process
                {
                    self.containers_to_tick.add(QueuedContainer {
                        chunk_key: top.chunk_key,
                        next: next_tick,
                    });
                } else {
                    Self::update_container_scheduling(
                        &mut self.next_tick_for_container,
                        &next_tick,
                    );
                }
            }
        }
        // rescheduleLeftoverContainers
        for queued in self.containers_to_tick.iter() {
            Self::update_container_scheduling(&mut self.next_tick_for_container, &queued.next);
        }
    }

    /// One step of `runCollectedTicks`: the next collected tick (the caller runs it).
    pub fn poll_collected_tick(&mut self) -> Option<(BlockPos, T)> {
        let entry = self.to_run_this_tick.pop_front()?;
        if !self.to_run_this_tick_set.is_empty() {
            self.to_run_this_tick_set.remove(&unique_key(&entry));
        }
        self.already_run_this_tick.push(entry);
        Some((entry.pos, entry.ty))
    }

    /// `cleanupAfterTick`.
    pub fn cleanup_after_tick(&mut self) {
        self.to_run_this_tick.clear();
        self.containers_to_tick.clear();
        self.already_run_this_tick.clear();
        self.to_run_this_tick_set.clear();
    }

    pub fn clear_area(&mut self, area: &BoundingBox) {
        let inside = |t: &ScheduledTick<T>| area.is_inside(&t.pos);
        for pos in for_containers_in_area(area) {
            let Some(container) = self.all_containers.get_mut(&pos) else {
                continue;
            };
            let previous_top = container.peek().copied();
            container.remove_if(inside);
            let new_top = container.peek().copied();
            if new_top != previous_top {
                match new_top {
                    Some(t) => {
                        Self::update_container_scheduling(&mut self.next_tick_for_container, &t)
                    }
                    None => self.next_tick_for_container.remove(pos),
                }
            }
        }
        self.already_run_this_tick.retain(|t| !inside(t));
        self.to_run_this_tick.retain(|t| !inside(t));
    }

    fn ticks_in_area(&self, area: &BoundingBox) -> Vec<ScheduledTick<T>> {
        let mut ticks: Vec<ScheduledTick<T>> = Vec::new();
        let inside = |t: &&ScheduledTick<T>| area.is_inside(&t.pos);
        ticks.extend(self.already_run_this_tick.iter().filter(inside));
        ticks.extend(self.to_run_this_tick.iter().filter(inside));
        for pos in for_containers_in_area(area) {
            if let Some(container) = self.all_containers.get(&pos) {
                ticks.extend(container.get_all().filter(inside));
            }
        }
        ticks
    }

    fn schedule_copies(&mut self, ticks: Vec<ScheduledTick<T>>, offset: &Vec3i) {
        let min_sub_tick = ticks
            .iter()
            .map(|t| t.sub_tick_order)
            .min()
            .unwrap_or(i64::MAX);
        let max_sub_tick = ticks
            .iter()
            .map(|t| t.sub_tick_order)
            .max()
            .unwrap_or(i64::MIN);
        for t in ticks {
            self.schedule_tick(ScheduledTick {
                ty: t.ty,
                pos: t.pos.offset_vec(offset),
                trigger_tick: t.trigger_tick,
                priority: t.priority,
                sub_tick_order: t
                    .sub_tick_order
                    .wrapping_sub(min_sub_tick)
                    .wrapping_add(max_sub_tick)
                    .wrapping_add(1),
            });
        }
    }

    pub fn copy_area(&mut self, area: &BoundingBox, offset: &Vec3i) {
        let ticks = self.ticks_in_area(area);
        self.schedule_copies(ticks, offset);
    }

    pub fn copy_area_from(&mut self, source: &LevelTicks<T>, area: &BoundingBox, offset: &Vec3i) {
        let ticks = source.ticks_in_area(area);
        self.schedule_copies(ticks, offset);
    }

    /// `schedule`; a tick for an unloaded chunk is dropped (vanilla logs and pauses in an IDE only).
    pub fn schedule_tick(&mut self, tick: ScheduledTick<T>) {
        let Some(container) = self
            .all_containers
            .get_mut(&ChunkPos::pack_block_pos(&tick.pos))
        else {
            return;
        };
        // the container's `onTickAdded` callback: a new head tick updates the chunk's next trigger tick
        if container.schedule_tick(tick) && container.peek() == Some(&tick) {
            Self::update_container_scheduling(&mut self.next_tick_for_container, &tick);
        }
    }
}

impl<T: Copy + Eq + Hash> TickAccess<T> for LevelTicks<T> {
    fn schedule(&mut self, tick: ScheduledTick<T>) {
        self.schedule_tick(tick);
    }

    fn has_scheduled_tick(&self, pos: &BlockPos, ty: T) -> bool {
        self.all_containers
            .get(&ChunkPos::pack_block_pos(pos))
            .is_some_and(|c| c.has_scheduled_tick(pos, ty))
    }

    fn count(&self) -> usize {
        self.all_containers.values().map(TickAccess::count).sum()
    }
}

impl<T: Copy + Eq + Hash> LevelTickAccess<T> for LevelTicks<T> {
    fn will_tick_this_tick(&mut self, pos: &BlockPos, ty: T) -> bool {
        if self.to_run_this_tick_set.is_empty() && !self.to_run_this_tick.is_empty() {
            self.to_run_this_tick_set
                .extend(self.to_run_this_tick.iter().map(unique_key));
        }
        self.to_run_this_tick_set.contains(&(ty, *pos))
    }
}
