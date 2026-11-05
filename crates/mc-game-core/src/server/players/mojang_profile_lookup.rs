//! `ProfileLookup` against the Mojang services, like authlib's `MinecraftServicesProfileRepository` /
//! `MinecraftServicesSessionService`: the endpoints come from the discovery document, name lookups are
//! `getByName`, profile fetches are `getProfileById?unsigned=false` (`fetchProfile(id, true)`). Failures are logged
//! like authlib does (`Couldn't find profile with name: ..`) and answer "not found".

use std::time::Duration;

use mc_protocol::codec::{GameProfile, Property};
use parking_lot::Mutex;
use serde_json::Value;
use uuid::Uuid;

use super::name_and_id::NameAndId;
use super::name_resolver::ProfileLookup;

/// `MinecraftServicesEnvironment.PROD`.
const DISCOVERY_URL: &str = "https://discovery.minecraftservices.com/minecraft/client";
const TIMEOUT: Duration = Duration::from_secs(15);

#[derive(Default)]
pub struct MojangProfileLookup {
    discovery: Mutex<Option<Value>>,
}

impl MojangProfileLookup {
    pub fn new() -> Self {
        Self::default()
    }

    fn client() -> Result<reqwest::blocking::Client, reqwest::Error> {
        reqwest::blocking::Client::builder().connect_timeout(TIMEOUT).timeout(TIMEOUT).build()
    }

    /// `GET url` -> `Ok(None)` for 204/404 (authlib: no profile), the parsed JSON otherwise.
    fn get_json(url: &str) -> Result<Option<Value>, reqwest::Error> {
        let response = Self::client()?.get(url).send()?;
        if matches!(response.status().as_u16(), 204 | 404) {
            return Ok(None);
        }
        Ok(Some(response.error_for_status()?.json()?))
    }

    /// `MinecraftServicesDiscoveryService.getUrl(service, key)`; the document is fetched until it succeeded once.
    fn endpoint(&self, service: &str, key: &str) -> Option<String> {
        let mut discovery = self.discovery.lock();
        if discovery.is_none() {
            match Self::get_json(DISCOVERY_URL) {
                Ok(Some(document)) => *discovery = Some(document),
                Ok(None) => {}
                Err(e) => log::warn!("Failed to fetch discovery document from {DISCOVERY_URL}: {e}"),
            }
        }
        discovery.as_ref()?.pointer(&format!("/discovery/{service}/endpoints/{key}/uri"))?.as_str().map(str::to_owned)
    }
}

impl ProfileLookup for MojangProfileLookup {
    fn find_profile_by_name(&self, name: &str) -> Option<NameAndId> {
        let url = self.endpoint("profiles", "getByName")?.replace("{name}", &name.to_lowercase());
        match Self::get_json(&url) {
            Ok(Some(body)) => Some(NameAndId::new(Uuid::parse_str(body.get("id")?.as_str()?).ok()?, body.get("name")?.as_str()?)),
            Ok(None) => None,
            Err(e) => {
                log::warn!("Couldn't find profile with name: {name}\n{e}");
                None
            }
        }
    }

    fn fetch_profile(&self, id: Uuid) -> Option<GameProfile> {
        let url = self.endpoint("session", "getProfileById")?.replace("{profileId}", &id.simple().to_string());
        match Self::get_json(&format!("{url}?unsigned=false")) {
            Ok(Some(body)) => {
                let properties = body
                    .get("properties")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .filter_map(|p| {
                        Some(Property {
                            name: p.get("name")?.as_str()?.to_owned(),
                            value: p.get("value")?.as_str()?.to_owned(),
                            signature: p.get("signature").and_then(Value::as_str).map(str::to_owned),
                        })
                    })
                    .collect();
                Some(GameProfile { id: Uuid::parse_str(body.get("id")?.as_str()?).ok()?, name: body.get("name")?.as_str()?.to_owned(), properties })
            }
            Ok(None) => None,
            Err(e) => {
                log::warn!("Couldn't look up profile properties for {id}\n{e}");
                None
            }
        }
    }
}
