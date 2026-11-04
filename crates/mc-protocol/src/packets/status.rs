//! `status` state: server list ping.
use mc_text::Component;
use serde_json::{Map, Value};
use uuid::Uuid;

use crate::codec::{read_utf, write_utf, DecodeError, DecodeResult};
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

impl ServerStatus {
    /// `ServerStatus.CODEC` encode: fields equal to their defaults (empty description, empty
    /// `sample`, `enforcesSecureChat == false`) are omitted, in declaration order.
    pub fn to_json(&self) -> Value {
        let mut o = Map::new();
        if self.description != Component::empty() {
            o.insert("description".into(), self.description.to_json().unwrap_or_else(|e| panic!("Failed to encode: {e}")));
        }
        if let Some(p) = &self.players {
            let mut po = Map::new();
            po.insert("max".into(), p.max.into());
            po.insert("online".into(), p.online.into());
            if !p.sample.is_empty() {
                let sample = p
                    .sample
                    .iter()
                    .map(|(id, name)| {
                        let mut e = Map::new();
                        e.insert("id".into(), id.hyphenated().to_string().into());
                        e.insert("name".into(), name.clone().into());
                        Value::Object(e)
                    })
                    .collect();
                po.insert("sample".into(), Value::Array(sample));
            }
            o.insert("players".into(), Value::Object(po));
        }
        if let Some(v) = &self.version {
            let mut vo = Map::new();
            vo.insert("name".into(), v.name.clone().into());
            vo.insert("protocol".into(), v.protocol.into());
            o.insert("version".into(), Value::Object(vo));
        }
        if let Some(png) = &self.favicon {
            o.insert("favicon".into(), format!("{FAVICON_PREFIX}{}", base64_encode(png)).into());
        }
        if self.enforces_secure_chat {
            o.insert("enforcesSecureChat".into(), true.into());
        }
        Value::Object(o)
    }

    /// `ServerStatus.CODEC` decode. Optional fields (`lenientOptionalFieldOf`) that are missing or
    /// malformed fall back to their defaults.
    pub fn from_json(v: &Value) -> Result<Self, String> {
        let o = v.as_object().ok_or("Not a JSON object")?;
        let description = match o.get("description") {
            Some(d) => Component::from_json(d).unwrap_or_else(|_| Component::empty()),
            None => Component::empty(),
        };
        let players = o.get("players").and_then(|p| {
            let p = p.as_object()?;
            let sample = p
                .get("sample")
                .and_then(Value::as_array)
                .map(|a| {
                    a.iter()
                        .map(|e| {
                            let id = Uuid::parse_str(e.get("id")?.as_str()?).ok()?;
                            Some((id, e.get("name")?.as_str()?.to_owned()))
                        })
                        .collect::<Option<Vec<_>>>()
                })
                .and_then(|s| s)
                .unwrap_or_default();
            Some(Players {
                max: i32::try_from(p.get("max")?.as_i64()?).ok()?,
                online: i32::try_from(p.get("online")?.as_i64()?).ok()?,
                sample,
            })
        });
        let version = o.get("version").and_then(|v| {
            Some(Version {
                name: v.get("name")?.as_str()?.to_owned(),
                protocol: i32::try_from(v.get("protocol")?.as_i64()?).ok()?,
            })
        });
        let favicon = o
            .get("favicon")
            .and_then(Value::as_str)
            .and_then(|s| s.strip_prefix(FAVICON_PREFIX))
            .and_then(|b| base64_decode(&b.replace('\n', "")));
        let enforces_secure_chat = o.get("enforcesSecureChat").and_then(Value::as_bool).unwrap_or(false);
        Ok(Self { description, players, version, favicon, enforces_secure_chat })
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

/// Java `Base64.getDecoder()` (strict, padded).
fn base64_decode(s: &str) -> Option<Vec<u8>> {
    let b = s.as_bytes();
    if !b.len().is_multiple_of(4) {
        return None;
    }
    let mut out = Vec::with_capacity(b.len() / 4 * 3);
    for (ci, c) in b.chunks(4).enumerate() {
        let last = ci == b.len() / 4 - 1;
        let pad = c.iter().rev().take_while(|&&x| x == b'=').count();
        if pad > 2 || (pad > 0 && !last) {
            return None;
        }
        let mut n = 0u32;
        for (i, &x) in c.iter().enumerate() {
            let v = if i >= 4 - pad { 0 } else { B64.iter().position(|&y| y == x)? as u32 };
            n |= v << (18 - 6 * i);
        }
        out.extend_from_slice(&n.to_be_bytes()[1..4 - pad]);
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
        write_utf(out, &self.status.to_json().to_string())
    }
}
impl Decode for ClientboundStatusResponsePacket {
    fn decode(input: &mut &[u8]) -> DecodeResult<Self> {
        let json: Value = serde_json::from_str(&read_utf(input, 32767)?)
            .map_err(|e| DecodeError::Message(format!("Failed to parse JSON: {e}")))?;
        let status = ServerStatus::from_json(&json).map_err(|e| DecodeError::Message(format!("Failed to decode: {e} {json}")))?;
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
