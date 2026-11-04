//! `UserNameToIdResolver`, `CachedUserNameToIdResolver` (`usercache.json`) and `ProfileResolver`.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use chrono::{Local, Months};
use mc_protocol::codec::GameProfile;
use parking_lot::Mutex;
use uuid::Uuid;

use super::ban_list_entry::{Date, format_date, parse_date};
use super::gson::{self, JsonElement, JsonObject};
use super::name_and_id::NameAndId;

const GAMEPROFILES_MRU_LIMIT: usize = 1000;

/// `StringUtil.isValidPlayerName`.
pub fn is_valid_player_name(name: &str) -> bool {
    name.encode_utf16().count() <= 16 && name.encode_utf16().all(|c| c > 32 && c < 127)
}

/// The Mojang profile lookups vanilla does through authlib's `GameProfileRepository` / `SessionService`.
/// The blocking calls are made from the thread that asks (login, command) exactly like vanilla.
pub trait ProfileLookup: Send + Sync {
    /// `GameProfileRepository.findProfileByName`.
    fn find_profile_by_name(&self, name: &str) -> Option<NameAndId>;
    /// `SessionService.fetchProfile(id, true)` (the full profile with textures).
    fn fetch_profile(&self, id: Uuid) -> Option<GameProfile>;
}

/// A lookup that never finds anything (offline machines, tests).
pub struct NoProfileLookup;

impl ProfileLookup for NoProfileLookup {
    fn find_profile_by_name(&self, _name: &str) -> Option<NameAndId> {
        None
    }

    fn fetch_profile(&self, _id: Uuid) -> Option<GameProfile> {
        None
    }
}

/// `UserNameToIdResolver`.
pub trait UserNameToIdResolver: Send + Sync {
    fn add(&self, name_and_id: NameAndId);
    fn get_by_name(&self, name: &str) -> Option<NameAndId>;
    fn get_by_id(&self, id: Uuid) -> Option<NameAndId>;
    fn resolve_offline_users(&self, value: bool);
    fn save(&self);
}

struct GameProfileInfo {
    name_and_id: NameAndId,
    expiration_date: Date,
    last_access: AtomicU64,
}

#[derive(Default)]
struct State {
    resolve_offline_users: bool,
    by_name: HashMap<String, Arc<GameProfileInfo>>,
    by_uuid: HashMap<Uuid, Arc<GameProfileInfo>>,
}

/// `CachedUserNameToIdResolver`.
pub struct CachedUserNameToIdResolver {
    state: Mutex<State>,
    profile_repository: Arc<dyn ProfileLookup>,
    file: PathBuf,
    operation_count: AtomicU64,
}

/// Java `String.toLowerCase(Locale.ROOT)` over the BMP/SMP: Rust's full Unicode lowercase differs only for a
/// handful of special cases (final sigma, dotted I), which cannot appear in valid player names.
fn lower(s: &str) -> String {
    s.to_lowercase()
}

impl CachedUserNameToIdResolver {
    pub fn new(profile_repository: Arc<dyn ProfileLookup>, file: impl Into<PathBuf>) -> Self {
        let resolver = CachedUserNameToIdResolver {
            state: Mutex::new(State { resolve_offline_users: true, ..Default::default() }),
            profile_repository,
            file: file.into(),
            operation_count: AtomicU64::new(0),
        };
        for info in resolver.load().into_iter().rev() {
            resolver.safe_add(info);
        }
        resolver
    }

    fn next_operation(&self) -> u64 {
        self.operation_count.fetch_add(1, Ordering::SeqCst) + 1
    }

    fn safe_add(&self, info: GameProfileInfo) -> Arc<GameProfileInfo> {
        info.last_access.store(self.next_operation(), Ordering::SeqCst);
        let info = Arc::new(info);
        let mut state = self.state.lock();
        state.by_name.insert(lower(&info.name_and_id.name), info.clone());
        state.by_uuid.insert(info.name_and_id.id, info.clone());
        info
    }

    fn lookup_game_profile(&self, name: &str) -> Option<NameAndId> {
        if !is_valid_player_name(name) {
            return self.create_unknown_profile(name);
        }
        self.profile_repository.find_profile_by_name(name).or_else(|| self.create_unknown_profile(name))
    }

    fn create_unknown_profile(&self, name: &str) -> Option<NameAndId> {
        self.state.lock().resolve_offline_users.then(|| NameAndId::create_offline(name))
    }

    fn add_internal(&self, profile: NameAndId) -> Arc<GameProfileInfo> {
        let expiration = Local::now().checked_add_months(Months::new(1)).unwrap_or_else(Local::now).to_utc();
        let info = self.safe_add(GameProfileInfo { name_and_id: profile, expiration_date: expiration, last_access: AtomicU64::new(0) });
        self.save();
        info
    }

    fn load(&self) -> Vec<GameProfileInfo> {
        let text = match std::fs::read(&self.file) {
            Ok(bytes) => String::from_utf8_lossy(&bytes).into_owned(),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Vec::new(),
            Err(e) => {
                log::warn!("Failed to load profile cache {}\n{e}", self.file.display());
                return Vec::new();
            }
        };
        let elements = match gson::parse(&text, true) {
            Ok(Some(JsonElement::Array(a))) => a,
            Ok(_) => return Vec::new(),
            Err(e) => {
                log::warn!("Failed to load profile cache {}\n{e}", self.file.display());
                return Vec::new();
            }
        };
        elements.iter().filter_map(read_game_profile).collect()
    }

    /// `getTopMRUProfiles(1000)` in file order.
    fn top_mru(&self) -> Vec<Arc<GameProfileInfo>> {
        let mut all: Vec<_> = self.state.lock().by_uuid.values().cloned().collect();
        all.sort_by_key(|i| std::cmp::Reverse(i.last_access.load(Ordering::SeqCst)));
        all.truncate(GAMEPROFILES_MRU_LIMIT);
        all
    }
}

fn read_game_profile(json: &JsonElement) -> Option<GameProfileInfo> {
    let object = json.as_object()?;
    let name_and_id = NameAndId::from_json(object).ok()??;
    let date_string = object.get("expiresOn")?.as_string().ok()?;
    match parse_date(&date_string) {
        Some(expiration_date) => Some(GameProfileInfo { name_and_id, expiration_date, last_access: AtomicU64::new(0) }),
        None => {
            log::warn!("Failed to parse date {date_string}");
            None
        }
    }
}

impl UserNameToIdResolver for CachedUserNameToIdResolver {
    fn add(&self, name_and_id: NameAndId) {
        self.add_internal(name_and_id);
    }

    fn get_by_name(&self, name: &str) -> Option<NameAndId> {
        let user_name = lower(name);
        let mut needs_save = false;
        let mut cached = self.state.lock().by_name.get(&user_name).cloned();
        if let Some(info) = &cached {
            if Date::from(std::time::SystemTime::now()) >= info.expiration_date {
                let mut state = self.state.lock();
                state.by_uuid.remove(&info.name_and_id.id);
                state.by_name.remove(&lower(&info.name_and_id.name));
                needs_save = true;
                cached = None;
            }
        }
        let result = match cached {
            Some(info) => {
                info.last_access.store(self.next_operation(), Ordering::SeqCst);
                Some(info.name_and_id.clone())
            }
            None => match self.lookup_game_profile(&user_name) {
                Some(profile) => {
                    needs_save = false;
                    Some(self.add_internal(profile).name_and_id.clone())
                }
                None => None,
            },
        };
        if needs_save {
            self.save();
        }
        result
    }

    fn get_by_id(&self, id: Uuid) -> Option<NameAndId> {
        let info = self.state.lock().by_uuid.get(&id).cloned()?;
        info.last_access.store(self.next_operation(), Ordering::SeqCst);
        Some(info.name_and_id.clone())
    }

    fn resolve_offline_users(&self, value: bool) {
        self.state.lock().resolve_offline_users = value;
    }

    fn save(&self) {
        let list = JsonElement::Array(
            self.top_mru()
                .iter()
                .map(|entry| {
                    let mut object = JsonObject::new();
                    entry.name_and_id.append_to(&mut object);
                    object.add_property_str("expiresOn", Some(&format_date(&entry.expiration_date)));
                    JsonElement::Object(object)
                })
                .collect(),
        );
        // vanilla swallows IOExceptions here
        let _ = std::fs::write(&self.file, list.to_gson_string());
    }
}

/// `ProfileResolver`.
pub trait ProfileResolver: Send + Sync {
    fn fetch_by_name(&self, name: &str) -> Option<GameProfile>;
    fn fetch_by_id(&self, id: Uuid) -> Option<GameProfile>;
}

struct CacheEntry<T> {
    value: Option<T>,
    last_access: Instant,
}

/// `ProfileResolver.Cached`: two caches (by name, by id), 10 minutes after last access, at most 256 entries each.
pub struct CachedProfileResolver {
    lookup: Arc<dyn ProfileLookup>,
    name_to_id: Arc<dyn UserNameToIdResolver>,
    by_name: Mutex<HashMap<String, CacheEntry<GameProfile>>>,
    by_id: Mutex<HashMap<Uuid, CacheEntry<GameProfile>>>,
}

const PROFILE_CACHE_EXPIRY: Duration = Duration::from_secs(600);
const PROFILE_CACHE_MAX: usize = 256;

fn cached_get<K: Clone + Eq + std::hash::Hash, T: Clone>(
    cache: &Mutex<HashMap<K, CacheEntry<T>>>,
    key: &K,
    load: impl FnOnce() -> Option<T>,
) -> Option<T> {
    let now = Instant::now();
    {
        let mut cache = cache.lock();
        cache.retain(|_, e| now.duration_since(e.last_access) < PROFILE_CACHE_EXPIRY);
        if let Some(entry) = cache.get_mut(key) {
            entry.last_access = now;
            return entry.value.clone();
        }
    }
    let value = load();
    let mut cache = cache.lock();
    if cache.len() >= PROFILE_CACHE_MAX {
        if let Some(oldest) = cache.iter().min_by_key(|(_, e)| e.last_access).map(|(k, _)| k.clone()) {
            cache.remove(&oldest);
        }
    }
    cache.insert(key.clone(), CacheEntry { value: value.clone(), last_access: now });
    value
}

impl CachedProfileResolver {
    pub fn new(lookup: Arc<dyn ProfileLookup>, name_to_id: Arc<dyn UserNameToIdResolver>) -> Self {
        CachedProfileResolver { lookup, name_to_id, by_name: Mutex::default(), by_id: Mutex::default() }
    }
}

impl ProfileResolver for CachedProfileResolver {
    fn fetch_by_name(&self, name: &str) -> Option<GameProfile> {
        if !is_valid_player_name(name) {
            return None;
        }
        cached_get(&self.by_name, &name.to_owned(), || {
            let name_and_id = self.name_to_id.get_by_name(name)?;
            self.fetch_by_id(name_and_id.id)
        })
    }

    fn fetch_by_id(&self, id: Uuid) -> Option<GameProfile> {
        cached_get(&self.by_id, &id, || self.lookup.fetch_profile(id))
    }
}
