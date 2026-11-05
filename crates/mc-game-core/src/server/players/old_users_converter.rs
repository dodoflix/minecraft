//! `OldUsersConverter`: migration of the pre-1.7.6 text user lists (`banned-players.txt`, `banned-ips.txt`,
//! `ops.txt`, `white-list.txt`) and per-name player files to the UUID based JSON lists.
//! shortcut: [`ProfileLookup`] cannot tell "not found" from "backend unreachable", so a lookup that fails for network
//! reasons is treated as not found (vanilla aborts the conversion with `Conversion failed, please try again later`).

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use uuid::Uuid;

use super::ban_list_entry::{IpBanListEntry, UserBanListEntry, parse_date};
use super::java_hash_map::JavaHashMap;
use super::name_and_id::NameAndId;
use super::name_resolver::{ProfileLookup, UserNameToIdResolver};
use super::user_lists::{IpBanList, ServerOpList, ServerOpListEntry, UserBanList, UserWhiteList, UserWhiteListEntry};
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

    /// `lookupPlayers`: `(canonical name, uuid)` per resolvable name, `None` for names Mojang does not know.
    fn lookup_players<'a>(&self, names: impl IntoIterator<Item = &'a str>, mut on_result: impl FnMut(&str, Option<Uuid>)) {
        for name in names.into_iter().filter(|s| !s.is_empty()) {
            if self.uses_authentication {
                match self.profile_lookup.find_profile_by_name(name) {
                    Some(profile) => on_result(&profile.name, Some(profile.id)),
                    None => on_result(name, None),
                }
            } else {
                on_result(name, Some(mc_core::uuid_util::create_offline_player_uuid(name)));
            }
        }
    }
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
fn read_old_list_format(file: &Path, user_map: &mut JavaHashMap<Vec<String>>) -> std::io::Result<Vec<String>> {
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
    Ok(String::from_utf8_lossy(&fs::read(file)?).lines().map(str::to_owned).collect())
}

fn is_file(path: &Path) -> bool {
    path.is_file()
}

fn rename_old_file(path: &Path) {
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let _ = fs::rename(path, path.with_file_name(format!("{name}.converted")));
}

fn old_columns(def: &[String]) -> (Option<super::ban_list_entry::Date>, Option<&str>, Option<super::ban_list_entry::Date>, Option<&str>) {
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
    let mut bans = UserBanList::new(ctx.path("banned-players.json"), Arc::new(EmptyNotificationService));
    if bans.get_file().exists() {
        if let Err(e) = bans.load() {
            log::warn!("Could not load existing file banned-players.json\n{e}");
        }
    }
    let mut user_map = JavaHashMap::new();
    if let Err(e) = read_old_list_format(&old, &mut user_map) {
        log::warn!("Could not read old user banlist to convert it!\n{e}");
        return false;
    }
    let names: Vec<String> = user_map.keys().map(str::to_owned).collect();
    let mut failed = false;
    ctx.lookup_players(names.iter().map(String::as_str), |name, id| match id {
        Some(id) => {
            let profile = NameAndId::new(id, name);
            ctx.name_to_id_cache.add(profile.clone());
            match user_map.get(&profile.name.to_lowercase()) {
                None => {
                    log::warn!("Could not convert user banlist entry for {}", profile.name);
                    failed = true;
                }
                Some(def) => {
                    let (created, source, expires, reason) = old_columns(def);
                    bans.add(UserBanListEntry::new(Some(profile), created, source, expires, reason));
                }
            }
        }
        None => log::warn!("Could not lookup user banlist entry for {name}\ncom.mojang.authlib.yggdrasil.ProfileNotFoundException: Server returned HTTP response code: 204"),
    });
    if failed {
        log::error!("Conversion failed, please try again later\ncom.mojang..ConversionError: Profile not in the conversionlist");
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
    let mut ip_bans = IpBanList::new(ctx.path("banned-ips.json"), Arc::new(EmptyNotificationService));
    if ip_bans.get_file().exists() {
        if let Err(e) = ip_bans.load() {
            log::warn!("Could not load existing file banned-ips.json\n{e}");
        }
    }
    let mut user_map = JavaHashMap::new();
    if let Err(e) = read_old_list_format(&old, &mut user_map) {
        log::warn!("Could not parse old ip banlist to convert it!\n{e}");
        return false;
    }
    for (key, def) in user_map.iter() {
        let (created, source, expires, reason) = old_columns(def);
        ip_bans.add(IpBanListEntry::new(Some(key.to_owned()), created, source, expires, reason));
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
    if ops.get_file().exists() {
        if let Err(e) = ops.load() {
            log::warn!("Could not load existing file ops.json\n{e}");
        }
    }
    let lines = match read_lines(&old) {
        Ok(l) => l,
        Err(e) => {
            log::warn!("Could not read old oplist to convert it!\n{e}");
            return false;
        }
    };
    ctx.lookup_players(lines.iter().map(String::as_str), |name, id| match id {
        Some(id) => {
            let profile = NameAndId::new(id, name);
            ctx.name_to_id_cache.add(profile.clone());
            ops.add(ServerOpListEntry::new(profile, ctx.operator_permissions, false));
        }
        None => log::warn!("Could not lookup oplist entry for {name}\ncom.mojang.authlib.yggdrasil.ProfileNotFoundException: Server returned HTTP response code: 204"),
    });
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
    let mut whitelist = UserWhiteList::new(ctx.path("whitelist.json"), Arc::new(EmptyNotificationService));
    if whitelist.get_file().exists() {
        if let Err(e) = whitelist.load() {
            log::warn!("Could not load existing file whitelist.json\n{e}");
        }
    }
    let lines = match read_lines(&old) {
        Ok(l) => l,
        Err(e) => {
            log::warn!("Could not read old whitelist to convert it!\n{e}");
            return false;
        }
    };
    ctx.lookup_players(lines.iter().map(String::as_str), |name, id| match id {
        Some(id) => {
            let profile = NameAndId::new(id, name);
            ctx.name_to_id_cache.add(profile.clone());
            whitelist.add(UserWhiteListEntry::new(profile));
        }
        None => log::warn!("Could not lookup user whitelist entry for {name}\ncom.mojang.authlib.yggdrasil.ProfileNotFoundException: Server returned HTTP response code: 204"),
    });
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
        ctx.lookup_players([owner], |name, id| match id {
            Some(id) => {
                let profile = NameAndId::new(id, name);
                ctx.name_to_id_cache.add(profile.clone());
                found.get_or_insert(profile.id);
            }
            None => log::warn!("Could not lookup user whitelist entry for {name}"),
        });
        found
    } else {
        Some(mc_core::uuid_util::create_offline_player_uuid(owner))
    }
}

/// `convertPlayers`: move `<world>/players/<name>.dat` to `<world>/playerdata/<uuid>.dat`
/// (unknown names go to `<world>/unknownplayers/`). `world_dir` is the level directory.
pub fn convert_players(ctx: &ConversionContext, world_dir: &Path) -> bool {
    let old_dir = world_dir.join("players");
    let new_dir = world_dir.join("playerdata");
    let unknown_dir = world_dir.join("unknownplayers");
    if !old_dir.is_dir() {
        return true;
    }
    let Ok(read) = fs::read_dir(&old_dir) else { return true };
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
    let mut failed: Option<String> = None;
    ctx.lookup_players(names.iter().map(String::as_str), |name, id| {
        let Some(file_name) = names.iter().find(|n| n.eq_ignore_ascii_case(name)) else {
            failed = Some(format!("Could not find the filename for {name} anymore"));
            return;
        };
        let (dir, new_name) = match id {
            Some(id) => {
                ctx.name_to_id_cache.add(NameAndId::new(id, name));
                (&new_dir, id.to_string())
            }
            None => {
                log::warn!("Could not lookup user uuid for {name}\ncom.mojang.authlib.yggdrasil.ProfileNotFoundException: Server returned HTTP response code: 204");
                (&unknown_dir, file_name.clone())
            }
        };
        let moved = fs::create_dir_all(dir).and_then(|()| fs::rename(old_dir.join(format!("{file_name}.dat")), dir.join(format!("{new_name}.dat"))));
        if moved.is_err() {
            failed = Some(format!("Could not convert file for {file_name}"));
        }
    });
    if let Some(reason) = failed {
        log::error!("Conversion failed, please try again later\n{reason}");
        return false;
    }
    true
}

/// `areOldUserlistsRemoved`.
pub fn are_old_userlists_removed(dir: &Path) -> bool {
    let found: Vec<&str> = [OLD_USERBANLIST, OLD_IPBANLIST, OLD_OPLIST, OLD_WHITELIST].into_iter().filter(|f| is_file(&dir.join(f))).collect();
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
    use crate::server::players::name_resolver::{CachedUserNameToIdResolver, NoProfileLookup};

    fn ctx(dir: &Path) -> ConversionContext {
        ConversionContext {
            dir: dir.to_owned(),
            uses_authentication: false,
            is_singleplayer: false,
            profile_lookup: Arc::new(NoProfileLookup),
            name_to_id_cache: Arc::new(CachedUserNameToIdResolver::new(Arc::new(NoProfileLookup), dir.join("usercache.json"))),
            operator_permissions: LevelBasedPermissionSet::OWNER,
        }
    }

    #[test]
    fn converts_offline_lists_and_renames_the_old_files() {
        let dir = tempfile::tempdir().unwrap();
        let d = dir.path();
        fs::write(d.join(OLD_WHITELIST), "Alice\n\nbob\n").unwrap();
        fs::write(d.join(OLD_OPLIST), "Alice\n").unwrap();
        fs::write(d.join(OLD_USERBANLIST), "# comment\nGrief|2013-01-02 03:04:05 +0000|Server|forever|bad person\n").unwrap();
        fs::write(d.join(OLD_IPBANLIST), "1.2.3.4|2013-01-02 03:04:05 +0000|Server|forever|spam\n").unwrap();
        let c = ctx(d);
        assert!(!are_old_userlists_removed(d));
        assert!(convert_white_list(&c) && convert_ops_list(&c) && convert_user_banlist(&c) && convert_ip_banlist(&c));
        assert!(are_old_userlists_removed(d));
        for f in [OLD_WHITELIST, OLD_OPLIST, OLD_USERBANLIST, OLD_IPBANLIST] {
            assert!(d.join(format!("{f}.converted")).exists());
        }
        let alice = NameAndId::create_offline("Alice");
        let wl = fs::read_to_string(d.join("whitelist.json")).unwrap();
        assert!(wl.contains(&alice.id.to_string()) && wl.contains("bob"), "{wl}");
        let ops = fs::read_to_string(d.join("ops.json")).unwrap();
        assert!(ops.contains("\"level\": 4"), "{ops}");
        let bans = fs::read_to_string(d.join("banned-players.json")).unwrap();
        assert!(bans.contains("\"reason\": \"bad person\"") && bans.contains("\"source\": \"Server\""), "{bans}");
        let ips = fs::read_to_string(d.join("banned-ips.json")).unwrap();
        assert!(ips.contains("\"ip\": \"1.2.3.4\"") && ips.contains("\"reason\": \"spam\""), "{ips}");
    }

    #[test]
    fn mob_owner_and_player_files() {
        let dir = tempfile::tempdir().unwrap();
        let c = ctx(dir.path());
        // the name cache looks names up lower-cased (so an offline profile is made for "notch"), as vanilla does
        assert_eq!(convert_mob_owner_if_necessary(&c, "Notch"), Some(mc_core::uuid_util::create_offline_player_uuid("notch")));
        assert_eq!(convert_mob_owner_if_necessary(&c, "069a79f4-44e9-4726-a5be-fca90e38aaf5").unwrap().to_string(), "069a79f4-44e9-4726-a5be-fca90e38aaf5");
        assert_eq!(convert_mob_owner_if_necessary(&c, "not a uuid at all, too long"), None);
        let world = dir.path().join("world");
        fs::create_dir_all(world.join("players")).unwrap();
        fs::write(world.join("players/Steve.dat"), b"x").unwrap();
        assert!(convert_players(&c, &world));
        let id = mc_core::uuid_util::create_offline_player_uuid("Steve");
        assert!(world.join("playerdata").join(format!("{id}.dat")).exists());
    }
}
