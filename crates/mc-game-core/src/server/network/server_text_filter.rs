//! `net.minecraft.server.network.ServerTextFilter`: the text filter that asks a moderation service over HTTP.
//! [`ServerTextFilter::create_from_config`] picks the implementation of `text-filtering-version`; the request flow
//! (worker pool, one ordered executor per player, request shapes, answers read through the `GsonHelper` accessors)
//! is shared by both.

use super::legacy_text_filter::LegacyTextFilter;
use super::player_safety_text_filter;
use super::text_filter::TextFilter;
use super::text_filter_json::as_object;
use crate::server::chat::FilteredText;
use crate::server::chat::filter_mask::FilterMaskExt;
use crate::server::dedicated::properties::DedicatedServerProperties;
use crate::util::completion::Completion;
use mc_core::gson::{JsonElement, JsonObject, parse_lenient_json_parser};
use mc_core::java_exception::JavaException;
use mc_protocol::codec::GameProfile;
use mc_protocol::packets::play::FilterMask;
use parking_lot::{Condvar, Mutex};
use std::collections::VecDeque;
use std::io::Read as _;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

/// `WORKER_COUNT`: the number in the next worker thread's name, over every filter of the process.
static WORKER_COUNT: AtomicUsize = AtomicUsize::new(1);

/// `HttpURLConnection`'s limit of followed redirects.
const MAX_REDIRECTS: usize = 20;
/// The `User-Agent` `HttpURLConnection` sends by itself (`Java/<java.version>`).
const JAVA_USER_AGENT: &str = "Java/27.0.1.1";

type Job = Box<dyn FnOnce() + Send>;

struct PoolState {
    threads: usize,
    queue: VecDeque<Job>,
    shutdown: bool,
}

/// `Executors.newFixedThreadPool(n, THREAD_FACTORY)`: a task starts a new thread (`Chat-Filter-Worker-N`) while
/// fewer than `n` exist, even when others are idle; after that tasks wait in a queue for a free worker.
pub struct WorkerPool {
    max: usize,
    state: Mutex<PoolState>,
    wake: Condvar,
}

impl WorkerPool {
    pub fn new(max: usize) -> Arc<Self> {
        Arc::new(Self {
            max,
            state: Mutex::new(PoolState {
                threads: 0,
                queue: VecDeque::new(),
                shutdown: false,
            }),
            wake: Condvar::new(),
        })
    }

    /// `execute(job)`; dropped once the pool is shut down (`RejectedExecutionException`).
    pub fn execute(self: &Arc<Self>, job: Job) {
        let mut state = self.state.lock();
        if state.shutdown {
            return;
        }
        if state.threads < self.max {
            state.threads += 1;
            let pool = self.clone();
            let name = format!(
                "Chat-Filter-Worker-{}",
                WORKER_COUNT.fetch_add(1, Ordering::SeqCst)
            );
            let spawned = std::thread::Builder::new()
                .name(name)
                .spawn(move || pool.work(job));
            if spawned.is_err() {
                state.threads -= 1;
            }
        } else {
            state.queue.push_back(job);
            self.wake.notify_one();
        }
    }

    fn work(&self, first: Job) {
        first();
        loop {
            let job = {
                let mut state = self.state.lock();
                loop {
                    if state.shutdown {
                        return;
                    }
                    if let Some(job) = state.queue.pop_front() {
                        break job;
                    }
                    self.wake.wait(&mut state);
                }
            };
            job();
        }
    }

    /// `shutdownNow()`: queued jobs are dropped, idle workers end.
    pub fn shutdown_now(&self) {
        let mut state = self.state.lock();
        state.shutdown = true;
        state.queue.clear();
        self.wake.notify_all();
    }
}

#[derive(PartialEq)]
enum Status {
    Sleeping,
    Running,
}

/// `ConsecutiveExecutor`: tasks run one after the other on the pool, one pool task per scheduled task.
pub struct ConsecutiveExecutor {
    pool: Arc<WorkerPool>,
    queue: Mutex<VecDeque<Job>>,
    status: Mutex<Status>,
}

impl ConsecutiveExecutor {
    pub fn new(pool: Arc<WorkerPool>) -> Arc<Self> {
        Arc::new(Self {
            pool,
            queue: Mutex::new(VecDeque::new()),
            status: Mutex::new(Status::Sleeping),
        })
    }

    pub fn schedule(self: &Arc<Self>, task: Job) {
        self.queue.lock().push_back(task);
        self.register_for_execution();
    }

    fn register_for_execution(self: &Arc<Self>) {
        if self.queue.lock().is_empty() {
            return;
        }
        let mut status = self.status.lock();
        if *status == Status::Sleeping {
            *status = Status::Running;
            drop(status);
            let this = self.clone();
            self.pool.execute(Box::new(move || this.run()));
        }
    }

    fn run(self: &Arc<Self>) {
        let task = self.queue.lock().pop_front();
        if let Some(task) = task {
            task();
        }
        *self.status.lock() = Status::Sleeping;
        self.register_for_execution();
    }
}

/// `ServerTextFilter.IgnoreStrategy.select(hashesToDrop)`.
#[derive(Clone, Copy)]
pub enum IgnoreStrategy {
    NeverIgnore,
    IgnoreFullyFiltered,
    OverThreshold(i32),
}

impl IgnoreStrategy {
    pub fn select(hashes_to_drop: i32) -> Self {
        match hashes_to_drop {
            -1 => Self::NeverIgnore,
            0 => Self::IgnoreFullyFiltered,
            threshold => Self::OverThreshold(threshold),
        }
    }

    fn should_ignore(self, message: &str, removed: usize) -> bool {
        match self {
            Self::NeverIgnore => false,
            Self::IgnoreFullyFiltered => message.encode_utf16().count() == removed,
            Self::OverThreshold(threshold) => removed as i64 >= i64::from(threshold),
        }
    }
}

/// `parseMask`.
pub fn parse_mask(
    message: &str,
    removed_chars: &[JsonElement],
    strategy: IgnoreStrategy,
) -> Result<FilterMask, JavaException> {
    if removed_chars.is_empty() {
        return Ok(FilterMask::PassThrough);
    }
    if strategy.should_ignore(message, removed_chars.len()) {
        return Ok(FilterMask::FullyFiltered);
    }
    let mut mask = FilterMask::partial();
    for element in removed_chars {
        let index = super::text_filter_json::get_as_int(element)?;
        // `BitSet.set(int)`
        let index = usize::try_from(index).map_err(|_| {
            JavaException::unchecked(
                "java.lang.IndexOutOfBoundsException",
                format!("bitIndex < 0: {index}"),
            )
        })?;
        mask.set_filtered(index);
    }
    Ok(mask)
}

/// The failure of one request, as the exception vanilla's `HttpURLConnection` flow throws.
pub type FilterError = JavaException;

/// What differs between the two filter versions.
pub trait FilterBackend: Send + Sync {
    /// `setAuthorizationProperty`: the `Authorization` header value.
    fn authorization(&self) -> Result<String, FilterError>;
    /// `filterText`.
    fn filter_text(
        &self,
        message: &str,
        strategy: IgnoreStrategy,
        result: &JsonObject,
    ) -> Result<FilteredText, FilterError>;
    /// `connectionReadTimeout`.
    fn read_timeout(&self) -> Duration {
        Duration::from_millis(2000)
    }
    /// The join and leave requests of a player (`LegacyTextFilter` only).
    fn join_leave(&self) -> Option<&JoinLeave> {
        None
    }
}

pub type ProfileEncoder = Box<dyn Fn(&GameProfile) -> JsonObject + Send + Sync>;

/// `LegacyTextFilter`'s join and leave endpoints with their body encoder.
pub struct JoinLeave {
    pub join: reqwest::Url,
    pub leave: reqwest::Url,
    pub encoder: ProfileEncoder,
}

pub type MessageEncoder = Box<dyn Fn(&GameProfile, &str) -> JsonObject + Send + Sync>;

pub struct ServerTextFilter {
    chat_endpoint: reqwest::Url,
    chat_encoder: MessageEncoder,
    chat_ignore_strategy: IgnoreStrategy,
    worker_pool: Arc<WorkerPool>,
    backend: Box<dyn FilterBackend>,
    http: reqwest::blocking::Client,
}

impl ServerTextFilter {
    pub fn new(
        chat_endpoint: reqwest::Url,
        chat_encoder: MessageEncoder,
        chat_ignore_strategy: IgnoreStrategy,
        worker_pool: Arc<WorkerPool>,
        backend: Box<dyn FilterBackend>,
    ) -> Self {
        // shortcut: the read timeout bounds the whole request (reqwest's blocking client has no per-read timeout), a
        // slow answer that keeps trickling in is cut where Java's would carry on; upgrade with a custom connector
        let http = reqwest::blocking::Client::builder()
            .connect_timeout(Duration::from_millis(15000))
            .timeout(backend.read_timeout())
            .redirect(reqwest::redirect::Policy::none())
            .no_proxy()
            .build()
            .expect("an HTTP client");
        Self {
            chat_endpoint,
            chat_encoder,
            chat_ignore_strategy,
            worker_pool,
            backend,
            http,
        }
    }

    /// `createWorkerPool`.
    pub fn create_worker_pool(max_concurrent_requests: i32) -> Arc<WorkerPool> {
        // `newFixedThreadPool(0)` throws: the config error surfaces like in vanilla
        WorkerPool::new(max_concurrent_requests.max(0) as usize)
    }

    /// `createFromConfig`: `Ok(None)` for a blank config, an unsupported version or a config that makes no filter;
    /// `Err` for a version 1 config that does not parse (nothing catches it in vanilla, the start fails).
    pub fn create_from_config(
        config: &DedicatedServerProperties,
    ) -> Result<Option<Arc<ServerTextFilter>>, FilterError> {
        let text = &config.text_filtering_config;
        if is_blank(text) {
            return Ok(None);
        }
        let filter = match config.text_filtering_version {
            0 => LegacyTextFilter::create_text_filter_from_config(text),
            1 => player_safety_text_filter::create_text_filter_from_config(text)?,
            _ => {
                log::warn!(
                    "Could not create text filter - unsupported text filtering version used"
                );
                None
            }
        };
        Ok(filter.map(Arc::new))
    }

    /// `close()`.
    pub fn close(&self) {
        self.worker_pool.shutdown_now();
    }

    /// `createContext(gameProfile)`.
    pub fn create_context(self: &Arc<Self>, profile: GameProfile) -> Arc<dyn TextFilter> {
        Arc::new(PlayerContext {
            filter: self.clone(),
            executor: ConsecutiveExecutor::new(self.worker_pool.clone()),
            profile,
        })
    }

    /// `makeRequest` up to the answer: a connection whose status is 2xx, as the response.
    fn make_request(
        &self,
        payload: &JsonObject,
        url: &reqwest::Url,
    ) -> Result<reqwest::blocking::Response, FilterError> {
        let authorization = self.backend.authorization()?;
        let mut response = self
            .http
            .post(url.clone())
            .header("Content-Type", "application/json; charset=utf-8")
            .header("Accept", "application/json")
            .header(
                "User-Agent",
                format!("Minecraft server{}", super::hooks::VERSION_NAME),
            )
            .header("Authorization", authorization)
            .body(JsonElement::Object(payload.clone()).to_compact_string())
            .send()
            .map_err(connection_error)?;
        // `HttpURLConnection` follows up to 20 redirects of 301, 302 and 303 with a fresh GET: none of the headers set
        // above are sent again, only the connection's own
        let mut location_base = url.clone();
        for _ in 0..MAX_REDIRECTS {
            if !matches!(response.status().as_u16(), 301..=303) {
                break;
            }
            let Some(next) = response
                .headers()
                .get(reqwest::header::LOCATION)
                .and_then(|l| l.to_str().ok())
                .and_then(|l| location_base.join(l).ok())
            else {
                break;
            };
            response = self
                .http
                .get(next.clone())
                .header("User-Agent", JAVA_USER_AGENT)
                .send()
                .map_err(connection_error)?;
            location_base = next;
        }
        let status = response.status();
        if status.is_success() {
            Ok(response)
        } else {
            Err(JavaException::unchecked(
                "net.minecraft.server.network.ServerTextFilter$RequestFailedException",
                format!(
                    "{} {}",
                    status.as_u16(),
                    status.canonical_reason().unwrap_or("null")
                ),
            ))
        }
    }

    fn process_request_response(
        &self,
        payload: &JsonObject,
        url: &reqwest::Url,
    ) -> Result<JsonObject, FilterError> {
        let mut response = self.make_request(payload, url)?;
        if response.status().as_u16() == 204 {
            return Ok(JsonObject::new());
        }
        let mut body = Vec::new();
        response.read_to_end(&mut body).map_err(read_error)?;
        let text = String::from_utf8_lossy(&body);
        let element = parse_lenient_json_parser(text.as_ref())
            .map_err(|e| super::text_filter_json::syntax(e.0))?;
        as_object(&element).cloned()
    }

    /// `processRequest` of the join and leave posts: the answer is read and dropped.
    pub(super) fn process_request(
        &self,
        payload: &JsonObject,
        url: &reqwest::Url,
    ) -> Result<(), FilterError> {
        let mut response = self.make_request(payload, url)?;
        let mut sink = Vec::new();
        response.read_to_end(&mut sink).map_err(read_error)?;
        Ok(())
    }

    /// `requestMessageProcessing`.
    fn request_message_processing(
        self: &Arc<Self>,
        sender: &GameProfile,
        message: &str,
        strategy: IgnoreStrategy,
        executor: &Arc<ConsecutiveExecutor>,
    ) -> Completion<FilteredText> {
        if message.is_empty() {
            return Completion::completed(FilteredText::empty());
        }
        let (completer, completion) = Completion::pending();
        let filter = self.clone();
        let sender = sender.clone();
        let message = message.to_owned();
        executor.schedule(Box::new(move || {
            let object = (filter.chat_encoder)(&sender, &message);
            let result = filter
                .process_request_response(&object, &filter.chat_endpoint)
                .and_then(|result| filter.backend.filter_text(&message, strategy, &result));
            completer.complete(match result {
                Ok(filtered) => filtered,
                Err(e) => {
                    log::warn!("Failed to validate message '{message}'\n{e}");
                    FilteredText::fully_filtered(message)
                }
            });
        }));
        completion
    }
}

/// `StringUtil.isBlank`.
fn is_blank(text: &str) -> bool {
    text.chars().all(|c| c.is_whitespace())
}

/// The exception `HttpURLConnection` throws for a failed `send`.
fn connection_error(e: reqwest::Error) -> FilterError {
    if e.is_timeout() {
        let message = if e.is_connect() {
            "Connect timed out"
        } else {
            "Read timed out"
        };
        return JavaException::new("java.net.SocketTimeoutException", message);
    }
    if e.is_connect() {
        return JavaException::new("java.net.ConnectException", "Connection refused");
    }
    JavaException::new("java.io.IOException", e.to_string())
}

fn read_error(e: std::io::Error) -> FilterError {
    if e.kind() == std::io::ErrorKind::TimedOut {
        JavaException::new("java.net.SocketTimeoutException", "Read timed out")
    } else {
        JavaException::new("java.io.IOException", e.to_string())
    }
}

/// `ServerTextFilter.PlayerContext`.
struct PlayerContext {
    filter: Arc<ServerTextFilter>,
    executor: Arc<ConsecutiveExecutor>,
    profile: GameProfile,
}

impl TextFilter for PlayerContext {
    fn join(&self) {
        self.join_or_leave(|j| &j.join);
    }

    fn leave(&self) {
        self.join_or_leave(|j| &j.leave);
    }

    fn process_stream_message(&self, message: &str) -> Completion<FilteredText> {
        self.filter.request_message_processing(
            &self.profile,
            message,
            self.filter.chat_ignore_strategy,
            &self.executor,
        )
    }

    fn process_message_bundle(&self, messages: &[String]) -> Completion<Vec<FilteredText>> {
        Completion::sequence(
            messages
                .iter()
                .map(|m| self.process_stream_message(m))
                .collect(),
        )
    }
}

impl PlayerContext {
    /// `processJoinOrLeave`; only the version 0 filter has endpoints for it.
    fn join_or_leave(&self, endpoint: fn(&JoinLeave) -> &reqwest::Url) {
        if self.filter.backend.join_leave().is_none() {
            return;
        }
        let filter = self.filter.clone();
        let profile = self.profile.clone();
        self.executor.schedule(Box::new(move || {
            let Some(join_leave) = filter.backend.join_leave() else {
                return;
            };
            let url = endpoint(join_leave);
            let object = (join_leave.encoder)(&profile);
            if let Err(e) = filter.process_request(&object, url) {
                log::warn!(
                    "Failed to send join/leave packet to {url} for player {}\n{e}",
                    profile_to_string(&profile)
                );
            }
        }));
    }
}

/// `GameProfile.toString()` (a record).
fn profile_to_string(profile: &GameProfile) -> String {
    format!(
        "GameProfile[id={}, name={}, properties={{}}]",
        profile.id, profile.name
    )
}
