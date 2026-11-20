//! `ProfileLookup` against the Mojang services, like authlib's `MinecraftServicesProfileRepository` /
//! `MinecraftServicesSessionService`: the endpoints come from the discovery document, single name lookups are
//! `getByName`, batch lookups `getManyByName` (pages of 10, three attempts per page), profile fetches are
//! `getProfileById?unsigned=false` (`fetchProfile(id, true)`). A failing single lookup is logged like authlib does
//! (`Couldn't find profile with name: ..`) and answers "not found"; a failing batch reports a backend error.

use std::collections::HashSet;
use std::time::Duration;

use mc_protocol::codec::{GameProfile, Property};
use parking_lot::Mutex;
use serde_json::Value;
use uuid::Uuid;

use super::java_hash_map::JavaHashMap;
use super::name_and_id::NameAndId;
use super::name_resolver::{ProfileLookup, ProfileLookupCallback, ProfileLookupError};
use crate::server::services_keys::discovery_url;

const TIMEOUT: Duration = Duration::from_secs(15);
/// `MinecraftClient.CONNECT_TIMEOUT_MS` / `READ_TIMEOUT_MS`.
const BATCH_TIMEOUT: Duration = Duration::from_secs(5);

/// `MinecraftServicesProfileRepository` constants.
const ENTRIES_PER_PAGE: usize = 10;
const MAX_FAIL_COUNT: u32 = 3;
const DELAY_BETWEEN_PAGES: Duration = Duration::from_millis(100);
const DELAY_BETWEEN_FAILURES: Duration = Duration::from_millis(750);

pub struct MojangProfileLookup {
    discovery_url: String,
    discovery: Mutex<Option<Value>>,
}

impl Default for MojangProfileLookup {
    fn default() -> Self {
        Self::with_discovery_url(discovery_url())
    }
}

impl MojangProfileLookup {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_discovery_url(discovery_url: String) -> Self {
        Self {
            discovery_url,
            discovery: Mutex::new(None),
        }
    }

    fn client(timeout: Duration) -> Result<reqwest::blocking::Client, reqwest::Error> {
        reqwest::blocking::Client::builder()
            .connect_timeout(timeout)
            .timeout(timeout)
            .build()
    }

    /// `GET url` -> `Ok(None)` for 204/404 (authlib: no profile), the parsed JSON otherwise.
    fn get_json(url: &str) -> Result<Option<Value>, reqwest::Error> {
        let response = Self::client(TIMEOUT)?.get(url).send()?;
        if matches!(response.status().as_u16(), 204 | 404) {
            return Ok(None);
        }
        Ok(Some(response.error_for_status()?.json()?))
    }

    /// `MinecraftServicesDiscoveryService.getUrl(service, key)`; the document is fetched until it succeeded once.
    fn endpoint(&self, service: &str, key: &str) -> Option<String> {
        let mut discovery = self.discovery.lock();
        if discovery.is_none() {
            let url = &self.discovery_url;
            match Self::get_json(url) {
                Ok(Some(document)) => *discovery = Some(document),
                Ok(None) => {}
                Err(e) => {
                    log::warn!("Failed to fetch discovery document from {url}: {e}")
                }
            }
        }
        discovery
            .as_ref()?
            .pointer(&format!("/discovery/{service}/endpoints/{key}/uri"))?
            .as_str()
            .map(str::to_owned)
    }

    /// `MinecraftClient.post(getManyByName, names)`: `(id, name)` of the profiles Mojang knows. `Err` is the first
    /// line of the exception authlib raises (`MinecraftClientException.toAuthenticationException`).
    fn post_names(&self, names: &[String]) -> Result<Vec<(Uuid, String)>, String> {
        const UNAVAILABLE: &str =
            "com.mojang.authlib.exceptions.AuthenticationUnavailableException";
        let url = self
            .endpoint("profiles", "getManyByName")
            .ok_or_else(|| UNAVAILABLE.to_owned())?;
        let unavailable = |e: reqwest::Error| {
            log::debug!("Failed to POST {url}: {e}");
            UNAVAILABLE.to_owned()
        };
        let response = Self::client(BATCH_TIMEOUT)
            .and_then(|client| client.post(&url).json(names).send())
            .map_err(unavailable)?;
        let status = response.status().as_u16();
        if status >= 400 {
            return Err(match status {
                401 => format!(
                    "com.mojang.authlib.exceptions.InvalidCredentialsException: Status: {status}"
                ),
                500.. => format!("{UNAVAILABLE}: Status: {status}"),
                _ => format!(
                    "com.mojang.authlib.exceptions.AuthenticationException: Status: {status}"
                ),
            });
        }
        let body = response.text().map_err(unavailable)?;
        if body.is_empty() {
            return Ok(Vec::new());
        }
        let malformed = |e: &dyn std::fmt::Display| {
            format!("com.mojang.authlib.exceptions.AuthenticationException: {e}")
        };
        match serde_json::from_str::<Value>(&body).map_err(|e| malformed(&e))? {
            Value::Null => Ok(Vec::new()),
            Value::Array(profiles) => profiles
                .iter()
                .map(|p| {
                    let id = p
                        .get("id")
                        .and_then(Value::as_str)
                        .and_then(|s| Uuid::parse_str(s).ok());
                    let name = p.get("name").and_then(Value::as_str);
                    id.zip(name)
                        .map(|(id, name)| (id, name.to_owned()))
                        .ok_or_else(|| malformed(&format!("malformed profile entry {p}")))
                })
                .collect(),
            other => Err(malformed(&format!("expected a profile list, got {other}"))),
        }
    }
}

impl ProfileLookup for MojangProfileLookup {
    fn find_profile_by_name(&self, name: &str) -> Option<NameAndId> {
        let url = self
            .endpoint("profiles", "getByName")?
            .replace("{name}", &mc_core::java_case::to_lower_case_root(name));
        match Self::get_json(&url) {
            Ok(Some(body)) => Some(NameAndId::new(
                Uuid::parse_str(body.get("id")?.as_str()?).ok()?,
                body.get("name")?.as_str()?,
            )),
            Ok(None) => None,
            Err(e) => {
                log::warn!("Couldn't find profile with name: {name}\n{e}");
                None
            }
        }
    }

    fn find_profiles_by_names(&self, names: &[String], callback: &mut ProfileLookupCallback) {
        // `Collectors.toSet()`: duplicates go, the order is that of a `HashSet`
        let mut set = JavaHashMap::new();
        for name in names.iter().filter(|n| !n.is_empty()) {
            set.insert(name.clone(), ());
        }
        let criteria: Vec<&str> = set.keys().collect();
        for request in criteria.chunks(ENTRIES_PER_PAGE) {
            let normalized: Vec<String> = request
                .iter()
                .map(|n| mc_core::java_case::to_lower_case_root(n))
                .collect();
            let mut fail_count = 0;
            loop {
                match self.post_names(&normalized) {
                    Ok(results) => {
                        let mut received = HashSet::new();
                        for (id, name) in &results {
                            received.insert(mc_core::java_case::to_lower_case_root(name));
                            if callback(name, Ok(*id)).is_break() {
                                return;
                            }
                        }
                        for name in request {
                            if !received.contains(&mc_core::java_case::to_lower_case_root(name))
                                && callback(name, Err(ProfileLookupError::NotFound)).is_break()
                            {
                                return;
                            }
                        }
                        std::thread::sleep(DELAY_BETWEEN_PAGES);
                        break;
                    }
                    Err(line) => {
                        fail_count += 1;
                        if fail_count == MAX_FAIL_COUNT {
                            for name in request {
                                if callback(name, Err(ProfileLookupError::Backend(line.clone())))
                                    .is_break()
                                {
                                    return;
                                }
                            }
                            break;
                        }
                        std::thread::sleep(DELAY_BETWEEN_FAILURES);
                    }
                }
            }
        }
    }

    fn fetch_profile(&self, id: Uuid) -> Option<GameProfile> {
        let url = self
            .endpoint("session", "getProfileById")?
            .replace("{profileId}", &id.simple().to_string());
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
                            signature: p
                                .get("signature")
                                .and_then(Value::as_str)
                                .map(str::to_owned),
                        })
                    })
                    .collect();
                Some(GameProfile {
                    id: Uuid::parse_str(body.get("id")?.as_str()?).ok()?,
                    name: body.get("name")?.as_str()?.to_owned(),
                    properties,
                })
            }
            Ok(None) => None,
            Err(e) => {
                log::warn!("Couldn't look up profile properties for {id}\n{e}");
                None
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::ops::ControlFlow;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;

    /// A local services endpoint: `/discovery` names `/many` as `getManyByName`; `answer(request body)` is the
    /// `(status, body)` of every POST to `/many`. Returns the lookup and the bodies received so far.
    fn mock(
        answer: impl Fn(&[String]) -> (u16, String) + Send + 'static,
    ) -> (MojangProfileLookup, Arc<Mutex<Vec<Vec<String>>>>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let requests = Arc::new(Mutex::new(Vec::new()));
        let seen = requests.clone();
        let many = format!("{base}/many");
        std::thread::spawn(move || {
            for mut stream in listener.incoming().flatten() {
                let mut data = Vec::new();
                let mut buf = [0u8; 4096];
                let (head_end, length) = loop {
                    let n = stream.read(&mut buf).unwrap();
                    data.extend_from_slice(&buf[..n]);
                    let text = String::from_utf8_lossy(&data).into_owned();
                    if let Some(end) = text.find("\r\n\r\n") {
                        let length = text
                            .lines()
                            .find_map(|l| {
                                l.to_lowercase()
                                    .strip_prefix("content-length: ")?
                                    .parse()
                                    .ok()
                            })
                            .unwrap_or(0usize);
                        break (end + 4, length);
                    }
                };
                while data.len() < head_end + length {
                    let n = stream.read(&mut buf).unwrap();
                    data.extend_from_slice(&buf[..n]);
                }
                let head = String::from_utf8_lossy(&data[..head_end]).into_owned();
                let (status, body) = if head.starts_with("POST /many") {
                    let names: Vec<String> =
                        serde_json::from_slice(&data[head_end..head_end + length]).unwrap();
                    let reply = answer(&names);
                    seen.lock().push(names);
                    reply
                } else {
                    let doc = serde_json::json!({"discovery": {"profiles": {"endpoints": {"getManyByName": {"uri": many}}}}});
                    (200, doc.to_string())
                };
                let _ = write!(
                    stream,
                    "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
            }
        });
        (
            MojangProfileLookup::with_discovery_url(format!("{base}/discovery")),
            requests,
        )
    }

    fn uuid_of(name: &str) -> Uuid {
        mc_core::uuid_util::create_offline_player_uuid(name)
    }

    /// The mock backend knows every name except those starting with `ghost`.
    fn known_profiles(names: &[String]) -> (u16, String) {
        let profiles: Vec<Value> = names
            .iter()
            .filter(|n| !n.starts_with("ghost"))
            .map(|n| serde_json::json!({"id": uuid_of(n).simple().to_string(), "name": n.to_uppercase()}))
            .collect();
        (200, Value::Array(profiles).to_string())
    }

    fn collect(
        lookup: &MojangProfileLookup,
        names: &[String],
    ) -> Vec<(String, Result<Uuid, ProfileLookupError>)> {
        let mut out = Vec::new();
        lookup.find_profiles_by_names(names, &mut |name, result| {
            out.push((name.to_owned(), result));
            ControlFlow::Continue(())
        });
        out
    }

    #[test]
    fn names_are_looked_up_in_lower_case_pages_of_ten() {
        let (lookup, requests) = mock(known_profiles);
        let mut names: Vec<String> = (0..11).map(|i| format!("Player{i}")).collect();
        names.push("Ghost".to_owned());
        names.push("Player3".to_owned()); // duplicate
        names.push(String::new()); // dropped
        let results = collect(&lookup, &names);
        let requests = requests.lock();
        let mut sizes: Vec<usize> = requests.iter().map(Vec::len).collect();
        sizes.sort();
        assert_eq!(sizes, [2, 10]);
        assert!(requests.iter().flatten().all(|n| *n == n.to_lowercase()));
        assert_eq!(results.len(), 12);
        for (name, result) in &results {
            if name == "Ghost" {
                assert_eq!(result, &Err(ProfileLookupError::NotFound));
            } else {
                // the callback gets the name Mojang answered with
                assert_eq!(result, &Ok(uuid_of(&name.to_lowercase())));
                assert!(name.starts_with("PLAYER"), "{name}");
            }
        }
    }

    #[test]
    fn a_failing_page_is_tried_three_times_then_reported_as_a_backend_error() {
        let (lookup, requests) = mock(|_| (500, String::new()));
        let results = collect(&lookup, &["Steve".to_owned(), "Alex".to_owned()]);
        assert_eq!(requests.lock().len(), 3);
        assert_eq!(results.len(), 2);
        for (_, result) in results {
            assert_eq!(
                result,
                Err(ProfileLookupError::Backend(
                    "com.mojang.authlib.exceptions.AuthenticationUnavailableException: Status: 500"
                        .to_owned()
                ))
            );
        }
    }

    #[test]
    fn a_callback_that_stops_the_lookup_prevents_the_next_request() {
        let (lookup, requests) = mock(known_profiles);
        let names: Vec<String> = (0..11).map(|i| format!("Player{i}")).collect();
        let mut calls = 0;
        lookup.find_profiles_by_names(&names, &mut |_, _| {
            calls += 1;
            ControlFlow::Break(())
        });
        assert_eq!((calls, requests.lock().len()), (1, 1));
    }

    #[test]
    fn a_page_that_fails_twice_and_then_works_is_answered() {
        let attempts = AtomicUsize::new(0);
        let (lookup, requests) = mock(move |names| {
            if attempts.fetch_add(1, Ordering::SeqCst) < 2 {
                (503, String::new())
            } else {
                known_profiles(names)
            }
        });
        let results = collect(&lookup, &["Steve".to_owned(), "Ghost".to_owned()]);
        assert_eq!(requests.lock().len(), 3);
        assert_eq!(results.len(), 2);
        for (name, result) in results {
            match name.as_str() {
                "Ghost" => assert_eq!(result, Err(ProfileLookupError::NotFound)),
                _ => assert_eq!(result, Ok(uuid_of("steve"))),
            }
        }
    }
}
