//! `UserWhiteList`, `ServerOpList`, `UserBanList`, `IpBanList` and their entries.

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use super::ban_list_entry::{IpBanListEntry, UserBanListEntry};
use super::gson::{JsonError, JsonObject};
use super::name_and_id::NameAndId;
use super::stored_user_list::{LoadError, StoredUserEntry, StoredUserList};
use crate::server::notifications::NotificationService;
use crate::server::permissions::{LevelBasedPermissionSet, PermissionLevel};

fn id_key(user: &NameAndId) -> String {
    user.id.to_string()
}

/// `UserWhiteListEntry`.
#[derive(Clone, Debug)]
pub struct UserWhiteListEntry {
    user: Option<NameAndId>,
}

impl UserWhiteListEntry {
    pub fn new(user: NameAndId) -> Self {
        UserWhiteListEntry { user: Some(user) }
    }
}

impl StoredUserEntry for UserWhiteListEntry {
    type User = NameAndId;

    fn user(&self) -> Option<&NameAndId> {
        self.user.as_ref()
    }

    fn key_for_user(user: &NameAndId) -> String {
        id_key(user)
    }

    fn serialize(&self, object: &mut JsonObject) {
        if let Some(user) = &self.user {
            user.append_to(object);
        }
    }

    fn from_json(object: &JsonObject) -> Result<Self, JsonError> {
        Ok(UserWhiteListEntry { user: NameAndId::from_json(object)? })
    }
}

/// `ServerOpListEntry`.
#[derive(Clone, Debug)]
pub struct ServerOpListEntry {
    user: Option<NameAndId>,
    permissions: LevelBasedPermissionSet,
    bypasses_player_limit: bool,
}

impl ServerOpListEntry {
    pub fn new(user: NameAndId, permissions: LevelBasedPermissionSet, bypasses_player_limit: bool) -> Self {
        ServerOpListEntry { user: Some(user), permissions, bypasses_player_limit }
    }

    pub fn permissions(&self) -> LevelBasedPermissionSet {
        self.permissions
    }

    pub fn get_bypasses_player_limit(&self) -> bool {
        self.bypasses_player_limit
    }
}

impl StoredUserEntry for ServerOpListEntry {
    type User = NameAndId;

    fn user(&self) -> Option<&NameAndId> {
        self.user.as_ref()
    }

    fn key_for_user(user: &NameAndId) -> String {
        id_key(user)
    }

    fn serialize(&self, object: &mut JsonObject) {
        if let Some(user) = &self.user {
            user.append_to(object);
            object.add_property_int("level", self.permissions.level().id() as i64);
            object.add_property_bool("bypassesPlayerLimit", self.bypasses_player_limit);
        }
    }

    fn from_json(object: &JsonObject) -> Result<Self, JsonError> {
        let user = NameAndId::from_json(object)?;
        let level = match object.get("level") {
            Some(l) => PermissionLevel::by_id(l.as_int()?),
            None => PermissionLevel::All,
        };
        let bypasses_player_limit = match object.get("bypassesPlayerLimit") {
            Some(b) => b.as_bool()?,
            None => false,
        };
        Ok(ServerOpListEntry { user, permissions: LevelBasedPermissionSet::for_level(level), bypasses_player_limit })
    }
}

fn names<E: StoredUserEntry<User = NameAndId>>(list: &StoredUserList<E>) -> Vec<String> {
    list.get_entries().filter_map(|e| e.user()).map(|u| u.name.clone()).collect()
}

/// `UserWhiteList`.
pub struct UserWhiteList {
    list: StoredUserList<UserWhiteListEntry>,
    notifications: Arc<dyn NotificationService>,
}

impl UserWhiteList {
    pub fn new(file: impl Into<PathBuf>, notifications: Arc<dyn NotificationService>) -> Self {
        UserWhiteList { list: StoredUserList::new(file), notifications }
    }

    pub fn is_white_listed(&self, user: &NameAndId) -> bool {
        self.list.contains(user)
    }

    pub fn add(&mut self, infos: UserWhiteListEntry) -> bool {
        let user = infos.user().cloned();
        if !self.list.add(infos) {
            return false;
        }
        if let Some(user) = user {
            self.notifications.player_added_to_allowlist(&user);
        }
        true
    }

    pub fn remove(&mut self, user: &NameAndId) -> bool {
        if !self.list.remove(user) {
            return false;
        }
        self.notifications.player_removed_from_allowlist(user);
        true
    }

    pub fn clear(&mut self) {
        for user in self.list.get_entries().filter_map(|e| e.user()) {
            self.notifications.player_removed_from_allowlist(user);
        }
        self.list.clear();
    }

    pub fn get_user_list(&self) -> Vec<String> {
        names(&self.list)
    }

    pub fn get(&mut self, user: &NameAndId) -> Option<&UserWhiteListEntry> {
        self.list.get(user)
    }

    pub fn get_entries(&self) -> impl Iterator<Item = &UserWhiteListEntry> {
        self.list.get_entries()
    }

    pub fn is_empty(&self) -> bool {
        self.list.is_empty()
    }

    pub fn save(&self) -> std::io::Result<()> {
        self.list.save()
    }

    pub fn load(&mut self) -> Result<(), LoadError> {
        self.list.load()
    }

    pub fn get_file(&self) -> &std::path::Path {
        self.list.get_file()
    }
}

/// `ServerOpList`.
pub struct ServerOpList {
    list: StoredUserList<ServerOpListEntry>,
    notifications: Arc<dyn NotificationService>,
}

impl ServerOpList {
    pub fn new(file: impl Into<PathBuf>, notifications: Arc<dyn NotificationService>) -> Self {
        ServerOpList { list: StoredUserList::new(file), notifications }
    }

    pub fn add(&mut self, infos: ServerOpListEntry) -> bool {
        if !self.list.add(infos.clone()) {
            return false;
        }
        if infos.user().is_some() {
            self.notifications.player_oped(&infos);
        }
        true
    }

    pub fn remove(&mut self, user: &NameAndId) -> bool {
        let entry = self.list.get(user).cloned();
        if !self.list.remove(user) {
            return false;
        }
        if let Some(entry) = entry {
            self.notifications.player_deoped(&entry);
        }
        true
    }

    pub fn clear(&mut self) {
        for entry in self.list.get_entries() {
            if entry.user().is_some() {
                self.notifications.player_deoped(entry);
            }
        }
        self.list.clear();
    }

    pub fn can_bypass_player_limit(&mut self, user: &NameAndId) -> bool {
        self.list.get(user).is_some_and(ServerOpListEntry::get_bypasses_player_limit)
    }

    pub fn get_user_list(&self) -> Vec<String> {
        names(&self.list)
    }

    pub fn get(&mut self, user: &NameAndId) -> Option<&ServerOpListEntry> {
        self.list.get(user)
    }

    pub fn contains(&self, user: &NameAndId) -> bool {
        self.list.contains(user)
    }

    pub fn get_entries(&self) -> impl Iterator<Item = &ServerOpListEntry> {
        self.list.get_entries()
    }

    pub fn is_empty(&self) -> bool {
        self.list.is_empty()
    }

    pub fn save(&self) -> std::io::Result<()> {
        self.list.save()
    }

    pub fn load(&mut self) -> Result<(), LoadError> {
        self.list.load()
    }

    pub fn get_file(&self) -> &std::path::Path {
        self.list.get_file()
    }
}

/// `UserBanList`.
pub struct UserBanList {
    list: StoredUserList<UserBanListEntry>,
    notifications: Arc<dyn NotificationService>,
}

impl UserBanList {
    pub fn new(file: impl Into<PathBuf>, notifications: Arc<dyn NotificationService>) -> Self {
        UserBanList { list: StoredUserList::new(file), notifications }
    }

    pub fn is_banned(&self, user: &NameAndId) -> bool {
        self.list.contains(user)
    }

    pub fn add(&mut self, infos: UserBanListEntry) -> bool {
        if !self.list.add(infos.clone()) {
            return false;
        }
        if infos.user().is_some() {
            self.notifications.player_banned(&infos);
        }
        true
    }

    pub fn remove(&mut self, user: &NameAndId) -> bool {
        if !self.list.remove(user) {
            return false;
        }
        self.notifications.player_unbanned(user);
        true
    }

    pub fn clear(&mut self) {
        for user in self.list.get_entries().filter_map(|e| e.user()) {
            self.notifications.player_unbanned(user);
        }
        self.list.clear();
    }

    pub fn get_user_list(&self) -> Vec<String> {
        names(&self.list)
    }

    pub fn get(&mut self, user: &NameAndId) -> Option<&UserBanListEntry> {
        self.list.get(user)
    }

    pub fn get_entries(&self) -> impl Iterator<Item = &UserBanListEntry> {
        self.list.get_entries()
    }

    pub fn is_empty(&self) -> bool {
        self.list.is_empty()
    }

    pub fn save(&self) -> std::io::Result<()> {
        self.list.save()
    }

    pub fn load(&mut self) -> Result<(), LoadError> {
        self.list.load()
    }

    pub fn get_file(&self) -> &std::path::Path {
        self.list.get_file()
    }
}

/// `IpBanList`.
pub struct IpBanList {
    list: StoredUserList<IpBanListEntry>,
    notifications: Arc<dyn NotificationService>,
}

impl IpBanList {
    pub fn new(file: impl Into<PathBuf>, notifications: Arc<dyn NotificationService>) -> Self {
        IpBanList { list: StoredUserList::new(file), notifications }
    }

    /// `getIpFromAddress` for a `SocketAddress.toString()` (`host/ip:port` or `/ip:port`): the text between the
    /// first `/` and the next `:`.
    pub fn get_ip_from_address(address: &str) -> String {
        let mut ip = address;
        if let Some(i) = ip.find('/') {
            ip = &ip[i + 1..];
        }
        if let Some(i) = ip.find(':') {
            ip = &ip[..i];
        }
        ip.to_owned()
    }

    /// The `isBanned(SocketAddress)` key for a connected peer (Java's `InetSocketAddress.toString()` is `/ip:port`;
    /// for IPv6 the part before the first `:` is empty, exactly like vanilla).
    pub fn get_ip_from_socket_addr(address: &SocketAddr) -> String {
        Self::get_ip_from_address(&format!("/{address}"))
    }

    pub fn is_banned(&self, ip: &str) -> bool {
        self.list.contains(&ip.to_owned())
    }

    pub fn is_banned_address(&self, address: &SocketAddr) -> bool {
        self.is_banned(&Self::get_ip_from_socket_addr(address))
    }

    pub fn get(&mut self, ip: &str) -> Option<&IpBanListEntry> {
        self.list.get(&ip.to_owned())
    }

    pub fn get_address(&mut self, address: &SocketAddr) -> Option<&IpBanListEntry> {
        let ip = Self::get_ip_from_socket_addr(address);
        self.get(&ip)
    }

    pub fn add(&mut self, infos: IpBanListEntry) -> bool {
        if !self.list.add(infos.clone()) {
            return false;
        }
        if infos.user().is_some() {
            self.notifications.ip_banned(&infos);
        }
        true
    }

    pub fn remove(&mut self, ip: &str) -> bool {
        if !self.list.remove(&ip.to_owned()) {
            return false;
        }
        self.notifications.ip_unbanned(ip);
        true
    }

    pub fn clear(&mut self) {
        for ip in self.list.get_entries().filter_map(|e| e.user()) {
            self.notifications.ip_unbanned(ip);
        }
        self.list.clear();
    }

    /// `getUserList` (map keys in Java order).
    pub fn get_user_list(&self) -> Vec<String> {
        self.list.keys()
    }

    pub fn get_entries(&self) -> impl Iterator<Item = &IpBanListEntry> {
        self.list.get_entries()
    }

    pub fn is_empty(&self) -> bool {
        self.list.is_empty()
    }

    pub fn save(&self) -> std::io::Result<()> {
        self.list.save()
    }

    pub fn load(&mut self) -> Result<(), LoadError> {
        self.list.load()
    }

    pub fn get_file(&self) -> &std::path::Path {
        self.list.get_file()
    }
}
