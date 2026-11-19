//! `OldUsersConverter`: migration of the pre-1.7.6 text user lists (`banned-players.txt`, `banned-ips.txt`,
//! `ops.txt`, `white-list.txt`) and per-name player files to the UUID based JSON lists.

use std::fs;
use std::ops::ControlFlow;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use uuid::Uuid;

use super::ban_list_entry::{IpBanListEntry, UserBanListEntry, parse_date};
use super::java_hash_map::JavaHashMap;
use super::name_and_id::NameAndId;
use super::name_resolver::{ProfileLookup, ProfileLookupError, UserNameToIdResolver};
use super::user_lists::{
    IpBanList, ServerOpList, ServerOpListEntry, UserBanList, UserWhiteList, UserWhiteListEntry,
};
use crate::server::notifications::EmptyNotificationService;
use crate::server::permissions::LevelBasedPermissionSet;

pub const OLD_IPBANLIST: &str = "banned-ips.txt";
pub const OLD_USERBANLIST: &str = "banned-players.txt";
pub const OLD_OPLIST: &str = "ops.txt";
pub const OLD_WHITELIST: &str = "white-list.txt";

/// What the converter needs from the server.
pub struct ConversionContext {
    /// Directory holding the list files (the server's working directory).
    pub dir: PathBuf,
    /// `MinecraftServer.usesAuthentication`.
    pub uses_authentication: bool,
    /// `MinecraftServer.isSingleplayer`.
    pub is_singleplayer: bool,
    pub profile_lookup: Arc<dyn ProfileLookup>,
    pub name_to_id_cache: Arc<dyn UserNameToIdResolver>,
    /// `MinecraftServer.operatorUserPermissions()`.
    pub operator_permissions: LevelBasedPermissionSet,
}

impl ConversionContext {
    fn path(&self, name: &str) -> PathBuf {
        self.dir.join(name)
    }

    /// `lookupPlayers`: `on_result` is the `ProfileLookupCallback`, an `Err` from it is the callback throwing a
    /// `ConversionError` (the lookup stops there).
    fn lookup_players<'a>(
        &self,
        names: impl IntoIterator<Item = &'a str>,
        mut on_result: impl FnMut(&str, Result<Uuid, ProfileLookupError>) -> Result<(), ConversionError>,
    ) -> Result<(), ConversionError> {
        let names: Vec<String> = names
            .into_iter()
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
            .collect();
        if !self.uses_authentication {
            return names.iter().try_for_each(|name| {
                on_result(
                    name,
                    Ok(mc_core::uuid_util::create_offline_player_uuid(name)),
                )
            });
        }
        let mut thrown = Ok(());
        self.profile_lookup
            .find_profiles_by_names(&names, &mut |name, result| match on_result(name, result) {
                Ok(()) => ControlFlow::Continue(()),
                Err(e) => {
                    thrown = Err(e);
                    ControlFlow::Break(())
                }
            });
        thrown
    }
}

/// `OldUsersConverter.ConversionError`: stops the running conversion, which then reports `false`.
struct ConversionError(String);

impl ConversionError {
    fn log(&self) {
        log::error!(
            "Conversion failed, please try again later\nnet.minecraft.server.players.OldUsersConverter$ConversionError: {}",
            self.0
        );
    }
}

/// The `onProfileLookupFailed` of the list conversions: a name Mojang does not know is skipped, any other failure
/// aborts the conversion.
fn skip_unknown_name(
    what: &str,
    name: &str,
    error: ProfileLookupError,
) -> Result<(), ConversionError> {
    log::warn!("Could not lookup {what} for {name}\n{error}");
    match error {
        ProfileLookupError::NotFound => Ok(()),
        ProfileLookupError::Backend(_) => Err(backend_error(name)),
    }
}

fn backend_error(name: &str) -> ConversionError {
    ConversionError(format!(
        "Could not request user {name} from backend systems"
    ))
}

/// `String.split("\\|")`: trailing empty strings are dropped.
fn split_pipe(line: &str) -> Vec<String> {
    let mut parts: Vec<String> = line.split('|').map(str::to_owned).collect();
    while parts.len() > 1 && parts.last().is_some_and(String::is_empty) {
        parts.pop();
    }
    parts
}

/// `readOldListFormat`: fills `user_map` (lower-case first column -> columns) and returns the raw lines.
fn read_old_list_format(
    file: &Path,
    user_map: &mut JavaHashMap<Vec<String>>,
) -> std::io::Result<Vec<String>> {
    let lines = read_lines(file)?;
    for line in &lines {
        let line = line.trim();
        if !line.starts_with('#') && !line.is_empty() {
            let parts = split_pipe(line);
            user_map.insert(parts[0].to_lowercase(), parts);
        }
    }
    Ok(lines)
}

/// Guava `Files.readLines(file, UTF_8)`.
fn read_lines(file: &Path) -> std::io::Result<Vec<String>> {
    Ok(String::from_utf8_lossy(&fs::read(file)?)
        .lines()
        .map(str::to_owned)
        .collect())
}

fn is_file(path: &Path) -> bool {
    path.is_file()
}

fn rename_old_file(path: &Path) {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let _ = fs::rename(path, path.with_file_name(format!("{name}.converted")));
}

fn old_columns(
    def: &[String],
) -> (
    Option<super::ban_list_entry::Date>,
    Option<&str>,
    Option<super::ban_list_entry::Date>,
    Option<&str>,
) {
    let created = def.get(1).and_then(|d| parse_date(d));
    let source = def.get(2).map(String::as_str);
    let expires = def.get(3).and_then(|d| parse_date(d));
    let reason = def.get(4).map(String::as_str);
    (created, source, expires, reason)
}

/// `convertUserBanlist`.
pub fn convert_user_banlist(ctx: &ConversionContext) -> bool {
    let old = ctx.path(OLD_USERBANLIST);
    if !is_file(&old) {
        return true;
    }
    let mut bans = UserBanList::new(
        ctx.path("banned-players.json"),
        Arc::new(EmptyNotificationService),
    );
    if bans.get_file().exists()
        && let Err(e) = bans.load()
    {
        log::warn!("Could not load existing file banned-players.json\n{e}");
    }
    let mut user_map = JavaHashMap::new();
    if let Err(e) = read_old_list_format(&old, &mut user_map) {
        log::warn!("Could not read old user banlist to convert it!\n{e}");
        return false;
    }
    let names: Vec<String> = user_map.keys().map(str::to_owned).collect();
    let looked_up = ctx.lookup_players(names.iter().map(String::as_str), |name, result| {
        let Ok(id) = result else {
            return skip_unknown_name("user banlist entry", name, result.unwrap_err());
        };
        let profile = NameAndId::new(id, name);
        ctx.name_to_id_cache.add(profile.clone());
        let Some(def) = user_map.get(&profile.name.to_lowercase()) else {
            log::warn!("Could not convert user banlist entry for {}", profile.name);
            return Err(ConversionError(
                "Profile not in the conversionlist".to_owned(),
            ));
        };
        let (created, source, expires, reason) = old_columns(def);
        bans.add(UserBanListEntry::new(
            Some(profile),
            created,
            source,
            expires,
            reason,
        ));
        Ok(())
    });
    if let Err(e) = looked_up {
        e.log();
        return false;
    }
    if let Err(e) = bans.save() {
        log::warn!("Could not read old user banlist to convert it!\n{e}");
        return false;
    }
    rename_old_file(&old);
    true
}

/// `convertIpBanlist`.
pub fn convert_ip_banlist(ctx: &ConversionContext) -> bool {
    let old = ctx.path(OLD_IPBANLIST);
    if !is_file(&old) {
        return true;
    }
    let mut ip_bans = IpBanList::new(
        ctx.path("banned-ips.json"),
        Arc::new(EmptyNotificationService),
    );
    if ip_bans.get_file().exists()
        && let Err(e) = ip_bans.load()
    {
        log::warn!("Could not load existing file banned-ips.json\n{e}");
    }
    let mut user_map = JavaHashMap::new();
    if let Err(e) = read_old_list_format(&old, &mut user_map) {
        log::warn!("Could not parse old ip banlist to convert it!\n{e}");
        return false;
    }
    for (key, def) in user_map.iter() {
        let (created, source, expires, reason) = old_columns(def);
        ip_bans.add(IpBanListEntry::new(
            Some(key.to_owned()),
            created,
            source,
            expires,
            reason,
        ));
    }
    if let Err(e) = ip_bans.save() {
        log::warn!("Could not parse old ip banlist to convert it!\n{e}");
        return false;
    }
    rename_old_file(&old);
    true
}

/// `convertOpsList`.
pub fn convert_ops_list(ctx: &ConversionContext) -> bool {
    let old = ctx.path(OLD_OPLIST);
    if !is_file(&old) {
        return true;
    }
    let mut ops = ServerOpList::new(ctx.path("ops.json"), Arc::new(EmptyNotificationService));
    if ops.get_file().exists()
        && let Err(e) = ops.load()
    {
        log::warn!("Could not load existing file ops.json\n{e}");
    }
    let lines = match read_lines(&old) {
        Ok(l) => l,
        Err(e) => {
            log::warn!("Could not read old oplist to convert it!\n{e}");
            return false;
        }
    };
    let looked_up = ctx.lookup_players(lines.iter().map(String::as_str), |name, result| {
        let Ok(id) = result else {
            return skip_unknown_name("oplist entry", name, result.unwrap_err());
        };
        let profile = NameAndId::new(id, name);
        ctx.name_to_id_cache.add(profile.clone());
        ops.add(ServerOpListEntry::new(
            profile,
            ctx.operator_permissions,
            false,
        ));
        Ok(())
    });
    if let Err(e) = looked_up {
        e.log();
        return false;
    }
    if let Err(e) = ops.save() {
        log::warn!("Could not read old oplist to convert it!\n{e}");
        return false;
    }
    rename_old_file(&old);
    true
}

/// `convertWhiteList`.
pub fn convert_white_list(ctx: &ConversionContext) -> bool {
    let old = ctx.path(OLD_WHITELIST);
    if !is_file(&old) {
        return true;
    }
    let mut whitelist = UserWhiteList::new(
        ctx.path("whitelist.json"),
        Arc::new(EmptyNotificationService),
    );
    if whitelist.get_file().exists()
        && let Err(e) = whitelist.load()
    {
        log::warn!("Could not load existing file whitelist.json\n{e}");
    }
    let lines = match read_lines(&old) {
        Ok(l) => l,
        Err(e) => {
            log::warn!("Could not read old whitelist to convert it!\n{e}");
            return false;
        }
    };
    let looked_up = ctx.lookup_players(lines.iter().map(String::as_str), |name, result| {
        let Ok(id) = result else {
            return skip_unknown_name("user whitelist entry", name, result.unwrap_err());
        };
        let profile = NameAndId::new(id, name);
        ctx.name_to_id_cache.add(profile.clone());
        whitelist.add(UserWhiteListEntry::new(profile));
        Ok(())
    });
    if let Err(e) = looked_up {
        e.log();
        return false;
    }
    if let Err(e) = whitelist.save() {
        log::warn!("Could not read old whitelist to convert it!\n{e}");
        return false;
    }
    rename_old_file(&old);
    true
}

/// `convertMobOwnerIfNecessary`.
pub fn convert_mob_owner_if_necessary(ctx: &ConversionContext, owner: &str) -> Option<Uuid> {
    if owner.is_empty() || owner.encode_utf16().count() > 16 {
        return mc_core::uuid_util::uuid_from_string(owner).ok();
    }
    if let Some(profile) = ctx.name_to_id_cache.get_by_name(owner) {
        return Some(profile.id);
    }
    if !ctx.is_singleplayer && ctx.uses_authentication {
        let mut found = None;
        // the callback never throws: failures are only logged
        let _ = ctx.lookup_players([owner], |name, result| {
            match result {
                Ok(id) => {
                    let profile = NameAndId::new(id, name);
                    ctx.name_to_id_cache.add(profile.clone());
                    found.get_or_insert(profile.id);
                }
                Err(e) => log::warn!("Could not lookup user whitelist entry for {name}\n{e}"),
            }
            Ok(())
        });
        found
    } else {
        Some(mc_core::uuid_util::create_offline_player_uuid(owner))
    }
}

/// `convertPlayers`: move `<world>/players/<name>.dat` to `<world>/players/data/<uuid>.dat`
/// (unknown names go to `<world>/unknownplayers/`). `world_dir` is the level directory.
pub fn convert_players(ctx: &ConversionContext, world_dir: &Path) -> bool {
    let old_dir = world_dir.join("players");
    let new_dir = old_dir.join("data");
    let unknown_dir = world_dir.join("unknownplayers");
    if !old_dir.is_dir() {
        return true;
    }
    let Ok(read) = fs::read_dir(&old_dir) else {
        return true;
    };
    let mut names = Vec::new();
    for entry in read.flatten() {
        let file_name = entry.file_name().to_string_lossy().into_owned();
        if file_name.to_lowercase().ends_with(".dat") {
            let player = &file_name[..file_name.len() - 4];
            if !player.is_empty() {
                names.push(player.to_owned());
            }
        }
    }
    let move_player_file = |dir: &Path, old_name: &str, new_name: &str| {
        ensure_directory_exists(dir)?;
        fs::rename(
            old_dir.join(format!("{old_name}.dat")),
            dir.join(format!("{new_name}.dat")),
        )
        .map_err(|_| ConversionError(format!("Could not convert file for {old_name}")))
    };
    let file_name_for_profile = |profile_name: &str| {
        names
            .iter()
            .find(|n| n.eq_ignore_ascii_case(profile_name))
            .ok_or_else(|| {
                ConversionError(format!(
                    "Could not find the filename for {profile_name} anymore"
                ))
            })
    };
    let converted = ctx.lookup_players(
        names.iter().map(String::as_str),
        |name, result| match result {
            Ok(id) => {
                ctx.name_to_id_cache.add(NameAndId::new(id, name));
                move_player_file(&new_dir, file_name_for_profile(name)?, &id.to_string())
            }
            Err(error) => {
                log::warn!("Could not lookup user uuid for {name}\n{error}");
                if error != ProfileLookupError::NotFound {
                    return Err(backend_error(name));
                }
                let file_name = file_name_for_profile(name)?;
                move_player_file(&unknown_dir, file_name, file_name)
            }
        },
    );
    if let Err(e) = converted {
        e.log();
        return false;
    }
    true
}

/// `ensureDirectoryExists`.
fn ensure_directory_exists(dir: &Path) -> Result<(), ConversionError> {
    let usable = if dir.exists() {
        dir.is_dir()
    } else {
        fs::create_dir_all(dir).is_ok()
    };
    if usable {
        return Ok(());
    }
    let name = dir
        .file_name()
        .map(|n| n.to_string_lossy())
        .unwrap_or_default();
    Err(ConversionError(format!(
        "Can't create directory {name} in world save directory."
    )))
}

/// Pause between the attempts of one conversion (`DedicatedServer.waitForRetry`).
pub const RETRY_PAUSE: Duration = Duration::from_millis(5000);

/// `DedicatedServer.convertOldUsers`: each of the five conversions is tried up to three times, `sleep` runs between
/// two attempts. True when at least one conversion succeeded.
pub fn convert_old_users(
    ctx: &ConversionContext,
    world_dir: &Path,
    sleep: &dyn Fn(Duration),
) -> bool {
    let retry = |what: &str, convert: &dyn Fn() -> bool| {
        for attempt in 0..3 {
            if attempt > 0 {
                log::warn!(
                    "Encountered a problem while converting the {what}, retrying in a few seconds"
                );
                sleep(RETRY_PAUSE);
            }
            if convert() {
                return true;
            }
        }
        false
    };
    let user_banlist = retry("user banlist", &|| convert_user_banlist(ctx));
    let ip_banlist = retry("ip banlist", &|| convert_ip_banlist(ctx));
    let ops_list = retry("op list", &|| convert_ops_list(ctx));
    let whitelist = retry("whitelist", &|| convert_white_list(ctx));
    let players = retry("player save files", &|| convert_players(ctx, world_dir));
    user_banlist || ip_banlist || ops_list || whitelist || players
}

/// `areOldUserlistsRemoved`.
pub fn are_old_userlists_removed(dir: &Path) -> bool {
    let found: Vec<&str> = [OLD_USERBANLIST, OLD_IPBANLIST, OLD_OPLIST, OLD_WHITELIST]
        .into_iter()
        .filter(|f| is_file(&dir.join(f)))
        .collect();
    if found.is_empty() {
        return true;
    }
    log::warn!("**** FAILED TO START THE SERVER AFTER ACCOUNT CONVERSION!");
    log::warn!("** please remove the following files and restart the server:");
    for f in found {
        log::warn!("* {f}");
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use crate::server::players::name_resolver::{
        CachedUserNameToIdResolver, NoProfileLookup, ProfileLookupCallback,
    };

    /// Answers every name from a table (unknown names: not found) and counts the batch lookups.
    struct Scripted {
        answers: Vec<(&'static str, Result<Uuid, ProfileLookupError>)>,
        lookups: AtomicUsize,
    }

    impl Scripted {
        fn new(answers: Vec<(&'static str, Result<Uuid, ProfileLookupError>)>) -> Arc<Self> {
            Arc::new(Self {
                answers,
                lookups: AtomicUsize::new(0),
            })
        }
    }

    impl ProfileLookup for Scripted {
        fn find_profile_by_name(&self, _name: &str) -> Option<NameAndId> {
            None
        }

        fn find_profiles_by_names(&self, names: &[String], callback: &mut ProfileLookupCallback) {
            self.lookups.fetch_add(1, Ordering::SeqCst);
            for name in names {
                let answer = self
                    .answers
                    .iter()
                    .find(|(n, _)| n.eq_ignore_ascii_case(name))
                    .map_or(Err(ProfileLookupError::NotFound), |(_, a)| a.clone());
                if callback(name, answer).is_break() {
                    return;
                }
            }
        }

        fn fetch_profile(&self, _id: Uuid) -> Option<mc_protocol::codec::GameProfile> {
            None
        }
    }

    fn backend_down() -> Result<Uuid, ProfileLookupError> {
        Err(ProfileLookupError::Backend(
            "com.mojang.authlib.exceptions.AuthenticationUnavailableException".to_owned(),
        ))
    }

    fn online_ctx(dir: &Path, lookup: Arc<Scripted>) -> ConversionContext {
        ConversionContext {
            uses_authentication: true,
            profile_lookup: lookup,
            ..ctx(dir)
        }
    }

    fn ctx(dir: &Path) -> ConversionContext {
        ConversionContext {
            dir: dir.to_owned(),
            uses_authentication: false,
            is_singleplayer: false,
            profile_lookup: Arc::new(NoProfileLookup),
            name_to_id_cache: Arc::new(CachedUserNameToIdResolver::new(
                Arc::new(NoProfileLookup),
                dir.join("usercache.json"),
            )),
            operator_permissions: LevelBasedPermissionSet::OWNER,
        }
    }

    #[test]
    fn converts_offline_lists_and_renames_the_old_files() {
        let dir = tempfile::tempdir().unwrap();
        let d = dir.path();
        fs::write(d.join(OLD_WHITELIST), "Alice\n\nbob\n").unwrap();
        fs::write(d.join(OLD_OPLIST), "Alice\n").unwrap();
        fs::write(
            d.join(OLD_USERBANLIST),
            "# comment\nGrief|2013-01-02 03:04:05 +0000|Server|forever|bad person\n",
        )
        .unwrap();
        fs::write(
            d.join(OLD_IPBANLIST),
            "1.2.3.4|2013-01-02 03:04:05 +0000|Server|forever|spam\n",
        )
        .unwrap();
        let c = ctx(d);
        assert!(!are_old_userlists_removed(d));
        assert!(
            convert_white_list(&c)
                && convert_ops_list(&c)
                && convert_user_banlist(&c)
                && convert_ip_banlist(&c)
        );
        assert!(are_old_userlists_removed(d));
        for f in [OLD_WHITELIST, OLD_OPLIST, OLD_USERBANLIST, OLD_IPBANLIST] {
            assert!(d.join(format!("{f}.converted")).exists());
        }
        let alice = NameAndId::create_offline("Alice");
        let wl = fs::read_to_string(d.join("whitelist.json")).unwrap();
        assert!(
            wl.contains(&alice.id.to_string()) && wl.contains("bob"),
            "{wl}"
        );
        let ops = fs::read_to_string(d.join("ops.json")).unwrap();
        assert!(ops.contains("\"level\": 4"), "{ops}");
        let bans = fs::read_to_string(d.join("banned-players.json")).unwrap();
        assert!(
            bans.contains("\"reason\": \"bad person\"") && bans.contains("\"source\": \"Server\""),
            "{bans}"
        );
        let ips = fs::read_to_string(d.join("banned-ips.json")).unwrap();
        assert!(
            ips.contains("\"ip\": \"1.2.3.4\"") && ips.contains("\"reason\": \"spam\""),
            "{ips}"
        );
    }

    #[test]
    fn mob_owner_and_player_files() {
        let dir = tempfile::tempdir().unwrap();
        let c = ctx(dir.path());
        // the name cache looks names up lower-cased (so an offline profile is made for "notch"), as vanilla does
        assert_eq!(
            convert_mob_owner_if_necessary(&c, "Notch"),
            Some(mc_core::uuid_util::create_offline_player_uuid("notch"))
        );
        assert_eq!(
            convert_mob_owner_if_necessary(&c, "069a79f4-44e9-4726-a5be-fca90e38aaf5")
                .unwrap()
                .to_string(),
            "069a79f4-44e9-4726-a5be-fca90e38aaf5"
        );
        assert_eq!(
            convert_mob_owner_if_necessary(&c, "not a uuid at all, too long"),
            None
        );
        let world = dir.path().join("world");
        fs::create_dir_all(world.join("players")).unwrap();
        fs::write(world.join("players/Steve.dat"), b"x").unwrap();
        assert!(convert_players(&c, &world));
        let id = mc_core::uuid_util::create_offline_player_uuid("Steve");
        assert!(
            world
                .join("players/data")
                .join(format!("{id}.dat"))
                .exists()
        );
        assert!(!world.join("players/Steve.dat").exists());
        assert!(!world.join("playerdata").exists());
    }

    #[test]
    fn unknown_names_go_to_unknownplayers_and_found_ones_to_players_data() {
        let dir = tempfile::tempdir().unwrap();
        let alex = Uuid::from_u128(0xa1e5);
        let c = online_ctx(dir.path(), Scripted::new(vec![("Alex", Ok(alex))]));
        let world = dir.path().join("world");
        fs::create_dir_all(world.join("players")).unwrap();
        fs::write(world.join("players/Alex.dat"), b"a").unwrap();
        fs::write(world.join("players/Ghost.dat"), b"g").unwrap();
        assert!(convert_players(&c, &world));
        assert_eq!(
            fs::read(world.join(format!("players/data/{alex}.dat"))).unwrap(),
            b"a"
        );
        assert_eq!(
            fs::read(world.join("unknownplayers/Ghost.dat")).unwrap(),
            b"g"
        );
        assert!(
            !world.join("players/Alex.dat").exists() && !world.join("players/Ghost.dat").exists()
        );
    }

    #[test]
    fn backend_error_aborts_the_player_conversion_and_keeps_the_file() {
        let dir = tempfile::tempdir().unwrap();
        let c = online_ctx(dir.path(), Scripted::new(vec![("Steve", backend_down())]));
        let world = dir.path().join("world");
        fs::create_dir_all(world.join("players")).unwrap();
        fs::write(world.join("players/Steve.dat"), b"x").unwrap();
        assert!(!convert_players(&c, &world));
        assert!(world.join("players/Steve.dat").exists());
        assert!(!world.join("unknownplayers").exists() && !world.join("players/data").exists());
    }

    #[test]
    fn lists_skip_unknown_names_but_stop_on_a_backend_error() {
        let dir = tempfile::tempdir().unwrap();
        let d = dir.path();
        let alex = Uuid::from_u128(0xa1e5);
        fs::write(d.join(OLD_WHITELIST), "Alex\nGhost\n").unwrap();
        let c = online_ctx(d, Scripted::new(vec![("Alex", Ok(alex))]));
        assert!(convert_white_list(&c));
        let wl = fs::read_to_string(d.join("whitelist.json")).unwrap();
        assert!(
            wl.contains(&alex.to_string()) && !wl.contains("Ghost"),
            "{wl}"
        );
        assert!(d.join(format!("{OLD_WHITELIST}.converted")).exists());

        fs::write(d.join(OLD_OPLIST), "Steve\n").unwrap();
        let c = online_ctx(d, Scripted::new(vec![("Steve", backend_down())]));
        assert!(!convert_ops_list(&c));
        assert!(d.join(OLD_OPLIST).exists() && !d.join("ops.json").exists());

        fs::write(
            d.join(OLD_USERBANLIST),
            "Steve|2013-01-02 03:04:05 +0000|Server|forever|x\n",
        )
        .unwrap();
        assert!(!convert_user_banlist(&c));
        assert!(d.join(OLD_USERBANLIST).exists() && !d.join("banned-players.json").exists());
    }

    #[test]
    fn a_failing_conversion_is_tried_three_times_with_the_pause() {
        let dir = tempfile::tempdir().unwrap();
        let d = dir.path();
        fs::write(d.join(OLD_OPLIST), "Steve\n").unwrap();
        let lookup = Scripted::new(vec![("Steve", backend_down())]);
        let c = online_ctx(d, lookup.clone());
        let sleeps = RefCell::new(Vec::new());
        convert_old_users(&c, &d.join("world"), &|pause| {
            sleeps.borrow_mut().push(pause)
        });
        assert_eq!(lookup.lookups.load(Ordering::SeqCst), 3);
        assert_eq!(*sleeps.borrow(), vec![RETRY_PAUSE; 2]);
        assert_eq!(RETRY_PAUSE, Duration::from_secs(5));
        assert!(d.join(OLD_OPLIST).exists());

        // a conversion that works on the first attempt never waits
        fs::write(d.join(OLD_WHITELIST), "Steve\n").unwrap();
        fs::remove_file(d.join(OLD_OPLIST)).unwrap();
        let c = ctx(d);
        sleeps.borrow_mut().clear();
        assert!(convert_old_users(&c, &d.join("world"), &|pause| sleeps
            .borrow_mut()
            .push(pause)));
        assert!(sleeps.borrow().is_empty());
    }
}
