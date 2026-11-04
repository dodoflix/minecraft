//! `PlayerChunkSender`: per-player chunk batching with the client-driven rate control
//! (`ServerboundChunkBatchReceivedPacket.desiredChunksPerTick`). The chunk map marks chunks as pending
//! (`mark_chunk_pending_to_send`), `send_next_chunks` runs once per tick from `MinecraftServer.tickChildren`
//! ("send chunks") and sends at most one batch.
//!
//! The handle is cheaply cloneable: the listener keeps one, the chunk map keeps another one boxed as a
//! [`ChunkSendSink`]; both see the same state (game thread only, the mutex is uncontended).

use super::ConnectionSender;
use crate::util::long_open_hash_set::LongOpenHashSet;
use mc_core::pos::ChunkPos;
use mc_protocol::packets::play::{ClientboundChunkBatchFinishedPacket, ClientboundLevelChunkWithLightPacket, ClientboundChunkBatchStartPacket, ClientboundForgetLevelChunkPacket};
use parking_lot::Mutex;
use std::sync::Arc;

pub const MIN_CHUNKS_PER_TICK: f32 = 0.01;
pub const MAX_CHUNKS_PER_TICK: f32 = 64.0;
const START_CHUNKS_PER_TICK: f32 = 9.0;
const MAX_UNACKNOWLEDGED_BATCHES: i32 = 10;

/// What `sendNextChunks` needs from the level: `ChunkMap.getChunkToSend(pos)` plus
/// `new ClientboundLevelChunkWithLightPacket(chunk, level.getLightEngine(), null, null)`.
pub trait ChunkPacketSource {
    /// Encoded packet payload (`VarInt id + body`) of the chunk at `pos` if it is sendable (vanilla: `getChunkToSend != null`).
    fn chunk_packet(&mut self, pos: ChunkPos) -> Option<Arc<[u8]>>;
    /// `level.debugSynchronizers().startTrackingChunk(player, pos)` after a chunk was sent.
    fn start_tracking_chunk(&mut self, _pos: ChunkPos) {}
}

struct Inner {
    pending_chunks: LongOpenHashSet,
    memory_connection: bool,
    desired_chunks_per_tick: f32,
    batch_quota: f32,
    unacknowledged_batches: i32,
    max_unacknowledged_batches: i32,
}

#[derive(Clone)]
pub struct PlayerChunkSender {
    inner: Arc<Mutex<Inner>>,
    connection: ConnectionSender,
}

impl PlayerChunkSender {
    pub fn new(connection: ConnectionSender, memory_connection: bool) -> Self {
        Self {
            inner: Arc::new(Mutex::new(Inner {
                pending_chunks: LongOpenHashSet::default(),
                memory_connection,
                desired_chunks_per_tick: START_CHUNKS_PER_TICK,
                batch_quota: 0.0,
                unacknowledged_batches: 0,
                max_unacknowledged_batches: 1,
            })),
            connection,
        }
    }

    pub fn mark_chunk_pending_to_send(&self, pos: ChunkPos) {
        self.inner.lock().pending_chunks.add(pos.pack());
    }

    /// `dropChunk(player, pos)`: `player_alive` = `player.isAlive()`.
    pub fn drop_chunk(&self, pos: ChunkPos, player_alive: bool) {
        if !self.inner.lock().pending_chunks.remove(pos.pack()) && player_alive {
            self.connection.send(&ClientboundForgetLevelChunkPacket { pos });
        }
    }

    pub fn is_pending(&self, pos: i64) -> bool {
        self.inner.lock().pending_chunks.contains(pos)
    }

    /// `sendNextChunks(player)`; `player_pos` = `player.chunkPosition()`.
    pub fn send_next_chunks(&self, player_pos: ChunkPos, source: &mut impl ChunkPacketSource) {
        let mut inner = self.inner.lock();
        if inner.unacknowledged_batches >= inner.max_unacknowledged_batches {
            return;
        }
        let max_batch_size = inner.desired_chunks_per_tick.max(1.0);
        inner.batch_quota = (inner.batch_quota + inner.desired_chunks_per_tick).min(max_batch_size);
        if inner.batch_quota < 1.0 || inner.pending_chunks.is_empty() {
            return;
        }
        let chunks_to_send = inner.collect_chunks_to_send(player_pos, source);
        if chunks_to_send.is_empty() {
            return;
        }
        inner.unacknowledged_batches += 1;
        self.connection.send(&ClientboundChunkBatchStartPacket);
        for (pos, payload) in &chunks_to_send {
            self.connection.send_payload(payload.clone());
            source.start_tracking_chunk(*pos);
        }
        self.connection.send(&ClientboundChunkBatchFinishedPacket { batch_size: chunks_to_send.len() as i32 });
        inner.batch_quota -= chunks_to_send.len() as f32;
    }

    /// `onChunkBatchReceivedByClient(desiredChunksPerTick)`.
    pub fn on_chunk_batch_received_by_client(&self, desired_chunks_per_tick: f32) {
        let mut inner = self.inner.lock();
        inner.unacknowledged_batches -= 1;
        if inner.unacknowledged_batches < 0 {
            inner.unacknowledged_batches = 0;
        }
        inner.desired_chunks_per_tick =
            if desired_chunks_per_tick.is_nan() { MIN_CHUNKS_PER_TICK } else { mc_core::math::mth::clamp_f32(desired_chunks_per_tick, MIN_CHUNKS_PER_TICK, MAX_CHUNKS_PER_TICK) };
        if inner.unacknowledged_batches == 0 {
            inner.batch_quota = 1.0;
        }
        inner.max_unacknowledged_batches = MAX_UNACKNOWLEDGED_BATCHES;
    }
}

impl crate::server::level::chunk_send_sink::ChunkSendSink for PlayerChunkSender {
    fn mark_chunk_pending_to_send(&mut self, pos: ChunkPos) {
        PlayerChunkSender::mark_chunk_pending_to_send(self, pos);
    }
    fn drop_chunk(&mut self, pos: ChunkPos, player_alive: bool) {
        PlayerChunkSender::drop_chunk(self, pos, player_alive);
    }
    fn is_pending(&self, pos_packed: i64) -> bool {
        PlayerChunkSender::is_pending(self, pos_packed)
    }
}

/// `ChunkMap.getChunkToSend` + `new ClientboundLevelChunkWithLightPacket(chunk, level.getLightEngine(), null, null)`.
pub struct LevelChunkSource<'a> {
    pub level: &'a crate::server::level::ServerLevel,
}

impl ChunkPacketSource for LevelChunkSource<'_> {
    fn chunk_packet(&mut self, pos: ChunkPos) -> Option<Arc<[u8]>> {
        let chunk = self.level.chunk_source.chunk_map.get_chunk_to_send(pos.pack())?;
        let packet = level_chunk_with_light_packet(self.level, chunk);
        let mut buf = Vec::with_capacity(4096);
        mc_protocol::codec::write_varint(&mut buf, <ClientboundLevelChunkWithLightPacket as mc_protocol::packet::Packet>::ID);
        mc_protocol::Encode::encode(&packet, &mut buf);
        Some(buf.into())
    }
}

/// `new ClientboundLevelChunkWithLightPacket(chunk, lightEngine, null, null)`: all sections, all light.
/// Block entities are the chunk's live instances (`getBlockEntities()`); the light is whatever the level's light engine
/// holds.
pub fn level_chunk_with_light_packet(level: &crate::server::level::ServerLevel, chunk: &crate::world::level::chunk::LevelChunk) -> ClientboundLevelChunkWithLightPacket {
    use mc_protocol::packets::play::level::{BlockEntityInfo, ClientboundLevelChunkPacketData, ClientboundLightUpdatePacketData, HeightmapTypes};
    let pos = chunk.pos();
    let access = &chunk.data.access;
    let block_entities: Vec<mc_world::chunk_packet::BlockEntityInfo> = chunk
        .block_entities
        .iter()
        .map(|(p, be)| mc_world::chunk_packet::BlockEntityInfo::create(p.x(), p.y(), p.z(), be.get_type(), be.get_update_tag()))
        .collect();
    let heightmaps = access.heightmaps.iter().map(|(ty, h)| (*ty, h.get_raw_data()));
    let data = mc_world::chunk_packet::LevelChunkPacketData::new(&access.sections, heightmaps, block_entities)
        .expect("chunk sections serialize into their computed size");
    let chunk_data = ClientboundLevelChunkPacketData {
        heightmaps: data.heightmaps.into_iter().map(|(ty, longs)| (HeightmapTypes::by_id(i32::from(ty.id())), longs)).collect(),
        buffer: data.buffer,
        block_entities_data: data
            .block_entities
            .into_iter()
            .map(|b| BlockEntityInfo { packed_xz: b.packed_xz as i8, y: b.y, r#type: b.block_entity_type, tag: b.tag })
            .collect(),
    };
    // `ClientboundLightUpdatePacketData(pos, lightEngine, null, null)`: every section
    let data = level.light_engine().engine().light_update_packet_data(pos, None, None);
    let to_protocol = |b: &mc_world::chunk_packet::BitSet| mc_protocol::codec::BitSet::from_bytes(&b.to_bytes());
    let light_data = ClientboundLightUpdatePacketData {
        sky_y_mask: to_protocol(&data.sky_y_mask),
        block_y_mask: to_protocol(&data.block_y_mask),
        empty_sky_y_mask: to_protocol(&data.empty_sky_y_mask),
        empty_block_y_mask: to_protocol(&data.empty_block_y_mask),
        sky_updates: data.sky_updates,
        block_updates: data.block_updates,
    };
    ClientboundLevelChunkWithLightPacket { x: pos.x, z: pos.z, chunk_data, light_data }
}

impl Inner {
    /// `collectChunksToSend(chunkMap, playerPos)`.
    fn collect_chunks_to_send(&mut self, player_pos: ChunkPos, source: &mut impl ChunkPacketSource) -> Vec<(ChunkPos, Arc<[u8]>)> {
        let max_batch_size = mc_core::math::mth::floor_f32(self.batch_quota);
        let dist = |packed: i64| player_pos.distance_squared_long(packed);
        let chunks: Vec<(ChunkPos, Arc<[u8]>)> = if !self.memory_connection && self.pending_chunks.len() as i64 > i64::from(max_batch_size) {
            let selected = least(max_batch_size.max(0) as usize, self.pending_chunks.iter(), |a, b| dist(*a).cmp(&dist(*b)));
            selected
                .into_iter()
                .filter_map(|packed| {
                    let pos = ChunkPos::unpack(packed);
                    source.chunk_packet(pos).map(|p| (pos, p))
                })
                .collect()
        } else {
            let mut all: Vec<(ChunkPos, Arc<[u8]>)> = self
                .pending_chunks
                .iter()
                .filter_map(|packed| {
                    let pos = ChunkPos::unpack(packed);
                    source.chunk_packet(pos).map(|p| (pos, p))
                })
                .collect();
            all.sort_by_key(|(pos, _)| player_pos.distance_squared(pos));
            all
        };
        for (pos, _) in &chunks {
            self.pending_chunks.remove(pos.pack());
        }
        chunks
    }
}

/// Guava `Comparators.least(k, comparator)` collected sequentially (`TopKSelector`): the `k` least elements in
/// ascending order. The partition/trim steps are reproduced because the order of elements that compare equal
/// (here: chunks at the same distance) depends on them.
pub fn least<T: Copy>(k: usize, items: impl Iterator<Item = T>, cmp: impl Fn(&T, &T) -> std::cmp::Ordering) -> Vec<T> {
    if k == 0 {
        return Vec::new();
    }
    let mut buffer: Vec<Option<T>> = vec![None; 2 * k];
    let mut buffer_size = 0usize;
    let mut threshold: Option<T> = None;
    let get = |buffer: &Vec<Option<T>>, i: usize| buffer[i].expect("filled");
    for elem in items {
        if buffer_size == 0 {
            buffer[0] = Some(elem);
            threshold = Some(elem);
            buffer_size = 1;
        } else if buffer_size < k {
            buffer[buffer_size] = Some(elem);
            buffer_size += 1;
            if cmp(&elem, &threshold.expect("set")).is_gt() {
                threshold = Some(elem);
            }
        } else if cmp(&elem, &threshold.expect("set")).is_lt() {
            buffer[buffer_size] = Some(elem);
            buffer_size += 1;
            if buffer_size == 2 * k {
                // trim()
                let mut left = 0usize;
                let mut right = 2 * k - 1;
                let mut min_threshold_position = 0usize;
                let mut iterations = 0;
                let max_iterations = ceil_log2(right - left) * 3;
                while left < right {
                    let pivot_index = (left + right + 1) >> 1;
                    // partition(left, right, pivotIndex)
                    let pivot_value = get(&buffer, pivot_index);
                    buffer[pivot_index] = buffer[right];
                    let mut pivot_new_index = left;
                    for i in left..right {
                        if cmp(&get(&buffer, i), &pivot_value).is_lt() {
                            buffer.swap(pivot_new_index, i);
                            pivot_new_index += 1;
                        }
                    }
                    buffer[right] = buffer[pivot_new_index];
                    buffer[pivot_new_index] = Some(pivot_value);
                    if pivot_new_index > k {
                        right = pivot_new_index - 1;
                    } else if pivot_new_index < k {
                        left = pivot_new_index.max(left + 1);
                        min_threshold_position = pivot_new_index;
                    } else {
                        break;
                    }
                    iterations += 1;
                    if iterations >= max_iterations {
                        buffer[left..=right].sort_by(|a, b| cmp(&a.expect("filled"), &b.expect("filled")));
                        break;
                    }
                }
                buffer_size = k;
                let mut t = get(&buffer, min_threshold_position);
                for i in min_threshold_position + 1..k {
                    if cmp(&get(&buffer, i), &t).is_gt() {
                        t = get(&buffer, i);
                    }
                }
                threshold = Some(t);
            }
        }
    }
    // topK()
    let mut out: Vec<T> = buffer[..buffer_size].iter().map(|e| e.expect("filled")).collect();
    out.sort_by(|a, b| cmp(a, b));
    out.truncate(k);
    out
}

/// `IntMath.log2(x, RoundingMode.CEILING)` for `x > 0`.
fn ceil_log2(x: usize) -> i32 {
    if x <= 1 { 0 } else { (usize::BITS - (x - 1).leading_zeros()) as i32 }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::mpsc::unbounded_channel;

    struct Src;
    impl ChunkPacketSource for Src {
        fn chunk_packet(&mut self, pos: ChunkPos) -> Option<Arc<[u8]>> {
            Some(vec![0x2c, pos.x as u8, pos.z as u8].into())
        }
    }

    fn sender() -> (PlayerChunkSender, tokio::sync::mpsc::UnboundedReceiver<super::super::Outbound>) {
        let (tx, rx) = unbounded_channel();
        (PlayerChunkSender::new(ConnectionSender::new(tx), false), rx)
    }

    fn drain(rx: &mut tokio::sync::mpsc::UnboundedReceiver<super::super::Outbound>) -> Vec<Vec<u8>> {
        let mut out = vec![];
        while let Ok(super::super::Outbound::Payload(p)) = rx.try_recv() {
            out.push(p.to_vec());
        }
        out
    }

    #[test]
    fn batches_follow_the_vanilla_rate_control() {
        let (s, mut rx) = sender();
        for x in 0..20 {
            s.mark_chunk_pending_to_send(ChunkPos::new(x, 0));
        }
        let center = ChunkPos::new(0, 0);
        // tick 1: quota = min(0+9, max(1,9)) = 9 -> batch of 9 nearest chunks
        s.send_next_chunks(center, &mut Src);
        let sent = drain(&mut rx);
        assert_eq!(sent.len(), 9 + 2, "batch start + 9 chunks + batch finished");
        let xs: Vec<u8> = sent[1..10].iter().map(|p| p[1]).collect();
        assert_eq!(xs.iter().copied().collect::<std::collections::BTreeSet<_>>(), (0..9).collect());
        // unacknowledged batch limit is 1 until the client answers
        s.send_next_chunks(center, &mut Src);
        assert!(drain(&mut rx).is_empty());
        // client acknowledges, wants 2 per tick
        s.on_chunk_batch_received_by_client(2.0);
        s.send_next_chunks(center, &mut Src); // quota = min(1+2, max(1,2)) = 2
        assert_eq!(drain(&mut rx).len(), 2 + 2);
    }

    #[test]
    fn desired_rate_is_clamped_and_nan_is_minimum() {
        let (s, _rx) = sender();
        s.on_chunk_batch_received_by_client(f32::NAN);
        assert_eq!(s.inner.lock().desired_chunks_per_tick, MIN_CHUNKS_PER_TICK);
        s.on_chunk_batch_received_by_client(1000.0);
        assert_eq!(s.inner.lock().desired_chunks_per_tick, MAX_CHUNKS_PER_TICK);
        assert_eq!(s.inner.lock().unacknowledged_batches, 0);
    }

    #[test]
    fn least_returns_k_smallest_sorted() {
        let v: Vec<i32> = (0..100).map(|i| (i * 37) % 101).collect();
        let r = least(5, v.iter().copied(), |a, b| a.cmp(b));
        let mut expect = v.clone();
        expect.sort();
        assert_eq!(r, expect[..5]);
    }
}
