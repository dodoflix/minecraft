//! `StoredUserList` / `StoredUserEntry`: the JSON-file backed lists behind `ops.json`, `whitelist.json`,
//! `banned-players.json` and `banned-ips.json`. Entries are keyed in a `java.util.HashMap`, so the file order is
//! Java's hash order ([`JavaHashMap`]).

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use super::gson::{self, JsonElement, JsonError, JsonObject};
use super::java_hash_map::JavaHashMap;

/// `StoredUserEntry<K>`.
pub trait StoredUserEntry: Sized {
    type User;

    /// `getUser` (`None` for an entry vanilla could not resolve).
    fn user(&self) -> Option<&Self::User>;

    /// `StoredUserList.getKeyForUser` of the list that holds this entry type.
    fn key_for_user(user: &Self::User) -> String;

    fn has_expired(&self) -> bool {
        false
    }

    /// `serialize`.
    fn serialize(&self, object: &mut JsonObject);

    /// `createEntry(JsonObject)`.
    fn from_json(object: &JsonObject) -> Result<Self, JsonError>;

    /// `infos.equals(previous)`; entries without a vanilla `equals` are never equal (identity).
    fn equals(&self, _other: &Self) -> bool {
        false
    }
}

#[derive(Debug, thiserror::Error)]
pub enum LoadError {
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error("{0}")]
    Json(#[from] JsonError),
}

pub struct StoredUserList<E: StoredUserEntry> {
    file: PathBuf,
    map: JavaHashMap<E>,
}

impl<E: StoredUserEntry> StoredUserList<E> {
    pub fn new(file: impl Into<PathBuf>) -> Self {
        StoredUserList {
            file: file.into(),
            map: JavaHashMap::new(),
        }
    }

    pub fn get_file(&self) -> &Path {
        &self.file
    }

    /// `add`: `false` when an equal entry is already present; saves on change (a failed save is only logged).
    pub fn add(&mut self, infos: E) -> bool {
        let Some(user) = infos.user() else {
            return false;
        };
        let key = E::key_for_user(user);
        if self
            .map
            .get(&key)
            .is_some_and(|previous| infos.equals(previous))
        {
            return false;
        }
        self.map.insert(key, infos);
        if let Err(e) = self.save() {
            log::warn!("Could not save the list after adding a user.\n{e}");
        }
        true
    }

    /// `get`: drops expired entries first (without saving).
    pub fn get(&mut self, user: &E::User) -> Option<&E> {
        self.remove_expired();
        self.map.get(&E::key_for_user(user))
    }

    pub fn remove(&mut self, user: &E::User) -> bool {
        if self.map.remove(&E::key_for_user(user)).is_none() {
            return false;
        }
        if let Err(e) = self.save() {
            log::warn!("Could not save the list after removing a user.\n{e}");
        }
        true
    }

    pub fn clear(&mut self) {
        self.map.clear();
        if let Err(e) = self.save() {
            log::warn!("Could not save the list after removing a user.\n{e}");
        }
    }

    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    /// `contains` (no expiry sweep, like vanilla).
    pub fn contains(&self, user: &E::User) -> bool {
        self.map.contains_key(&E::key_for_user(user))
    }

    /// Map keys in Java iteration order (`getUserList` of the default implementation).
    pub fn keys(&self) -> Vec<String> {
        self.map.keys().map(str::to_owned).collect()
    }

    /// `getEntries`.
    pub fn get_entries(&self) -> impl Iterator<Item = &E> {
        self.map.values()
    }

    fn remove_expired(&mut self) {
        self.map.retain(|e| !e.has_expired());
    }

    pub fn save(&self) -> io::Result<()> {
        let array = JsonElement::Array(
            self.map
                .values()
                .map(|entry| {
                    let mut object = JsonObject::new();
                    entry.serialize(&mut object);
                    JsonElement::Object(object)
                })
                .collect(),
        );
        fs::write(&self.file, array.to_pretty_string())
    }

    /// `load`: replaces the contents with the file's (nothing happens when the file does not exist). A malformed
    /// entry aborts the load, leaving the entries read so far, like vanilla.
    pub fn load(&mut self) -> Result<(), LoadError> {
        if !self.file.exists() {
            return Ok(());
        }
        let text = String::from_utf8_lossy(&fs::read(&self.file)?).into_owned();
        self.map.clear();
        let Some(contents) = gson::parse(&text, true)? else {
            return Ok(());
        };
        let JsonElement::Array(elements) = contents else {
            return Err(JsonError("Expected a com.google.gson.JsonArray".into()).into());
        };
        for element in &elements {
            let JsonElement::Object(object) = element else {
                return Err(JsonError("Expected entry to be a JsonObject".into()).into());
            };
            let entry = E::from_json(object)?;
            if let Some(user) = entry.user() {
                let key = E::key_for_user(user);
                self.map.insert(key, entry);
            }
        }
        Ok(())
    }
}
