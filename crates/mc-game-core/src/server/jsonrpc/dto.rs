//! The JSON shapes of the management protocol (vanilla's `PlayerDto`, `UserBanDto`, `OperatorDto`, ...) with their
//! codecs. Decoding failures carry DFU-style messages (they are returned as the `data` of `Invalid params`).

use chrono::{DateTime, SecondsFormat, Utc};
use mc_text::Component;
use mc_text::component::TranslatableArg;
use uuid::Uuid;

use super::api::{GameRuleInfo, GameRuleValue, ServerVersion};
use crate::server::permissions::PermissionLevel;
use crate::server::players::NameAndId;
use crate::server::players::ServerOpListEntry;
use crate::server::players::StoredUserEntry;
use crate::server::players::ban_list_entry::{Date, IpBanListEntry, UserBanListEntry};
use crate::server::players::gson::{JsonElement, JsonObject};

/// A decoding failure (`DataResult` error message).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecodeError(pub String);

pub type Decoded<T> = Result<T, DecodeError>;

fn err<T>(msg: String) -> Decoded<T> {
    Err(DecodeError(msg))
}

fn not(kind: &str, e: &JsonElement) -> DecodeError {
    DecodeError(format!("Not a {kind}: {}", e.to_compact_string()))
}

// ---- primitive codecs ----------------------------------------------------------------------------------------

pub fn decode_string(e: &JsonElement) -> Decoded<String> {
    match e {
        JsonElement::String(s) => Ok(s.clone()),
        _ => Err(not("string", e)),
    }
}

pub fn decode_bool(e: &JsonElement) -> Decoded<bool> {
    match e {
        JsonElement::Bool(b) => Ok(*b),
        JsonElement::Number(n) => Ok(number_as_i32(n) as i8 != 0),
        _ => Err(not("boolean", e)),
    }
}

fn number_as_i32(text: &str) -> i32 {
    text.parse::<i32>()
        .or_else(|_| text.parse::<i64>().map(|v| v as i32))
        .unwrap_or_else(|_| text.parse::<f64>().map(|v| v as i32).unwrap_or(0))
}

/// `Codec.INT` (`Number.intValue`).
pub fn decode_int(e: &JsonElement) -> Decoded<i32> {
    match e {
        JsonElement::Number(n) => Ok(number_as_i32(n)),
        _ => Err(not("number", e)),
    }
}

pub fn decode_list<'a>(e: &'a JsonElement) -> Decoded<&'a [JsonElement]> {
    match e {
        JsonElement::Array(items) => Ok(items),
        _ => Err(not("list", e)),
    }
}

pub fn decode_map(e: &JsonElement) -> Decoded<&JsonObject> {
    match e {
        JsonElement::Object(o) => Ok(o),
        _ => Err(not("map", e)),
    }
}

fn required<'a>(map: &'a JsonObject, key: &str) -> Decoded<&'a JsonElement> {
    match map.get(key) {
        Some(v) => Ok(v),
        None => err(format!(
            "No key {key} in MapLike[{}]",
            JsonElement::Object(map.clone()).to_compact_string()
        )),
    }
}

/// `optionalFieldOf`: absent is `None`, present-but-invalid is an error.
fn optional<T>(
    map: &JsonObject,
    key: &str,
    decode: impl FnOnce(&JsonElement) -> Decoded<T>,
) -> Decoded<Option<T>> {
    map.get(key).map(decode).transpose()
}

pub fn decode_list_of<T>(
    e: &JsonElement,
    decode: impl Fn(&JsonElement) -> Decoded<T>,
) -> Decoded<Vec<T>> {
    decode_list(e)?.iter().map(decode).collect()
}

fn decode_uuid(e: &JsonElement) -> Decoded<Uuid> {
    let s = decode_string(e)?;
    mc_core::uuid_util::uuid_from_string(&s)
        .map_err(|m| DecodeError(format!("Invalid UUID {s}: {m}")))
}

fn decode_instant(e: &JsonElement) -> Decoded<Date> {
    let s = decode_string(e)?;
    DateTime::parse_from_rfc3339(&s)
        .map(|d| d.with_timezone(&Utc))
        .map_err(|_| DecodeError(format!("Text '{s}' could not be parsed at index 0")))
}

/// `DateTimeFormatter.ISO_INSTANT.format`: seconds precision, a 3/6/9 digit fraction only when non-zero.
pub fn encode_instant(d: &Date) -> JsonElement {
    let format = match d.timestamp_subsec_nanos() {
        0 => SecondsFormat::Secs,
        n if n % 1_000_000 == 0 => SecondsFormat::Millis,
        n if n % 1_000 == 0 => SecondsFormat::Micros,
        _ => SecondsFormat::Nanos,
    };
    JsonElement::String(d.to_rfc3339_opts(format, true))
}

fn obj(members: Vec<(&str, Option<JsonElement>)>) -> JsonElement {
    let mut o = JsonObject::new();
    for (k, v) in members {
        if let Some(v) = v {
            o.add(k, v);
        }
    }
    JsonElement::Object(o)
}

fn str_e(s: &str) -> JsonElement {
    JsonElement::String(s.to_owned())
}

fn num(n: i64) -> JsonElement {
    JsonElement::Number(n.to_string())
}

// ---- PlayerDto -------------------------------------------------------------------------------------------------

/// `PlayerDto`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlayerDto {
    pub id: Option<Uuid>,
    pub name: Option<String>,
}

impl PlayerDto {
    pub fn from_name_and_id(p: &NameAndId) -> PlayerDto {
        PlayerDto {
            id: Some(p.id),
            name: Some(p.name.clone()),
        }
    }

    pub fn decode(e: &JsonElement) -> Decoded<PlayerDto> {
        let m = decode_map(e)?;
        Ok(PlayerDto {
            id: optional(m, "id", decode_uuid)?,
            name: optional(m, "name", decode_string)?,
        })
    }

    pub fn encode(&self) -> JsonElement {
        obj(vec![
            ("id", self.id.map(|i| str_e(&i.to_string()))),
            ("name", self.name.as_deref().map(str_e)),
        ])
    }
}

// ---- Message ---------------------------------------------------------------------------------------------------

/// `methods.Message`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Message {
    pub literal: Option<String>,
    pub translatable: Option<String>,
    pub translatable_params: Option<Vec<String>>,
}

impl Message {
    pub fn decode(e: &JsonElement) -> Decoded<Message> {
        let m = decode_map(e)?;
        Ok(Message {
            literal: optional(m, "literal", decode_string)?,
            translatable: optional(m, "translatable", decode_string)?,
            // lenientOptionalFieldOf: an invalid list is treated as absent
            translatable_params: m
                .get("translatableParams")
                .and_then(|p| decode_list_of(p, decode_string).ok()),
        })
    }

    /// `asComponent`.
    pub fn as_component(&self) -> Option<Component> {
        if let Some(key) = &self.translatable {
            let args: Vec<TranslatableArg> = self
                .translatable_params
                .iter()
                .flatten()
                .map(|a| TranslatableArg::String(a.clone()))
                .collect();
            return Some(Component::translatable(key.clone(), args));
        }
        self.literal.as_ref().map(|l| Component::literal(l.clone()))
    }
}

// ---- bans ------------------------------------------------------------------------------------------------------

pub const BAN_SOURCE: &str = "Management server";

/// `BanlistService.UserBanDto`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UserBanDto {
    pub player: PlayerDto,
    pub reason: Option<String>,
    pub source: Option<String>,
    pub expires: Option<Date>,
}

impl UserBanDto {
    pub fn decode(e: &JsonElement) -> Decoded<UserBanDto> {
        let m = decode_map(e)?;
        Ok(UserBanDto {
            player: PlayerDto::decode(required(m, "player")?)?,
            reason: optional(m, "reason", decode_string)?,
            source: optional(m, "source", decode_string)?,
            expires: optional(m, "expires", decode_instant)?,
        })
    }

    pub fn encode(&self) -> JsonElement {
        obj(vec![
            ("player", Some(self.player.encode())),
            ("reason", self.reason.as_deref().map(str_e)),
            ("source", self.source.as_deref().map(str_e)),
            ("expires", self.expires.as_ref().map(encode_instant)),
        ])
    }

    pub fn from_entry(entry: &UserBanListEntry) -> Option<UserBanDto> {
        let user = entry.user()?;
        Some(UserBanDto {
            player: PlayerDto::from_name_and_id(user),
            reason: entry.reason.clone(),
            source: Some(entry.source.clone()),
            expires: entry.expires,
        })
    }
}

/// `BanlistService.UserBan`: a resolved ban (equality is by value, as the Java record's).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UserBan {
    pub player: NameAndId,
    pub reason: Option<String>,
    pub source: String,
    pub expires: Option<Date>,
}

impl UserBan {
    pub fn from_entry(entry: &UserBanListEntry) -> Option<UserBan> {
        Some(UserBan {
            player: entry.user()?.clone(),
            reason: entry.reason.clone(),
            source: entry.source.clone(),
            expires: entry.expires,
        })
    }

    pub fn from_dto(dto: &UserBanDto, user: NameAndId) -> UserBan {
        UserBan {
            player: user,
            reason: dto.reason.clone(),
            source: dto.source.clone().unwrap_or_else(|| BAN_SOURCE.to_owned()),
            expires: dto.expires,
        }
    }

    pub fn to_entry(&self) -> UserBanListEntry {
        UserBanListEntry::new(
            Some(self.player.clone()),
            None,
            Some(&self.source),
            self.expires,
            self.reason.as_deref(),
        )
    }
}

/// `IpBanlistService.IpBanDto`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IpBanDto {
    pub ip: String,
    pub reason: Option<String>,
    pub source: Option<String>,
    pub expires: Option<Date>,
}

impl IpBanDto {
    pub fn decode(e: &JsonElement) -> Decoded<IpBanDto> {
        let m = decode_map(e)?;
        Ok(IpBanDto {
            ip: decode_string(required(m, "ip")?)?,
            reason: optional(m, "reason", decode_string)?,
            source: optional(m, "source", decode_string)?,
            expires: optional(m, "expires", decode_instant)?,
        })
    }

    pub fn encode(&self) -> JsonElement {
        obj(vec![
            ("ip", Some(str_e(&self.ip))),
            ("reason", self.reason.as_deref().map(str_e)),
            ("source", self.source.as_deref().map(str_e)),
            ("expires", self.expires.as_ref().map(encode_instant)),
        ])
    }

    pub fn from_entry(entry: &IpBanListEntry) -> Option<IpBanDto> {
        Some(IpBanDto {
            ip: entry.user()?.clone(),
            reason: entry.reason.clone(),
            source: Some(entry.source.clone()),
            expires: entry.expires,
        })
    }
}

/// `IpBanlistService.IpBan`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IpBan {
    pub ip: String,
    pub reason: Option<String>,
    pub source: String,
    pub expires: Option<Date>,
}

impl IpBan {
    pub fn from_entry(entry: &IpBanListEntry) -> Option<IpBan> {
        Some(IpBan {
            ip: entry.user()?.clone(),
            reason: entry.reason.clone(),
            source: entry.source.clone(),
            expires: entry.expires,
        })
    }

    pub fn from_dto(dto: &IpBanDto) -> IpBan {
        IpBan {
            ip: dto.ip.clone(),
            reason: dto.reason.clone(),
            source: dto.source.clone().unwrap_or_else(|| BAN_SOURCE.to_owned()),
            expires: dto.expires,
        }
    }

    pub fn to_entry(&self) -> IpBanListEntry {
        IpBanListEntry::new(
            Some(self.ip.clone()),
            None,
            Some(&self.source),
            self.expires,
            self.reason.as_deref(),
        )
    }
}

/// `IpBanlistService.IncomingIpBanDto`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IncomingIpBanDto {
    pub player: Option<PlayerDto>,
    pub ip: Option<String>,
    pub reason: Option<String>,
    pub source: Option<String>,
    pub expires: Option<Date>,
}

impl IncomingIpBanDto {
    pub fn decode(e: &JsonElement) -> Decoded<IncomingIpBanDto> {
        let m = decode_map(e)?;
        Ok(IncomingIpBanDto {
            player: optional(m, "player", PlayerDto::decode)?,
            ip: optional(m, "ip", decode_string)?,
            reason: optional(m, "reason", decode_string)?,
            source: optional(m, "source", decode_string)?,
            expires: optional(m, "expires", decode_instant)?,
        })
    }

    /// `toIpBan()` / `toIpBan(player)` once the address is known.
    pub fn to_ip_ban(&self, ip: String) -> IpBan {
        IpBan {
            ip,
            reason: self.reason.clone(),
            source: self.source.clone().unwrap_or_else(|| BAN_SOURCE.to_owned()),
            expires: self.expires,
        }
    }
}

// ---- operators -------------------------------------------------------------------------------------------------

/// `OperatorService.OperatorDto`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OperatorDto {
    pub player: PlayerDto,
    pub permission_level: Option<PermissionLevel>,
    pub bypasses_player_limit: Option<bool>,
}

impl OperatorDto {
    pub fn decode(e: &JsonElement) -> Decoded<OperatorDto> {
        let m = decode_map(e)?;
        Ok(OperatorDto {
            player: PlayerDto::decode(required(m, "player")?)?,
            permission_level: optional(m, "permissionLevel", |v| {
                decode_int(v).map(PermissionLevel::by_id)
            })?,
            bypasses_player_limit: optional(m, "bypassesPlayerLimit", decode_bool)?,
        })
    }

    pub fn encode(&self) -> JsonElement {
        obj(vec![
            ("player", Some(self.player.encode())),
            (
                "permissionLevel",
                self.permission_level.map(|l| num(l.id() as i64)),
            ),
            (
                "bypassesPlayerLimit",
                self.bypasses_player_limit.map(JsonElement::Bool),
            ),
        ])
    }

    pub fn from_entry(entry: &ServerOpListEntry) -> Option<OperatorDto> {
        Some(OperatorDto {
            player: PlayerDto::from_name_and_id(entry.user()?),
            permission_level: Some(entry.permissions().level()),
            bypasses_player_limit: Some(entry.get_bypasses_player_limit()),
        })
    }
}

// ---- players / state / game rules ---------------------------------------------------------------------------------

/// `PlayerService.KickDto`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KickDto {
    pub player: PlayerDto,
    pub message: Option<Message>,
}

impl KickDto {
    pub fn decode(e: &JsonElement) -> Decoded<KickDto> {
        let m = decode_map(e)?;
        Ok(KickDto {
            player: PlayerDto::decode(required(m, "player")?)?,
            message: optional(m, "message", Message::decode)?,
        })
    }
}

/// `ServerStateService.SystemMessage`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SystemMessage {
    pub message: Message,
    pub overlay: bool,
    pub receiving_players: Option<Vec<PlayerDto>>,
}

impl SystemMessage {
    pub fn decode(e: &JsonElement) -> Decoded<SystemMessage> {
        let m = decode_map(e)?;
        Ok(SystemMessage {
            message: Message::decode(required(m, "message")?)?,
            overlay: decode_bool(required(m, "overlay")?)?,
            // lenientOptionalFieldOf: an invalid list is treated as absent
            receiving_players: m
                .get("receivingPlayers")
                .and_then(|p| decode_list_of(p, PlayerDto::decode).ok()),
        })
    }
}

/// `ServerStateService.ServerState.CODEC` (an empty `players` list is omitted, being the field default).
pub fn encode_server_state(
    started: bool,
    players: &[PlayerDto],
    version: &ServerVersion,
) -> JsonElement {
    obj(vec![
        ("started", Some(JsonElement::Bool(started))),
        (
            "players",
            (!players.is_empty())
                .then(|| JsonElement::Array(players.iter().map(PlayerDto::encode).collect())),
        ),
        (
            "version",
            Some(obj(vec![
                ("name", Some(str_e(&version.name))),
                ("protocol", Some(num(version.protocol as i64))),
            ])),
        ),
    ])
}

pub fn encode_game_rule_value(v: GameRuleValue) -> JsonElement {
    match v {
        GameRuleValue::Bool(b) => JsonElement::Bool(b),
        GameRuleValue::Int(i) => num(i as i64),
    }
}

/// `GameRuleUpdate.TYPED_CODEC`: `key`, `type`, `value`.
pub fn encode_typed_game_rule(rule: &GameRuleInfo) -> JsonElement {
    obj(vec![
        ("key", Some(str_e(&rule.id))),
        ("type", Some(str_e(rule.rule_type.serialized_name()))),
        ("value", Some(encode_game_rule_value(rule.value))),
    ])
}

/// The `{key, value}` object of `gamerules/update`: the key text and the raw JSON value (typed by the server).
pub fn decode_untyped_game_rule(e: &JsonElement) -> Decoded<(String, GameRuleValue)> {
    let m = decode_map(e)?;
    let key = decode_string(required(m, "key")?)?;
    let value = match required(m, "value")? {
        JsonElement::Bool(b) => GameRuleValue::Bool(*b),
        JsonElement::Number(_) => GameRuleValue::Int(decode_int(required(m, "value")?)?),
        other => {
            return err(format!(
                "Not a boolean or number: {}",
                other.to_compact_string()
            ));
        }
    };
    Ok((key, value))
}

pub fn encode_player_list(players: &[NameAndId]) -> JsonElement {
    JsonElement::Array(
        players
            .iter()
            .map(|p| PlayerDto::from_name_and_id(p).encode())
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::server::players::gson;

    fn parse(s: &str) -> JsonElement {
        gson::parse(s, true).unwrap().unwrap()
    }

    #[test]
    fn player_dto_roundtrip_and_errors() {
        let p = PlayerDto::decode(&parse(
            "{\"id\":\"069a79f4-44e9-4726-a5be-fca90e38aaf5\",\"name\":\"Notch\"}",
        ))
        .unwrap();
        assert_eq!(
            p.encode().to_compact_string(),
            "{\"id\":\"069a79f4-44e9-4726-a5be-fca90e38aaf5\",\"name\":\"Notch\"}"
        );
        assert_eq!(
            PlayerDto::decode(&parse("{}")).unwrap(),
            PlayerDto {
                id: None,
                name: None
            }
        );
        assert_eq!(
            PlayerDto::decode(&parse("5")).unwrap_err().0,
            "Not a map: 5"
        );
        assert!(
            PlayerDto::decode(&parse("{\"id\":\"zz\"}"))
                .unwrap_err()
                .0
                .starts_with("Invalid UUID zz: ")
        );
        assert_eq!(
            PlayerDto::decode(&parse("{\"name\":4}")).unwrap_err().0,
            "Not a string: 4"
        );
    }

    #[test]
    fn ban_dto_and_instants() {
        let b = UserBanDto::decode(&parse("{\"player\":{\"name\":\"x\"},\"expires\":\"2030-01-02T03:04:05.250Z\",\"reason\":\"r\"}")).unwrap();
        assert_eq!(
            b.encode().to_compact_string(),
            "{\"player\":{\"name\":\"x\"},\"reason\":\"r\",\"expires\":\"2030-01-02T03:04:05.250Z\"}"
        );
        assert_eq!(
            UserBanDto::decode(&parse("{}")).unwrap_err().0,
            "No key player in MapLike[{}]"
        );
        assert_eq!(
            encode_instant(&parse_instant("2030-01-02T03:04:05Z")).to_compact_string(),
            "\"2030-01-02T03:04:05Z\""
        );
    }

    fn parse_instant(s: &str) -> Date {
        decode_instant(&JsonElement::String(s.to_owned())).unwrap()
    }

    #[test]
    fn operator_and_state_encoding() {
        let o = OperatorDto::decode(&parse(
            "{\"player\":{\"name\":\"a\"},\"permissionLevel\":9}",
        ))
        .unwrap();
        assert_eq!(o.permission_level, Some(PermissionLevel::Owners));
        assert_eq!(
            o.encode().to_compact_string(),
            "{\"player\":{\"name\":\"a\"},\"permissionLevel\":4}"
        );
        let v = ServerVersion {
            name: "26.3".into(),
            protocol: 777,
        };
        assert_eq!(
            encode_server_state(false, &[], &v).to_compact_string(),
            "{\"started\":false,\"version\":{\"name\":\"26.3\",\"protocol\":777}}"
        );
    }

    #[test]
    fn message_to_component() {
        let m = Message::decode(&parse("{\"translatable\":\"chat.type.text\",\"translatableParams\":[\"a\"],\"literal\":\"ignored\"}")).unwrap();
        assert!(m.as_component().is_some());
        assert!(
            Message::decode(&parse("{}"))
                .unwrap()
                .as_component()
                .is_none()
        );
    }
}
