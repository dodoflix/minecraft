//! `net.minecraft.server.players.NameAndId`.

use mc_core::uuid_util;
use uuid::Uuid;

use super::gson::{JsonError, JsonObject};

/// A player's profile id and name.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct NameAndId {
    pub id: Uuid,
    pub name: String,
}

impl NameAndId {
    pub fn new(id: Uuid, name: impl Into<String>) -> NameAndId {
        NameAndId { id, name: name.into() }
    }

    /// `NameAndId.fromJson`: `Ok(None)` when `uuid`/`name` are missing or the uuid does not parse; `Err` where the
    /// vanilla `getAsString` throws (non-primitive members).
    pub fn from_json(object: &JsonObject) -> Result<Option<NameAndId>, JsonError> {
        let (Some(uuid), Some(name)) = (object.get("uuid"), object.get("name")) else { return Ok(None) };
        let uuid_string = uuid.as_string()?;
        let Ok(id) = uuid_util::uuid_from_string(&uuid_string) else { return Ok(None) };
        Ok(Some(NameAndId { id, name: name.as_string()? }))
    }

    /// `appendTo`: `uuid`, then `name`.
    pub fn append_to(&self, output: &mut JsonObject) {
        output.add_property_str("uuid", Some(&self.id.to_string()));
        output.add_property_str("name", Some(&self.name));
    }

    /// `NameAndId.createOffline`.
    pub fn create_offline(name: &str) -> NameAndId {
        NameAndId { id: uuid_util::create_offline_player_uuid(name), name: name.to_owned() }
    }
}

impl From<&mc_protocol::codec::GameProfile> for NameAndId {
    fn from(profile: &mc_protocol::codec::GameProfile) -> Self {
        NameAndId { id: profile.id, name: profile.name.clone() }
    }
}

impl std::fmt::Display for NameAndId {
    /// Java's record `toString`, as vanilla prints profiles in the management server log.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "NameAndId[id={}, name={}]", self.id, self.name)
    }
}
