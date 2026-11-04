//! Worker threads of the chunk map: the region file IO thread (vanilla's `IOWorker`), a small pool for chunk NBT
//! parsing/encoding, and the worldgen thread (vanilla's serial `worldgen` `ConsecutiveExecutor`). Results come back
//! to the game thread through one channel of [`WorkerMessage`]s which the chunk map polls (`ServerChunkCache.pollTask`).

use super::chunk_status_tasks::{BatchResult, GenBatch, run_batch};
use super::world_gen::WorldGenContext;
use mc_core::pos::ChunkPos;
use mc_nbt::CompoundTag;
use mc_world::region::{RegionStorageInfo, SimpleRegionStorage};
use mc_world::serial::{ChunkReadException, SerializableChunkData};
use std::io;
use std::path::PathBuf;
use std::sync::atomic::{AtomicI32, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

/// What a worker thread hands back to the game thread.
pub enum WorkerMessage {
    ChunkRead { key: i64, result: io::Result<Option<CompoundTag>> },
    ChunkParsed { key: i64, result: Result<Option<SerializableChunkData>, ChunkReadException> },
    BatchDone(Box<BatchResult>),
}

enum IoRequest {
    Read { key: i64 },
    ReadBlocking { key: i64, reply: Sender<io::Result<Option<CompoundTag>>> },
    Write { pos: ChunkPos, tag: Option<CompoundTag>, active_writes: Option<Arc<AtomicI32>> },
    Synchronize { flush: bool, done: Sender<io::Result<()>> },
    Close { done: Sender<io::Result<()>> },
}

/// `IOWorker` over a [`SimpleRegionStorage`] on its own thread.
pub struct RegionIo {
    tx: Sender<IoRequest>,
    thread: Option<JoinHandle<()>>,
}

impl RegionIo {
    pub fn new(info: RegionStorageInfo, folder: PathBuf, sync_writes: bool, results: Sender<WorkerMessage>) -> Self {
        let (tx, rx) = channel::<IoRequest>();
        let thread = std::thread::Builder::new()
            .name(format!("IO-Worker-{}", info.dimension))
            .spawn(move || io_loop(SimpleRegionStorage::new(info, folder, sync_writes), rx, results))
            .expect("spawn IO worker");
        Self { tx, thread: Some(thread) }
    }

    /// `read(pos)`: the result arrives as [`WorkerMessage::ChunkRead`].
    pub fn read(&self, pos: ChunkPos) {
        let _ = self.tx.send(IoRequest::Read { key: pos.pack() });
    }

    /// `readChunk(pos).join()`.
    pub fn read_blocking(&self, pos: ChunkPos) -> io::Result<Option<CompoundTag>> {
        let (reply, rx) = channel();
        let _ = self.tx.send(IoRequest::ReadBlocking { key: pos.pack(), reply });
        rx.recv().unwrap_or_else(|_| Err(io::Error::other("IO worker is gone")))
    }

    /// A clonable write-only handle for worker threads.
    pub fn handle(&self) -> RegionIoHandle {
        RegionIoHandle { tx: self.tx.clone() }
    }

    /// `write(pos, tag)`; `active_writes` is decremented once the chunk reached the region file.
    pub fn write(&self, pos: ChunkPos, tag: Option<CompoundTag>, active_writes: Option<Arc<AtomicI32>>) {
        let _ = self.tx.send(IoRequest::Write { pos, tag, active_writes });
    }

    /// `synchronize(flush).join()`.
    pub fn synchronize(&self, flush: bool) -> io::Result<()> {
        let (done, rx) = channel();
        let _ = self.tx.send(IoRequest::Synchronize { flush, done });
        rx.recv().unwrap_or_else(|_| Err(io::Error::other("IO worker is gone")))
    }

    /// `close()`: drains the queued writes and closes the region files.
    pub fn close(&mut self) -> io::Result<()> {
        let (done, rx) = channel();
        let _ = self.tx.send(IoRequest::Close { done });
        let result = rx.recv().unwrap_or(Ok(()));
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
        result
    }
}

/// Write side of a [`RegionIo`] usable from pool threads.
#[derive(Clone)]
pub struct RegionIoHandle {
    tx: Sender<IoRequest>,
}

impl RegionIoHandle {
    pub fn write(&self, pos: ChunkPos, tag: Option<CompoundTag>, active_writes: Option<Arc<AtomicI32>>) {
        let _ = self.tx.send(IoRequest::Write { pos, tag, active_writes });
    }
}

fn io_loop(mut storage: SimpleRegionStorage, rx: Receiver<IoRequest>, results: Sender<WorkerMessage>) {
    // Mirror of the storage's pending-write order (same insertion semantics): the counters to release per chunk.
    let mut pending: indexmap::IndexMap<i64, Vec<Arc<AtomicI32>>> = indexmap::IndexMap::new();
    fn release(counters: Vec<Arc<AtomicI32>>) {
        for c in counters {
            c.fetch_sub(1, Ordering::AcqRel);
        }
    }
    loop {
        match rx.recv_timeout(Duration::from_millis(5)) {
            Ok(IoRequest::Read { key }) => {
                let result = storage.read(&ChunkPos::unpack(key));
                if results.send(WorkerMessage::ChunkRead { key, result }).is_err() {
                    return;
                }
            }
            Ok(IoRequest::ReadBlocking { key, reply }) => {
                let _ = reply.send(storage.read(&ChunkPos::unpack(key)));
            }
            Ok(IoRequest::Write { pos, tag, active_writes }) => {
                storage.write(&pos, tag);
                let entry = pending.entry(pos.pack()).or_default();
                entry.extend(active_writes);
            }
            Ok(IoRequest::Synchronize { flush, done }) => {
                let r = storage.synchronize(flush);
                pending.drain(..).for_each(|(_, v)| release(v));
                let _ = done.send(r);
            }
            Ok(IoRequest::Close { done }) => {
                let r = storage.close();
                pending.drain(..).for_each(|(_, v)| release(v));
                let _ = done.send(r);
                return;
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                // Idle: persist the oldest queued chunk, like `IOWorker.storePendingChunk` between tasks.
                let _ = storage.store_pending_chunk();
                if let Some((_, v)) = pending.shift_remove_index(0) {
                    release(v);
                }
            }
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                let _ = storage.close();
                return;
            }
        }
    }
}

/// Job queue shared by the parse/encode threads.
type PoolJob = Box<dyn FnOnce() -> Option<WorkerMessage> + Send>;

pub struct WorkerPool {
    tx: Option<Sender<PoolJob>>,
    threads: Vec<JoinHandle<()>>,
}

impl WorkerPool {
    pub fn new(name: &str, threads: usize, results: Sender<WorkerMessage>) -> Self {
        let (tx, rx) = channel::<PoolJob>();
        let rx = Arc::new(Mutex::new(rx));
        let threads = (0..threads.max(1))
            .map(|i| {
                let rx = rx.clone();
                let results = results.clone();
                std::thread::Builder::new()
                    .name(format!("{name}-{i}"))
                    .spawn(move || {
                        loop {
                            let job = rx.lock().unwrap_or_else(|e| e.into_inner()).recv();
                            match job {
                                Ok(job) => {
                                    if let Some(msg) = job() {
                                        if results.send(msg).is_err() {
                                            return;
                                        }
                                    }
                                }
                                Err(_) => return,
                            }
                        }
                    })
                    .expect("spawn worker")
            })
            .collect();
        Self { tx: Some(tx), threads }
    }

    pub fn submit(&self, job: impl FnOnce() -> Option<WorkerMessage> + Send + 'static) {
        if let Some(tx) = &self.tx {
            let _ = tx.send(Box::new(job));
        }
    }

    pub fn close(&mut self) {
        self.tx = None;
        for t in self.threads.drain(..) {
            let _ = t.join();
        }
    }
}

/// The serial worldgen thread (`ConsecutiveExecutor("worldgen")`): runs one [`GenBatch`] at a time.
pub struct WorldgenThread {
    tx: Option<Sender<(Arc<WorldGenContext>, GenBatch)>>,
    thread: Option<JoinHandle<()>>,
}

impl WorldgenThread {
    pub fn new(results: Sender<WorkerMessage>) -> Self {
        let (tx, rx) = channel::<(Arc<WorldGenContext>, GenBatch)>();
        let thread = std::thread::Builder::new()
            .name("Worldgen".into())
            .spawn(move || {
                while let Ok((ctx, batch)) = rx.recv() {
                    let result = run_batch(&ctx, batch);
                    if results.send(WorkerMessage::BatchDone(Box::new(result))).is_err() {
                        return;
                    }
                }
            })
            .expect("spawn worldgen thread");
        Self { tx: Some(tx), thread: Some(thread) }
    }

    pub fn submit(&self, ctx: Arc<WorldGenContext>, batch: GenBatch) {
        if let Some(tx) = &self.tx {
            let _ = tx.send((ctx, batch));
        }
    }

    pub fn close(&mut self) {
        self.tx = None;
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}
