//! Worker threads of the chunk map: the region file IO thread (vanilla's `IOWorker`), a small pool for chunk NBT
//! parsing/encoding, and the worldgen thread (vanilla's serial `worldgen` `ConsecutiveExecutor`). Results come back
//! to the game thread through one channel of [`WorkerMessage`]s which the chunk map polls (`ServerChunkCache.pollTask`).
//! Every message carries the sequence number the game thread gave its job when it submitted it; the chunk map applies
//! them in that order, so what a worker thread's speed decides is only how long a result waits, never what the game
//! thread does next.

use super::chunk_io_reporter::ChunkIoReporter;
use super::chunk_status_tasks::{BatchResult, GenBatch, run_batch};
use super::world_gen::WorldGenContext;
use crate::crash_report::CrashReport;
use crate::world::level::levelgen::structure::check::ChunkScanAccess;
use mc_core::pos::ChunkPos;
use mc_nbt::CompoundTag;
use mc_world::region::old_chunks::{OldChunkBits, OldChunkRegions};
use mc_world::region::{RegionStorageInfo, SimpleRegionStorage};
use mc_world::serial::{ChunkReadException, SerializableChunkData};
use std::io;
use std::path::PathBuf;
use std::sync::atomic::{AtomicI32, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, LazyLock, Mutex, OnceLock};
use std::thread::JoinHandle;
use std::time::Duration;

/// What a worker thread hands back to the game thread.
pub enum WorkerMessage {
    ChunkRead {
        key: i64,
        result: io::Result<Option<CompoundTag>>,
    },
    ChunkParsed {
        key: i64,
        result: Box<Result<Option<SerializableChunkData>, ChunkReadException>>,
    },
    BatchDone(Box<BatchResult>),
    /// A generation step panicked: the game thread throws the report (`BlockableEventLoop.relayDelayCrash`).
    BatchFailed(Box<CrashReport>),
}

/// The sending half of the worker-to-game-thread channel. Every send wakes the game thread
/// (`BlockableEventLoop.schedule`'s `LockSupport.unpark`): without it a result would wait until the game thread
/// finishes sleeping toward the next tick, which turns every worldgen step into a tick of latency.
#[derive(Clone)]
pub struct ResultSender {
    tx: Sender<(u64, WorkerMessage)>,
    game_thread: std::thread::Thread,
}

impl ResultSender {
    /// A channel whose results wake the calling thread (the game thread, which builds the chunk map).
    pub fn channel() -> (Self, Receiver<(u64, WorkerMessage)>) {
        let (tx, rx) = channel();
        let game_thread = std::thread::current();
        (Self { tx, game_thread }, rx)
    }

    pub fn send(
        &self,
        seq: u64,
        message: WorkerMessage,
    ) -> Result<(), std::sync::mpsc::SendError<(u64, WorkerMessage)>> {
        let sent = self.tx.send((seq, message));
        self.game_thread.unpark();
        sent
    }
}

enum IoRequest {
    Read {
        key: i64,
        seq: u64,
    },
    ReadBlocking {
        key: i64,
        reply: Sender<io::Result<Option<CompoundTag>>>,
    },
    Scan {
        key: i64,
        reply: Sender<io::Result<Option<CompoundTag>>>,
    },
    OldRegion {
        region_x: i32,
        region_z: i32,
        reply: Sender<OldChunkBits>,
    },
    Write {
        pos: ChunkPos,
        tag: Option<CompoundTag>,
        active_writes: Option<Arc<AtomicI32>>,
    },
    Synchronize {
        flush: bool,
        done: Sender<io::Result<()>>,
    },
    Close {
        done: Sender<io::Result<()>>,
    },
}

/// `IOWorker` over a [`SimpleRegionStorage`] on its own thread.
pub struct RegionIo {
    tx: Sender<IoRequest>,
    thread: Option<JoinHandle<()>>,
    /// `IOWorker.regionCacheForBlender`.
    old_regions: OldChunkRegions,
}

impl RegionIo {
    pub fn new(
        info: RegionStorageInfo,
        folder: PathBuf,
        sync_writes: bool,
        results: ResultSender,
        reporter: Arc<ChunkIoReporter>,
    ) -> Self {
        let (tx, rx) = channel::<IoRequest>();
        let mut storage = SimpleRegionStorage::new(info.clone(), folder, sync_writes);
        let old_regions = storage.old_chunk_regions();
        let thread = std::thread::Builder::new()
            .name(format!("IO-Worker-{}", info.dimension))
            .spawn(move || {
                // `IOWorker.runStore` completes the write exceptionally; `ChunkMap.save` reports that as a save failure
                storage.set_store_failure_handler(Box::new(move |pos, error| {
                    reporter.report_chunk_save_failure(&error.to_string(), &info, pos)
                }));
                io_loop(storage, rx, results)
            })
            .expect("spawn IO worker");
        Self {
            tx,
            thread: Some(thread),
            old_regions,
        }
    }

    /// `read(pos)`: the result arrives as [`WorkerMessage::ChunkRead`] with sequence number `seq`.
    pub fn read(&self, pos: ChunkPos, seq: u64) {
        let _ = self.tx.send(IoRequest::Read {
            key: pos.pack(),
            seq,
        });
    }

    /// `readChunk(pos).join()`.
    pub fn read_blocking(&self, pos: ChunkPos) -> io::Result<Option<CompoundTag>> {
        let (reply, rx) = channel();
        let _ = self.tx.send(IoRequest::ReadBlocking {
            key: pos.pack(),
            reply,
        });
        rx.recv()
            .unwrap_or_else(|_| Err(io::Error::other("IO worker is gone")))
    }

    /// `IOWorker` as the `ChunkScanAccess` of a level's structure check.
    pub fn scan_access(&self, encoding_chunks: Arc<AtomicI32>) -> RegionIoScan {
        RegionIoScan {
            tx: self.tx.clone(),
            old_regions: self.old_regions.clone(),
            encoding_chunks,
        }
    }

    /// A clonable write-only handle for worker threads.
    pub fn handle(&self) -> RegionIoHandle {
        RegionIoHandle {
            tx: self.tx.clone(),
        }
    }

    /// `write(pos, tag)`; `active_writes` is decremented once the chunk reached the region file.
    pub fn write(
        &self,
        pos: ChunkPos,
        tag: Option<CompoundTag>,
        active_writes: Option<Arc<AtomicI32>>,
    ) {
        let _ = self.tx.send(IoRequest::Write {
            pos,
            tag,
            active_writes,
        });
    }

    /// `synchronize(flush).join()`.
    pub fn synchronize(&self, flush: bool) -> io::Result<()> {
        let (done, rx) = channel();
        let _ = self.tx.send(IoRequest::Synchronize { flush, done });
        rx.recv()
            .unwrap_or_else(|_| Err(io::Error::other("IO worker is gone")))
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

/// `IOWorker.scanChunk` for any thread: asks the IO thread, which answers from its queued writes before the region file.
#[derive(Clone)]
pub struct RegionIoScan {
    tx: Sender<IoRequest>,
    old_regions: OldChunkRegions,
    /// Saves still being encoded on the pool: vanilla queues a save's write at once, here it reaches the IO thread when
    /// the encoding is done.
    encoding_chunks: Arc<AtomicI32>,
}

impl RegionIoScan {
    /// `IOWorker.isOldChunkAround(pos, range)`: answered from the cache of scanned regions; a region that is not in it is
    /// scanned by the IO thread first.
    pub fn is_old_chunk_around(&self, pos: ChunkPos, range: i32) -> bool {
        self.old_regions
            .is_old_chunk_around(pos, range, |region_x, region_z| {
                self.wait_for_encodes();
                let (reply, rx) = channel();
                let _ = self.tx.send(IoRequest::OldRegion {
                    region_x,
                    region_z,
                    reply,
                });
                rx.recv().unwrap_or_default()
            })
    }

    fn wait_for_encodes(&self) {
        while self.encoding_chunks.load(Ordering::Acquire) > 0 {
            std::thread::sleep(Duration::from_millis(1));
        }
    }
}

impl ChunkScanAccess for RegionIoScan {
    fn scan_chunk(&self, pos: ChunkPos) -> io::Result<Option<CompoundTag>> {
        self.wait_for_encodes();
        let (reply, rx) = channel();
        let _ = self.tx.send(IoRequest::Scan {
            key: pos.pack(),
            reply,
        });
        rx.recv()
            .unwrap_or_else(|_| Err(io::Error::other("IO worker is gone")))
    }
}

/// The scan access of a level whose IO worker does not exist yet: the structure check is built with the level's
/// generation environment, the chunk map (and its IO worker) after it.
#[derive(Clone, Default)]
pub struct DeferredChunkScan(Arc<OnceLock<RegionIoScan>>);

impl DeferredChunkScan {
    /// Connects the scan to the IO worker of the level's chunk map.
    pub fn attach(&self, scan: RegionIoScan) {
        let _ = self.0.set(scan);
    }
}

impl DeferredChunkScan {
    /// `IOWorker.isOldChunkAround`; a level without an IO worker has no old chunks.
    pub fn is_old_chunk_around(&self, pos: ChunkPos, range: i32) -> bool {
        self.0
            .get()
            .is_some_and(|scan| scan.is_old_chunk_around(pos, range))
    }
}

impl ChunkScanAccess for DeferredChunkScan {
    fn scan_chunk(&self, pos: ChunkPos) -> io::Result<Option<CompoundTag>> {
        match self.0.get() {
            Some(scan) => scan.scan_chunk(pos),
            None => Err(io::Error::other("the level's IO worker does not exist yet")),
        }
    }
}

/// Write side of a [`RegionIo`] usable from pool threads.
#[derive(Clone)]
pub struct RegionIoHandle {
    tx: Sender<IoRequest>,
}

impl RegionIoHandle {
    pub fn write(
        &self,
        pos: ChunkPos,
        tag: Option<CompoundTag>,
        active_writes: Option<Arc<AtomicI32>>,
    ) {
        let _ = self.tx.send(IoRequest::Write {
            pos,
            tag,
            active_writes,
        });
    }
}

fn io_loop(mut storage: SimpleRegionStorage, rx: Receiver<IoRequest>, results: ResultSender) {
    // Mirror of the storage's pending-write order (same insertion semantics): the counters to release per chunk.
    let mut pending: indexmap::IndexMap<i64, Vec<Arc<AtomicI32>>> = indexmap::IndexMap::new();
    fn release(counters: Vec<Arc<AtomicI32>>) {
        for c in counters {
            c.fetch_sub(1, Ordering::AcqRel);
        }
    }
    loop {
        match rx.recv_timeout(Duration::from_millis(5)) {
            Ok(IoRequest::Read { key, seq }) => {
                // a panic of the read must still answer: the chunk map applies results in submission order, a missing one
                // would hold every later result back
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    storage.read(&ChunkPos::unpack(key))
                }))
                .unwrap_or_else(|payload| {
                    Err(io::Error::other(crate::crash_report::exception_text(
                        payload.as_ref(),
                    )))
                });
                if results
                    .send(seq, WorkerMessage::ChunkRead { key, result })
                    .is_err()
                {
                    return;
                }
            }
            Ok(IoRequest::ReadBlocking { key, reply }) => {
                let _ = reply.send(storage.read(&ChunkPos::unpack(key)));
            }
            Ok(IoRequest::Scan { key, reply }) => {
                let _ = reply.send(storage.scan_chunk(&ChunkPos::unpack(key)));
            }
            Ok(IoRequest::OldRegion {
                region_x,
                region_z,
                reply,
            }) => {
                let _ = reply.send(storage.scan_old_region(region_x, region_z));
            }
            Ok(IoRequest::Write {
                pos,
                tag,
                active_writes,
            }) => {
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
type PoolJob = Box<dyn FnOnce(&ResultSender) + Send>;

pub struct WorkerPool {
    tx: Option<Sender<PoolJob>>,
    threads: Vec<JoinHandle<()>>,
}

impl WorkerPool {
    pub fn new(name: &str, threads: usize, results: ResultSender) -> Self {
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
                                Ok(job) => job(&results),
                                Err(_) => return,
                            }
                        }
                    })
                    .expect("spawn worker")
            })
            .collect();
        Self {
            tx: Some(tx),
            threads,
        }
    }

    /// A job nobody waits for (chunk encoding).
    pub fn submit(&self, job: impl FnOnce() + Send + 'static) {
        if let Some(tx) = &self.tx {
            let _ = tx.send(Box::new(move |_| job()));
        }
    }

    /// A job whose message is applied by the game thread as number `seq` of the submitted jobs. A panic of the job is
    /// answered with `on_panic(text)` instead, so the sequence never has a gap that would hold every later result back
    /// (vanilla: the failed future completes exceptionally and its `exceptionally` stage runs).
    pub fn submit_sequenced(
        &self,
        seq: u64,
        job: impl FnOnce() -> WorkerMessage + Send + 'static,
        on_panic: impl FnOnce(String) -> WorkerMessage + Send + 'static,
    ) {
        if let Some(tx) = &self.tx {
            let _ = tx.send(Box::new(move |results| {
                let message = std::panic::catch_unwind(std::panic::AssertUnwindSafe(job))
                    .unwrap_or_else(|payload| {
                        on_panic(crate::crash_report::exception_text(payload.as_ref()))
                    });
                let _ = results.send(seq, message);
            }));
        }
    }

    pub fn close(&mut self) {
        self.tx = None;
        for t in self.threads.drain(..) {
            let _ = t.join();
        }
    }
}

/// `Util.backgroundExecutor()`: the process-wide pool (`cores - 1` threads) the biome and terrain steps run on, shared by
/// every chunk map. Jobs carry their own [`ResultSender`].
struct BackgroundExecutor {
    tx: Sender<Box<dyn FnOnce() + Send>>,
}

static BACKGROUND: LazyLock<BackgroundExecutor> = LazyLock::new(|| {
    let (tx, rx) = channel::<Box<dyn FnOnce() + Send>>();
    let rx = Arc::new(Mutex::new(rx));
    let threads =
        std::thread::available_parallelism().map_or(1, |n| n.get().saturating_sub(1).max(1));
    for i in 0..threads {
        let rx = rx.clone();
        std::thread::Builder::new()
            .name(format!("Worker-Main-{i}"))
            .spawn(move || {
                loop {
                    // the guard is a temporary: other workers receive while the job runs
                    let job = rx.lock().unwrap_or_else(|e| e.into_inner()).recv();
                    match job {
                        Ok(job) => job(),
                        Err(_) => return,
                    }
                }
            })
            .expect("spawn background worker");
    }
    BackgroundExecutor { tx }
});

/// Runs a batch; a panic outside the per-step guard of `run_batch` is a crash report like a failed step.
fn run_batch_message(ctx: &WorldGenContext, batch: GenBatch) -> WorkerMessage {
    batch_message(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| run_batch(ctx, batch)))
            .unwrap_or_else(|payload| {
                Err(CrashReport::for_throwable(
                    payload,
                    "Exception generating new chunk",
                ))
            }),
    )
}

fn batch_message(result: Result<BatchResult, CrashReport>) -> WorkerMessage {
    match result {
        Ok(result) => WorkerMessage::BatchDone(Box::new(result)),
        Err(report) => WorkerMessage::BatchFailed(Box::new(report)),
    }
}

/// The worldgen executor: the serial `worldgen` thread (`ConsecutiveExecutor("worldgen")`) runs one [`GenBatch`] at a
/// time; the steps vanilla runs on `Util.backgroundExecutor()` (biomes, terrain) go to the shared pool.
pub struct WorldgenThread {
    tx: Option<Sender<(u64, Arc<WorldGenContext>, GenBatch)>>,
    thread: Option<JoinHandle<()>>,
    results: ResultSender,
    /// Oracle mode: the batch runs on the submitting thread (the vanilla oracle's direct executors), so the unique
    /// seeds it draws continue the calling thread's reproducible sequence, and [`WorldgenThread::submit`] hands the result back.
    inline: bool,
}

impl WorldgenThread {
    pub fn new(results: ResultSender) -> Self {
        if mc_core::random::oracle_uniquifier_active() {
            return Self {
                tx: None,
                thread: None,
                results,
                inline: true,
            };
        }
        let (tx, rx) = channel::<(u64, Arc<WorldGenContext>, GenBatch)>();
        let thread_results = results.clone();
        let thread = std::thread::Builder::new()
            .name("Worldgen".into())
            .spawn(move || {
                while let Ok((seq, ctx, batch)) = rx.recv() {
                    if thread_results
                        .send(seq, run_batch_message(&ctx, batch))
                        .is_err()
                    {
                        return;
                    }
                }
            })
            .expect("spawn worldgen thread");
        Self {
            tx: Some(tx),
            thread: Some(thread),
            results,
            inline: false,
        }
    }

    /// Whether biome and terrain batches may overlap (not in oracle mode, where every step runs inline).
    pub fn runs_in_parallel(&self) -> bool {
        !self.inline
    }

    pub fn is_inline(&self) -> bool {
        self.inline
    }

    /// Queues the batch for the worldgen thread (its result is number `seq` of the submitted jobs); inline (oracle mode) it
    /// runs now and its result is returned for the caller to apply at once, as vanilla's direct executors complete every
    /// future of a step inside the call that scheduled it. An inline batch takes no sequence number.
    #[must_use]
    pub fn submit(
        &self,
        ctx: Arc<WorldGenContext>,
        batch: GenBatch,
        seq: u64,
    ) -> Option<WorkerMessage> {
        if self.inline {
            return Some(run_batch_message(&ctx, batch));
        }
        if let Some(tx) = &self.tx {
            let _ = tx.send((seq, ctx, batch));
        }
        None
    }

    /// `CompletableFuture.supplyAsync(..., Util.backgroundExecutor())`: the batch runs on the shared pool, concurrently
    /// with the worldgen thread and with other background batches.
    pub fn submit_background(&self, ctx: Arc<WorldGenContext>, batch: GenBatch, seq: u64) {
        let results = self.results.clone();
        let job = Box::new(move || {
            let _ = results.send(seq, run_batch_message(&ctx, batch));
        });
        let _ = BACKGROUND.tx.send(job);
    }

    pub fn close(&mut self) {
        self.tx = None;
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mc_core::pos::ChunkPos;
    use std::time::Instant;

    /// A result sent while the game thread sleeps toward the next tick wakes it at once.
    #[test]
    fn a_result_wakes_the_sleeping_game_thread() {
        let (results, rx) = ResultSender::channel();
        let sender = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(50));
            let _ = results.send(
                0,
                WorkerMessage::ChunkRead {
                    key: ChunkPos::new(0, 0).pack(),
                    result: Ok(None),
                },
            );
        });
        let start = Instant::now();
        while rx.try_recv().is_err() {
            std::thread::park_timeout(Duration::from_secs(10));
        }
        assert!(start.elapsed() < Duration::from_secs(5));
        sender.join().unwrap();
    }

    /// A job that panics still delivers a message in its place, and the thread runs the next job.
    #[test]
    fn a_panicking_sequenced_job_still_answers() {
        let (results, rx) = ResultSender::channel();
        let mut pool = WorkerPool::new("test-pool", 1, results);
        let parse_failed = |text: String| WorkerMessage::ChunkRead {
            key: 7,
            result: Err(io::Error::other(text)),
        };
        pool.submit_sequenced(0, || panic!("boom"), parse_failed);
        pool.submit_sequenced(
            1,
            || WorkerMessage::ChunkRead {
                key: 8,
                result: Ok(None),
            },
            parse_failed,
        );
        let first = rx.recv_timeout(Duration::from_secs(10)).expect("first");
        let second = rx.recv_timeout(Duration::from_secs(10)).expect("second");
        assert!(matches!(
            first,
            (0, WorkerMessage::ChunkRead { key: 7, result: Err(e) }) if e.to_string().contains("boom")
        ));
        assert!(matches!(
            second,
            (1, WorkerMessage::ChunkRead { key: 8, .. })
        ));
        pool.close();
    }

    /// `IOWorker.scanChunk`: a queued write answers first, then the region file; scanning never creates a region file.
    #[test]
    fn scan_answers_from_queued_writes_then_the_region_file() {
        let dir = tempfile::tempdir().unwrap();
        let (results, _rx) = ResultSender::channel();
        let info = RegionStorageInfo::new(
            "level",
            mc_core::ident::Identifier::parse("minecraft:overworld").unwrap(),
            "chunk",
        );
        let mut io = RegionIo::new(
            info,
            dir.path().join("region"),
            false,
            results,
            Arc::default(),
        );
        let scan = io.scan_access(Arc::default());
        let pos = ChunkPos::new(3, 3);
        assert!(scan.scan_chunk(pos).unwrap().is_none());
        assert!(!dir.path().join("region").join("r.0.0.mca").exists());
        let tag = CompoundTag::new().with("xPos", 3);
        io.write(pos, Some(tag.clone()), None);
        assert_eq!(scan.scan_chunk(pos).unwrap(), Some(tag.clone()));
        io.synchronize(true).unwrap();
        assert_eq!(scan.scan_chunk(pos).unwrap(), Some(tag));
        assert!(scan.scan_chunk(ChunkPos::new(4, 4)).unwrap().is_none());
        io.close().unwrap();
    }
}
