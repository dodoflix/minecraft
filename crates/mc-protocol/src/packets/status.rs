//! `status` state: server list ping.
use mc_core::math::java::java_double_to_string;
use mc_core::uuid_util::uuid_from_string;
use mc_text::Component;
use serde_json::{Map, Number, Value};
use uuid::Uuid;

use crate::codec::{read_utf, write_utf, DecodeError, DecodeResult};
use crate::gson::{self, JsonElement, JsonObject};
use crate::{packet_enum, Decode, Encode, Packet};

const FAVICON_PREFIX: &str = "data:image/png;base64,";

/// `ServerStatus.Players`
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Players {
    pub max: i32,
    pub online: i32,
    /// `NameAndId` entries (`{"id": "<dashed uuid>", "name": ..}`)
    pub sample: Vec<(Uuid, String)>,
}

/// `ServerStatus.Version`
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Version {
    pub name: String,
    pub protocol: i32,
}

/// `ServerStatus`: the JSON body of the status response.
#[derive(Debug, Clone, PartialEq)]
pub struct ServerStatus {
    pub description: Component,
    pub players: Option<Players>,
    pub version: Option<Version>,
    /// Raw PNG bytes (`ServerStatus.Favicon`), sent as a `data:image/png;base64,` URL.
    pub favicon: Option<Vec<u8>>,
    pub enforces_secure_chat: bool,
}

fn string(s: &str) -> JsonElement {
    JsonElement::String(s.to_owned())
}

/// `JsonOps` view of a `ComponentSerialization` encoding (`serde_json` floats print like `Double.toString`).
fn value_to_element(v: &Value) -> JsonElement {
    match v {
        Value::Null => JsonElement::Null,
        Value::Bool(b) => JsonElement::Bool(*b),
        Value::Number(n) if n.is_i64() || n.is_u64() => JsonElement::Number(n.to_string()),
        Value::Number(n) => JsonElement::Number(java_double_to_string(n.as_f64().unwrap_or(0.0))),
        Value::String(s) => string(s),
        Value::Array(a) => JsonElement::Array(a.iter().map(value_to_element).collect()),
        Value::Object(o) => {
            let mut obj = JsonObject::new();
            for (k, v) in o {
                obj.add(k, value_to_element(v));
            }
            JsonElement::Object(obj)
        }
    }
}

/// The reverse, for the `description` input of `ComponentSerialization`. `None` for a number `serde_json` cannot hold
/// (outside the `f64` range), where vanilla's `BigDecimal` conversion would still succeed.
fn element_to_value(e: &JsonElement) -> Option<Value> {
    Some(match e {
        JsonElement::Null => Value::Null,
        JsonElement::Bool(b) => Value::Bool(*b),
        JsonElement::Number(t) => Value::Number(match (t.parse::<i64>(), t.parse::<u64>()) {
            (Ok(i), _) => Number::from(i),
            (_, Ok(u)) => Number::from(u),
            _ => Number::from_f64(t.parse::<f64>().ok()?)?,
        }),
        JsonElement::String(s) => Value::String(s.clone()),
        JsonElement::Array(a) => Value::Array(a.iter().map(element_to_value).collect::<Option<_>>()?),
        JsonElement::Object(o) => {
            let mut m = Map::new();
            for (k, v) in o.iter() {
                m.insert(k.to_owned(), element_to_value(v)?);
            }
            Value::Object(m)
        }
    })
}

/// `MapLike.get`: a missing key and a JSON `null` both read as absent.
fn field<'a>(o: &'a JsonObject, key: &str) -> Option<&'a JsonElement> {
    o.get(key).filter(|e| **e != JsonElement::Null)
}

/// `Codec.INT` over `JsonOps`: `Ok(None)` is a `DataResult` error, `Err` an exception that escapes the codec
/// (`LazilyParsedNumber.intValue` on a pathological number).
fn codec_int(e: &JsonElement) -> Result<Option<i32>, String> {
    match e {
        JsonElement::Number(t) => gson::lazily_parsed_int_value(t).map(Some),
        _ => Ok(None),
    }
}

fn codec_string(e: &JsonElement) -> Option<&str> {
    match e {
        JsonElement::String(s) => Some(s),
        _ => None,
    }
}

impl ServerStatus {
    /// `ServerStatus.CODEC` encode (`JsonOps`): fields equal to their defaults (empty description, empty `sample`,
    /// `enforcesSecureChat == false`) are omitted, in declaration order.
    pub fn to_json(&self) -> JsonElement {
        let mut o = JsonObject::new();
        if self.description != Component::empty() {
            let d = self.description.to_json().unwrap_or_else(|e| panic!("Failed to encode: {e}"));
            o.add("description", value_to_element(&d));
        }
        if let Some(p) = &self.players {
            let mut po = JsonObject::new();
            po.add_property_int("max", i64::from(p.max));
            po.add_property_int("online", i64::from(p.online));
            if !p.sample.is_empty() {
                let sample = p
                    .sample
                    .iter()
                    .map(|(id, name)| {
                        let mut e = JsonObject::new();
                        e.add("id", string(&id.hyphenated().to_string()));
                        e.add("name", string(name));
                        JsonElement::Object(e)
                    })
                    .collect();
                po.add("sample", JsonElement::Array(sample));
            }
            o.add("players", JsonElement::Object(po));
        }
        if let Some(v) = &self.version {
            let mut vo = JsonObject::new();
            vo.add("name", string(&v.name));
            vo.add_property_int("protocol", i64::from(v.protocol));
            o.add("version", JsonElement::Object(vo));
        }
        if let Some(png) = &self.favicon {
            o.add("favicon", string(&format!("{FAVICON_PREFIX}{}", base64_encode(png))));
        }
        if self.enforces_secure_chat {
            o.add_property_bool("enforcesSecureChat", true);
        }
        JsonElement::Object(o)
    }

    /// The text of the status response: `ByteBufCodecs.lenientJson` writes it with a Gson that has HTML escaping
    /// disabled.
    pub fn to_json_string(&self) -> String {
        self.to_json().to_gson_string_unescaped()
    }

    /// `ServerStatus.CODEC` decode. Optional fields (`lenientOptionalFieldOf`) that are missing or malformed fall back
    /// to their defaults; the only failures are a non-object document (`DecoderException` text of
    /// `fromCodec`) and exceptions that escape the codec (`NumberFormatException` text).
    pub fn from_json(json: &JsonElement) -> Result<Self, String> {
        let JsonElement::Object(o) = json else {
            let payload = json.to_compact_string();
            return Err(format!("Failed to decode: Not a JSON object: {payload} {payload}"));
        };
        let description = field(o, "description")
            .and_then(element_to_value)
            .and_then(|v| Component::from_json(&v).ok())
            .unwrap_or_else(Component::empty);
        let players = match field(o, "players") {
            Some(p) => Self::decode_players(p)?,
            None => None,
        };
        let version = match field(o, "version") {
            Some(JsonElement::Object(v)) => {
                let name = field(v, "name").and_then(codec_string);
                let protocol = match field(v, "protocol") {
                    Some(p) => codec_int(p)?,
                    None => None,
                };
                name.zip(protocol).map(|(name, protocol)| Version { name: name.to_owned(), protocol })
            }
            _ => None,
        };
        let favicon = field(o, "favicon")
            .and_then(codec_string)
            .and_then(|s| s.strip_prefix(FAVICON_PREFIX))
            .and_then(|b| base64_decode(&b.replace('\n', "")));
        let enforces_secure_chat = matches!(field(o, "enforcesSecureChat"), Some(JsonElement::Bool(true)));
        Ok(Self { description, players, version, favicon, enforces_secure_chat })
    }

    /// `ServerStatus.Players.CODEC`.
    fn decode_players(p: &JsonElement) -> Result<Option<Players>, String> {
        let JsonElement::Object(p) = p else { return Ok(None) };
        let max = match field(p, "max") {
            Some(e) => codec_int(e)?,
            None => None,
        };
        let online = match field(p, "online") {
            Some(e) => codec_int(e)?,
            None => None,
        };
        let sample = match field(p, "sample") {
            Some(JsonElement::Array(a)) => a.iter().map(Self::decode_name_and_id).collect::<Option<Vec<_>>>().unwrap_or_default(),
            _ => Vec::new(),
        };
        Ok(max.zip(online).map(|(max, online)| Players { max, online, sample }))
    }

    /// `NameAndId.CODEC` (`UUIDUtil.STRING_CODEC` id).
    fn decode_name_and_id(e: &JsonElement) -> Option<(Uuid, String)> {
        let JsonElement::Object(o) = e else { return None };
        let id = uuid_from_string(field(o, "id").and_then(codec_string)?).ok()?;
        Some((id, codec_string(field(o, "name")?)?.to_owned()))
    }
}

const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

fn base64_encode(data: &[u8]) -> String {
    let mut s = String::with_capacity(data.len().div_ceil(3) * 4);
    for c in data.chunks(3) {
        let n = c.iter().enumerate().fold(0u32, |a, (i, &b)| a | u32::from(b) << (16 - 8 * i));
        for i in 0..4 {
            if i <= c.len() { s.push(B64[(n >> (18 - 6 * i) & 63) as usize] as char) } else { s.push('=') }
        }
    }
    s
}

/// `Base64.getDecoder().decode(byte[])` (`Base64.Decoder.decode0`): padding is optional, the alphabet is strict.
/// `None` is the `IllegalArgumentException`.
fn base64_decode(s: &str) -> Option<Vec<u8>> {
    let src = s.as_bytes();
    let sl = src.len();
    if sl == 0 {
        return Some(Vec::new());
    }
    if sl < 2 {
        return None;
    }
    let mut out = Vec::with_capacity(sl / 4 * 3 + 3);
    let (mut bits, mut shiftto, mut sp) = (0u32, 18i32, 0usize);
    while sp < sl {
        let b = src[sp];
        sp += 1;
        let Some(v) = B64.iter().position(|&y| y == b) else {
            if b != b'=' {
                return None;
            }
            if shiftto == 6 {
                if sp == sl || src[sp] != b'=' {
                    return None;
                }
                sp += 1;
            } else if shiftto == 18 {
                return None;
            }
            break;
        };
        bits |= (v as u32) << shiftto;
        shiftto -= 6;
        if shiftto < 0 {
            out.extend_from_slice(&[(bits >> 16) as u8, (bits >> 8) as u8, bits as u8]);
            shiftto = 18;
            bits = 0;
        }
    }
    match shiftto {
        6 => out.push((bits >> 16) as u8),
        0 => out.extend_from_slice(&[(bits >> 16) as u8, (bits >> 8) as u8]),
        12 => return None,
        _ => {}
    }
    if sp < sl {
        return None;
    }
    Some(out)
}

/// `ServerboundStatusRequestPacket`
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, Packet)]
#[packet(state = status, flow = serverbound, name = "status_request")]
pub struct ServerboundStatusRequestPacket;

/// `ServerboundPingRequestPacket`
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, Packet)]
#[packet(state = status, flow = serverbound, name = "ping_request")]
pub struct ServerboundPingRequestPacket {
    pub time: i64,
}

/// `ClientboundPongResponsePacket`
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, Packet)]
#[packet(state = status, flow = clientbound, name = "pong_response")]
pub struct ClientboundPongResponsePacket {
    pub time: i64,
}

/// `ClientboundStatusResponsePacket`: JSON string, at most 32767 chars.
#[derive(Debug, Clone, PartialEq, Packet)]
#[packet(state = status, flow = clientbound, name = "status_response")]
pub struct ClientboundStatusResponsePacket {
    pub status: ServerStatus,
}

impl Encode for ClientboundStatusResponsePacket {
    fn encode(&self, out: &mut Vec<u8>) {
        write_utf(out, &self.status.to_json_string())
    }
}
impl Decode for ClientboundStatusResponsePacket {
    fn decode(input: &mut &[u8]) -> DecodeResult<Self> {
        let json = gson::parse_lenient_json_parser(&read_utf(input, 32767)?).map_err(|_| DecodeError::msg("Failed to parse JSON"))?;
        let status = ServerStatus::from_json(&json).map_err(DecodeError::Message)?;
        Ok(Self { status })
    }
}

packet_enum! {
    /// Serverbound status packets.
    pub enum ServerboundStatusPacket {
        StatusRequest(ServerboundStatusRequestPacket),
        PingRequest(ServerboundPingRequestPacket),
    }
}

packet_enum! {
    /// Clientbound status packets.
    pub enum ClientboundStatusPacket {
        StatusResponse(ClientboundStatusResponsePacket),
        PongResponse(ClientboundPongResponsePacket),
    }
}
