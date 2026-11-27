//! `net.minecraft.server.network.LegacyTextFilter`: text filtering version 0 (`text-filtering-config` with `apiServer`
//! and `apiKey`).

use super::server_text_filter::{
    FilterBackend, FilterError, IgnoreStrategy, JoinLeave, MessageEncoder, ProfileEncoder,
    ServerTextFilter, parse_mask,
};
use super::text_filter_json::{
    get_as_array, get_as_bool_or, get_as_int_or, get_as_java_string_or, get_as_object_opt,
    get_as_string, get_as_string_or, syntax,
};
use crate::server::chat::FilteredText;
use base64::Engine as _;
use mc_core::gson::{JsonElement, JsonError, JsonObject};
use mc_core::java_exception::JavaException;
use mc_core::java_string::JavaString;
use mc_protocol::codec::GameProfile;

const ENDPOINT: &str = "v1/chat";

pub struct LegacyTextFilter {
    join_leave: JoinLeave,
    auth_key: String,
}

fn string_property(object: &mut JsonObject, key: &str, value: impl Into<JavaString>) {
    object.add(key, JsonElement::String(value.into()));
}

/// `host.resolve("/" + endpoint).toURL()`: the host's scheme and authority with the endpoint as the path.
fn endpoint_url(host: &reqwest::Url, endpoint: &str) -> Result<reqwest::Url, FilterError> {
    host.join(&format!("/{endpoint}"))
        .map_err(|e| JavaException::unchecked("java.lang.IllegalArgumentException", e.to_string()))
}

fn endpoint_from_config(
    source: Option<&JsonObject>,
    id: &str,
    default: &str,
) -> Result<String, FilterError> {
    match source {
        Some(source) => get_as_string_or(source, id, default),
        None => Ok(default.to_owned()),
    }
}

impl LegacyTextFilter {
    /// `createTextFilterFromConfig`: a config that does not parse is a warning and no filter.
    pub fn create_text_filter_from_config(config: &JavaString) -> Option<ServerTextFilter> {
        match Self::parse(config) {
            Ok(filter) => Some(filter),
            Err(e) => {
                log::warn!(
                    "Failed to parse chat filter config {}\n{e}",
                    config.as_lossy_str()
                );
                None
            }
        }
    }

    fn parse(config: &JavaString) -> Result<ServerTextFilter, FilterError> {
        let parsed = parse_config_object(config)?;
        let host = uri(&get_as_string(&parsed, "apiServer")?)?;
        let key = get_as_string(&parsed, "apiKey")?;
        if key.is_empty() {
            return Err(JavaException::unchecked(
                "java.lang.IllegalArgumentException",
                "Missing API key",
            ));
        }
        let rule_id = get_as_int_or(&parsed, "ruleId", 1)?;
        let server_id = get_as_java_string_or(&parsed, "serverId", "")?;
        let room_id = get_as_java_string_or(&parsed, "roomId", "Java:Chat")?;
        let hashes_to_drop = get_as_int_or(&parsed, "hashesToDrop", -1)?;
        let max_concurrent_requests = get_as_int_or(&parsed, "maxConcurrentRequests", 7)?;
        let endpoints = get_as_object_opt(&parsed, "endpoints")?;
        let chat_endpoint_config = endpoint_from_config(endpoints, "chat", ENDPOINT)?;
        let is_legacy_chat_endpoint = chat_endpoint_config == ENDPOINT;
        let chat_endpoint = endpoint_url(&host, &chat_endpoint_config)?;
        let join_endpoint =
            endpoint_url(&host, &endpoint_from_config(endpoints, "join", "v1/join")?)?;
        let leave_endpoint = endpoint_url(
            &host,
            &endpoint_from_config(endpoints, "leave", "v1/leave")?,
        )?;
        let join_or_leave_server = server_id.clone();
        let join_or_leave_room = room_id.clone();
        let join_or_leave: ProfileEncoder = Box::new(move |user: &GameProfile| {
            let mut object = JsonObject::new();
            string_property(&mut object, "server", &join_or_leave_server);
            string_property(&mut object, "room", &join_or_leave_room);
            string_property(&mut object, "user_id", user.id.to_string());
            string_property(&mut object, "user_display_name", &user.name);
            object
        });
        let chat_encoder: MessageEncoder = if is_legacy_chat_endpoint {
            Box::new(move |sender: &GameProfile, message: &str| {
                let mut object = JsonObject::new();
                object.add_property_int("rule", i64::from(rule_id));
                string_property(&mut object, "server", &server_id);
                string_property(&mut object, "room", &room_id);
                string_property(&mut object, "player", sender.id.to_string());
                string_property(&mut object, "player_display_name", &sender.name);
                string_property(&mut object, "text", message);
                string_property(&mut object, "language", "*");
                object
            })
        } else {
            let rule_id = rule_id.to_string();
            Box::new(move |sender: &GameProfile, message: &str| {
                let mut object = JsonObject::new();
                string_property(&mut object, "rule_id", &rule_id);
                string_property(&mut object, "category", &server_id);
                string_property(&mut object, "subcategory", &room_id);
                string_property(&mut object, "user_id", sender.id.to_string());
                string_property(&mut object, "user_display_name", &sender.name);
                string_property(&mut object, "text", message);
                string_property(&mut object, "language", "*");
                object
            })
        };
        let strategy = IgnoreStrategy::select(hashes_to_drop);
        let pool = ServerTextFilter::create_worker_pool(max_concurrent_requests)?;
        // `key.getBytes(US_ASCII)`: a character outside ASCII becomes `?`
        let ascii: Vec<u8> = key
            .chars()
            .map(|c| if c.is_ascii() { c as u8 } else { b'?' })
            .collect();
        let backend = LegacyTextFilter {
            join_leave: JoinLeave {
                join: join_endpoint,
                leave: leave_endpoint,
                encoder: join_or_leave,
            },
            auth_key: base64::engine::general_purpose::STANDARD.encode(ascii),
        };
        Ok(ServerTextFilter::new(
            chat_endpoint,
            chat_encoder,
            strategy,
            pool,
            Box::new(backend),
        ))
    }
}

/// `GsonHelper.parse(config)`: one value read STRICT as a `JsonObject`; trailing data is ignored.
pub(super) fn parse_config_object(config: &JavaString) -> Result<JsonObject, FilterError> {
    let element = mc_core::gson::parse(config, false)
        .map_err(|JsonError(message)| {
            JavaException::unchecked("com.google.gson.JsonParseException", message)
        })?
        .ok_or_else(|| {
            JavaException::unchecked(
                "com.google.gson.JsonParseException",
                "JSON data was null or empty",
            )
        })?;
    match element {
        JsonElement::Object(object) => Ok(object),
        other => Err(syntax(format!(
            "Expected a com.google.gson.JsonObject but was {}; at path $",
            gson_class(&other)
        ))),
    }
}

fn gson_class(element: &JsonElement) -> &'static str {
    match element {
        JsonElement::Null => "com.google.gson.JsonNull",
        JsonElement::Array(_) => "com.google.gson.JsonArray",
        JsonElement::Object(_) => "com.google.gson.JsonObject",
        _ => "com.google.gson.JsonPrimitive",
    }
}

/// `new URI(string)`.
pub(super) fn uri(text: &str) -> Result<reqwest::Url, FilterError> {
    reqwest::Url::parse(text)
        .map_err(|e| JavaException::unchecked("java.lang.IllegalArgumentException", e.to_string()))
}

impl FilterBackend for LegacyTextFilter {
    fn authorization(&self) -> Result<String, FilterError> {
        Ok(format!("Basic {}", self.auth_key))
    }

    fn filter_text(
        &self,
        message: &str,
        strategy: IgnoreStrategy,
        result: &JsonObject,
    ) -> Result<FilteredText, FilterError> {
        if get_as_bool_or(result, "response", false)? {
            return Ok(FilteredText::pass_through(message));
        }
        if result.get("hashed").is_none() {
            return Ok(FilteredText::fully_filtered(message));
        }
        get_as_string(result, "hashed")?;
        let removed = get_as_array(result, "hashes")?;
        let mask = parse_mask(message, removed, strategy)?;
        Ok(FilteredText {
            raw: message.to_owned(),
            mask,
        })
    }

    fn join_leave(&self) -> Option<&JoinLeave> {
        Some(&self.join_leave)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::server::chat::filter_mask::FilterMaskExt;
    use mc_protocol::packets::play::FilterMask;
    use std::io::{BufRead, BufReader, Read, Write};
    use std::net::TcpListener;
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant};

    /// Answers every request with the next of `answers` (status line + body) and records `target\nbody`.
    fn mock(answers: Vec<(&'static str, &'static str)>) -> (String, Arc<Mutex<Vec<String>>>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let seen = Arc::new(Mutex::new(Vec::new()));
        let record = seen.clone();
        std::thread::spawn(move || {
            let mut answers = answers.into_iter();
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { return };
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut target = String::new();
                reader.read_line(&mut target).unwrap();
                let mut length = 0;
                loop {
                    let mut line = String::new();
                    reader.read_line(&mut line).unwrap();
                    if line.trim().is_empty() {
                        break;
                    }
                    if let Some(v) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                        length = v.trim().parse().unwrap();
                    }
                }
                let mut body = vec![0; length];
                reader.read_exact(&mut body).unwrap();
                record.lock().unwrap().push(format!(
                    "{}\n{}",
                    target.split_whitespace().nth(1).unwrap(),
                    String::from_utf8(body).unwrap()
                ));
                let (status, body) = answers.next().unwrap_or(("200 OK", "{}"));
                let _ = write!(
                    stream,
                    "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
            }
        });
        (url, seen)
    }

    fn profile() -> GameProfile {
        GameProfile {
            id: uuid::Uuid::from_u128(1),
            name: "Steve".into(),
            properties: vec![],
        }
    }

    fn answer_of(
        text: &str,
        answers: Vec<(&'static str, &'static str)>,
        extra: &str,
    ) -> (FilteredText, Vec<String>) {
        let (url, seen) = mock(answers);
        let config = format!(r#"{{"apiServer":"{url}","apiKey":"k"{extra}}}"#);
        let filter =
            Arc::new(LegacyTextFilter::create_text_filter_from_config(&config.into()).unwrap());
        let future = filter
            .create_context(profile())
            .process_stream_message(text);
        let start = Instant::now();
        while !future.is_done() {
            assert!(start.elapsed() < Duration::from_secs(10), "no answer");
            std::thread::sleep(Duration::from_millis(10));
        }
        filter.close();
        let seen = seen.lock().unwrap().clone();
        (future.get().unwrap(), seen)
    }

    #[test]
    fn chat_request_and_the_answers_of_the_service() {
        let (text, seen) = answer_of("hello", vec![("200 OK", r#"{"response":true}"#)], "");
        assert_eq!(text, FilteredText::pass_through("hello"));
        assert_eq!(
            seen,
            [r#"/v1/chat
{"rule":1,"server":"","room":"Java:Chat","player":"00000000-0000-0000-0000-000000000001","player_display_name":"Steve","text":"hello","language":"*"}"#]
        );
        let (text, _) = answer_of(
            "hello",
            vec![(
                "200 OK",
                r#"{"response":false,"hashed":"h##lo","hashes":[1,2]}"#,
            )],
            "",
        );
        let mut mask = FilterMask::partial();
        mask.set_filtered(1);
        mask.set_filtered(2);
        assert_eq!(text.mask, mask);
        // no `hashed`, a 204, a failure status, a bad index: fully filtered
        for answer in [
            ("200 OK", r#"{"response":false}"#),
            ("204 No Content", ""),
            ("500 Internal Server Error", "{}"),
            ("200 OK", r#"{"response":false,"hashed":"x","hashes":[-1]}"#),
            ("200 OK", "[1]"),
        ] {
            assert_eq!(
                answer_of("hello", vec![answer], "").0,
                FilteredText::fully_filtered("hello"),
                "{answer:?}"
            );
        }
        // the ignore strategies: all characters hashed, and a threshold
        let all = (
            "200 OK",
            r#"{"response":false,"hashed":"x","hashes":[0,1]}"#,
        );
        assert_eq!(
            answer_of("ab", vec![all], r#","hashesToDrop":0"#).0.mask,
            FilterMask::FullyFiltered
        );
        assert_eq!(
            answer_of("abcd", vec![all], r#","hashesToDrop":2"#).0.mask,
            FilterMask::FullyFiltered
        );
        assert_ne!(
            answer_of("abcd", vec![all], r#","hashesToDrop":3"#).0.mask,
            FilterMask::FullyFiltered
        );
    }

    #[test]
    fn empty_message_asks_nobody_and_join_leave_use_their_endpoints() {
        let (url, seen) = mock(vec![]);
        let config = format!(
            r#"{{"apiServer":"{url}","apiKey":"k","serverId":"s","roomId":"r","endpoints":{{"join":"in"}}}}"#
        );
        let filter =
            Arc::new(LegacyTextFilter::create_text_filter_from_config(&config.into()).unwrap());
        let context = filter.create_context(profile());
        assert_eq!(
            context.process_stream_message("").get(),
            Some(FilteredText::empty())
        );
        context.join();
        context.leave();
        let start = Instant::now();
        while seen.lock().unwrap().len() < 2 {
            assert!(start.elapsed() < Duration::from_secs(10), "no requests");
            std::thread::sleep(Duration::from_millis(10));
        }
        filter.close();
        let seen = seen.lock().unwrap().clone();
        let body = r#"{"server":"s","room":"r","user_id":"00000000-0000-0000-0000-000000000001","user_display_name":"Steve"}"#;
        assert_eq!(seen, [format!("/in\n{body}"), format!("/v1/leave\n{body}")]);
    }

    #[test]
    fn unusable_configurations_are_refused() {
        for config in [
            "not json",
            "[]",
            r#"{"apiKey":"k"}"#,
            r#"{"apiServer":"http://x"}"#,
            r#"{"apiServer":"http://x","apiKey":""}"#,
            r#"{"apiServer":"http://x","apiKey":"k","hashesToDrop":"x"}"#,
            r#"{"apiServer":"http://x","apiKey":"k","maxConcurrentRequests":0}"#,
        ] {
            assert!(
                LegacyTextFilter::create_text_filter_from_config(&config.into()).is_none(),
                "{config}"
            );
        }
        assert_eq!(
            LegacyTextFilter::parse(&r#"{"apiServer":"http://x","apiKey":""}"#.into())
                .err()
                .unwrap()
                .to_string(),
            "java.lang.IllegalArgumentException: Missing API key"
        );
    }

    /// A `serverId` and `roomId` that hold lone surrogates (raw in the config or as a JSON escape) reach the request body
    /// as vanilla writes it: each lone surrogate is `?`.
    #[test]
    fn lone_surrogates_in_the_config_reach_the_request_body_like_vanilla() {
        let mut n = 0;
        for c in mc_testkit::rows(
            include_str!("../../../tests/fixtures/lone_surrogate.tsv"),
            "text_filter",
        ) {
            let (url, seen) = mock(vec![]);
            let value = JavaString::from_utf16_units(&mc_testkit::unesc_units(c[1]));
            let mut config =
                JavaString::from(format!(r#"{{"apiServer":"{url}","apiKey":"k","serverId":"#));
            config.push_java(&value);
            config.push_str(r#","roomId":"#);
            config.push_java(&value);
            config.push('}');
            let filter =
                Arc::new(LegacyTextFilter::create_text_filter_from_config(&config).unwrap());
            let future = filter.create_context(profile()).process_stream_message("m");
            let start = Instant::now();
            while !future.is_done() {
                assert!(start.elapsed() < Duration::from_secs(10), "no answer");
                std::thread::sleep(Duration::from_millis(10));
            }
            filter.close();
            let seen = seen.lock().unwrap().clone();
            let body = seen[0].split_once('\n').unwrap().1;
            assert_eq!(mc_testkit::hex(body.as_bytes()), c[2], "config {}", c[1]);
            n += 1;
        }
        assert_eq!(n, 9);
    }
}
