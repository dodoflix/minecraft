//! Status ping, login (offline, compression, optional encryption) and the configuration phase,
//! answered like the real 26.3 client.
use std::collections::HashMap;
use std::time::{Duration, Instant};

use mc_core::ident::Identifier;
use mc_nbt::Tag;
use mc_protocol::auth::{encrypt_with_public_key, generate_secret};
use mc_protocol::packets::common::{ClientInformation, CustomPayload, ResourcePackAction};
use mc_protocol::packets::configuration::{self as cfg, *};
use mc_protocol::packets::handshake::*;
use mc_protocol::packets::login::{self as lg, *};
use mc_protocol::packets::status::*;
use mc_protocol::PROTOCOL_VERSION;
use sha1::{Digest, Sha1};
use uuid::Uuid;

use crate::conn::{Conn, SharedLog};
use crate::error::{BotError, Result};

/// What to do with resource pack pushes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResourcePackPolicy {
    /// Answer `DECLINED` (server kicks if the pack is required).
    Decline,
    /// Answer `ACCEPTED`, really download the URL and verify its SHA-1, then `DOWNLOADED` + `SUCCESSFULLY_LOADED`
    /// (`FAILED_DOWNLOAD` if the download or hash check fails).
    Download,
    /// Answer `ACCEPTED`, `DOWNLOADED`, `SUCCESSFULLY_LOADED` without fetching anything.
    PretendLoaded,
}

#[derive(Debug, Clone)]
pub struct BotOptions {
    pub host: String,
    pub port: u16,
    pub username: String,
    /// Defaults to the vanilla offline UUID of `username`.
    pub uuid: Option<Uuid>,
    /// Client brand sent in `minecraft:brand`.
    pub brand: String,
    pub client_information: ClientInformation,
    pub resource_packs: ResourcePackPolicy,
    pub accept_code_of_conduct: bool,
    /// Per-read timeout for the login/configuration phases.
    pub timeout: Duration,
    /// Also store the raw bytes of every logged packet.
    pub record_raw: bool,
    /// Declares `minecraft:core` as known pack (the real client does) so the server may omit known registry entries.
    pub known_packs: bool,
}

impl BotOptions {
    pub fn new(host: impl Into<String>, port: u16, username: impl Into<String>) -> Self {
        Self {
            host: host.into(),
            port,
            username: username.into(),
            uuid: None,
            brand: "vanilla".into(),
            client_information: ClientInformation { view_distance: 6, ..ClientInformation::default() },
            resource_packs: ResourcePackPolicy::Download,
            accept_code_of_conduct: true,
            timeout: Duration::from_secs(60),
            record_raw: false,
            known_packs: true,
        }
    }

    pub fn profile_uuid(&self) -> Uuid {
        self.uuid.unwrap_or_else(|| mc_core::uuid_util::create_offline_player_uuid(&self.username))
    }
}

/// Result of a server list ping.
#[derive(Debug, Clone)]
pub struct PingResult {
    pub status: ServerStatus,
    pub latency: Duration,
}

pub async fn open(opts: &BotOptions, log: SharedLog) -> Result<Conn> {
    Conn::connect(&opts.host, opts.port, opts.timeout, log).await
}

async fn handshake(conn: &mut Conn, opts: &BotOptions, intention: ClientIntent) -> Result<()> {
    conn.send(&ServerboundHandshakePacket::ClientIntention(ClientIntentionPacket {
        protocol_version: PROTOCOL_VERSION,
        host_name: opts.host.clone(),
        port: opts.port,
        intention,
    }))
    .await
}

/// Server list ping (status request + ping/pong).
pub async fn status_ping(opts: &BotOptions, log: SharedLog) -> Result<PingResult> {
    let mut conn = open(opts, log).await?;
    handshake(&mut conn, opts, ClientIntent::Status).await?;
    conn.send(&ServerboundStatusPacket::StatusRequest(ServerboundStatusRequestPacket)).await?;
    let ClientboundStatusPacket::StatusResponse(resp) = conn.recv::<ClientboundStatusPacket>().await? else {
        return Err(BotError::Protocol("expected status response".into()));
    };
    let sent = Instant::now();
    conn.send(&ServerboundStatusPacket::PingRequest(ServerboundPingRequestPacket { time: 0x1234_5678 })).await?;
    let ClientboundStatusPacket::PongResponse(pong) = conn.recv::<ClientboundStatusPacket>().await? else {
        return Err(BotError::Protocol("expected pong".into()));
    };
    if pong.time != 0x1234_5678 {
        return Err(BotError::Protocol(format!("pong time mismatch: {}", pong.time)));
    }
    Ok(PingResult { status: resp.status, latency: sent.elapsed() })
}

/// One synced registry (`ClientboundRegistryDataPacket`).
#[derive(Debug, Clone, PartialEq)]
pub struct SyncedRegistry {
    pub id: Identifier,
    /// Entries in protocol-id order; `data` is `None` when the client is expected to know it from a known pack.
    pub entries: Vec<(Identifier, Option<Tag>)>,
}

/// Everything learned during login + configuration.
#[derive(Debug, Clone, Default)]
pub struct Configured {
    pub profile_name: String,
    pub profile_id: Uuid,
    pub compression_threshold: i32,
    pub encrypted: bool,
    pub server_brand: Option<String>,
    pub enabled_features: Vec<Identifier>,
    pub registries: Vec<SyncedRegistry>,
    pub tags: Vec<(Identifier, Vec<(Identifier, Vec<i32>)>)>,
    pub offered_known_packs: Vec<KnownPack>,
    /// Resource pack id → last status answered.
    pub resource_packs: Vec<(Uuid, ResourcePackAction)>,
    pub server_links: usize,
    pub report_details: Vec<(String, String)>,
    pub code_of_conduct: Option<String>,
    pub dialog: Option<Tag>,
    /// Set when the server asked for a transfer (the connection is not finished).
    pub transfer: Option<(String, i32)>,
    /// Cookie jar (`store_cookie`), keyed by identifier.
    pub cookies: HashMap<Identifier, Vec<u8>>,
}

/// Login phase through `login_finished`/`login_acknowledged` (+ the brand/client information the real client sends
/// right after). On return the connection is in the configuration state.
pub async fn login(conn: &mut Conn, opts: &BotOptions) -> Result<Configured> {
    handshake(conn, opts, ClientIntent::Login).await?;
    conn.send(&ServerboundLoginPacket::Hello(ServerboundHelloPacket {
        name: opts.username.clone(),
        profile_id: opts.profile_uuid(),
    }))
    .await?;

    let mut out = Configured { compression_threshold: -1, ..Default::default() };
    loop {
        match conn.recv::<ClientboundLoginPacket>().await? {
            ClientboundLoginPacket::LoginCompression(p) => {
                conn.reader.set_compression(p.compression_threshold, true);
                conn.writer.set_compression(p.compression_threshold);
                out.compression_threshold = p.compression_threshold;
            }
            ClientboundLoginPacket::Hello(h) => {
                if h.should_authenticate {
                    return Err(BotError::Protocol(
                        "server is in online mode and requires session authentication (not supported by the testbot)".into(),
                    ));
                }
                let secret = generate_secret();
                let key = ServerboundKeyPacket {
                    keybytes: encrypt_with_public_key(&h.public_key, &secret)
                        .map_err(|e| BotError::Protocol(format!("encrypt secret: {e}")))?,
                    encrypted_challenge: encrypt_with_public_key(&h.public_key, &h.challenge)
                        .map_err(|e| BotError::Protocol(format!("encrypt challenge: {e}")))?,
                };
                conn.send(&ServerboundLoginPacket::Key(key)).await?;
                conn.writer.enable_encryption(secret);
                conn.reader.enable_encryption(secret);
                out.encrypted = true;
            }
            ClientboundLoginPacket::LoginFinished(f) => {
                out.profile_name = f.game_profile.name;
                out.profile_id = f.game_profile.id;
                conn.send(&ServerboundLoginPacket::LoginAcknowledged(ServerboundLoginAcknowledgedPacket)).await?;
                break;
            }
            ClientboundLoginPacket::LoginDisconnect(d) => {
                return Err(BotError::Disconnected(d.reason.0.get_string()));
            }
            ClientboundLoginPacket::CustomQuery(q) => {
                // Like vanilla: no payload understood -> empty answer.
                conn.send(&ServerboundLoginPacket::CustomQueryAnswer(ServerboundCustomQueryAnswerPacket {
                    transaction_id: q.transaction_id,
                    payload: None,
                }))
                .await?;
            }
            ClientboundLoginPacket::CookieRequest(r) => {
                conn.send(&ServerboundLoginPacket::CookieResponse(lg::ServerboundCookieResponsePacket {
                    payload: out.cookies.get(&r.key).cloned(),
                    key: r.key,
                }))
                .await?;
            }
        }
    }

    conn.send(&ServerboundConfigurationPacket::CustomPayload(ServerboundCustomPayloadPacket {
        payload: CustomPayload::brand(&opts.brand),
    }))
    .await?;
    conn.send(&ServerboundConfigurationPacket::ClientInformation(ServerboundClientInformationPacket {
        information: opts.client_information.clone(),
    }))
    .await?;
    Ok(out)
}

/// Login + configuration; on return the connection is in the play state.
pub async fn login_and_configure(conn: &mut Conn, opts: &BotOptions) -> Result<Configured> {
    let mut out = login(conn, opts).await?;
    configure(conn, opts, &mut out).await?;
    Ok(out)
}

/// The configuration phase until `finish_configuration` was answered (or the server asks for a transfer).
/// Also used for server-initiated re-configuration from the play state.
pub async fn configure(conn: &mut Conn, opts: &BotOptions, out: &mut Configured) -> Result<()> {
    let http = reqwest::Client::new();
    loop {
        use ClientboundConfigurationPacket as C;
        use ServerboundConfigurationPacket as S;
        match conn.recv::<ClientboundConfigurationPacket>().await? {
            C::CookieRequest(r) => {
                conn.send(&S::CookieResponse(cfg::ServerboundCookieResponsePacket {
                    payload: out.cookies.get(&r.key).cloned(),
                    key: r.key,
                }))
                .await?;
            }
            C::StoreCookie(c) => {
                out.cookies.insert(c.key, c.payload);
            }
            C::CustomPayload(p) => {
                if let Some(b) = p.payload.as_brand() {
                    out.server_brand = Some(b);
                }
            }
            C::Disconnect(d) => return Err(BotError::Disconnected(d.reason.get_string())),
            C::FinishConfiguration(_) => {
                conn.send(&S::FinishConfiguration(ServerboundFinishConfigurationPacket)).await?;
                return Ok(());
            }
            C::KeepAlive(k) => {
                conn.send(&S::KeepAlive(ServerboundKeepAlivePacket { id: k.id })).await?;
            }
            C::Ping(p) => {
                conn.send(&S::Pong(ServerboundPongPacket { id: p.id })).await?;
            }
            C::ResetChat(_) | C::PostEffects(_) => {}
            C::RegistryData(r) => {
                out.registries.push(SyncedRegistry {
                    id: r.registry,
                    entries: r.entries.into_iter().map(|e| (e.id, e.data)).collect(),
                });
            }
            C::ResourcePackPop(p) => {
                let ids: Vec<Uuid> = match p.id {
                    Some(id) => vec![id],
                    None => out.resource_packs.iter().map(|(id, _)| *id).collect(),
                };
                for id in ids {
                    if let Some(e) = out.resource_packs.iter_mut().find(|(i, _)| *i == id)
                        && e.1 != ResourcePackAction::Declined
                    {
                        e.1 = ResourcePackAction::Discarded;
                        conn.send(&S::ResourcePack(ServerboundResourcePackPacket { id, action: ResourcePackAction::Discarded }))
                            .await?;
                    }
                }
            }
            C::ResourcePackPush(p) => {
                let answers = resource_pack_answers(&http, opts.resource_packs, &p.url, &p.hash).await;
                for action in answers {
                    conn.send(&S::ResourcePack(ServerboundResourcePackPacket { id: p.id, action })).await?;
                    match out.resource_packs.iter_mut().find(|(i, _)| *i == p.id) {
                        Some(e) => e.1 = action,
                        None => out.resource_packs.push((p.id, action)),
                    }
                }
            }
            C::Transfer(t) => {
                out.transfer = Some((t.host, t.port));
                return Ok(());
            }
            C::UpdateEnabledFeatures(f) => out.enabled_features = f.features,
            C::UpdateTags(t) => {
                out.tags = t.tags.into_iter().map(|(r, p)| (r, p.tags)).collect();
            }
            C::SelectKnownPacks(k) => {
                // The real client lists the built-in packs it has that the server offered (vanilla core at its version).
                let own_version = own_version();
                let known: Vec<KnownPack> = if opts.known_packs {
                    k.known_packs.iter().filter(|p| p.namespace == "minecraft" && p.version == own_version).cloned().collect()
                } else {
                    Vec::new()
                };
                out.offered_known_packs = k.known_packs;
                conn.send(&S::SelectKnownPacks(ServerboundSelectKnownPacks { known_packs: known })).await?;
            }
            C::CustomReportDetails(d) => out.report_details = d.details.0,
            C::ServerLinks(l) => out.server_links = l.links.len(),
            C::ClearDialog(_) => out.dialog = None,
            C::ShowDialog(d) => out.dialog = Some(d.dialog),
            C::CodeOfConduct(c) => {
                out.code_of_conduct = Some(c.code_of_conduct);
                if opts.accept_code_of_conduct {
                    conn.send(&S::AcceptCodeOfConduct(ServerboundAcceptCodeOfConductPacket)).await?;
                }
            }
        }
    }
}

/// `SharedConstants.getCurrentVersion().id()` of the vanilla data this build was generated from.
fn own_version() -> String {
    mc_data::datapack::read_string("version.json")
        .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
        .and_then(|v| v["id"].as_str().map(str::to_string))
        .unwrap_or_else(|| "26.3".into())
}

/// The statuses the real client reports for a pushed pack, in order.
async fn resource_pack_answers(
    http: &reqwest::Client,
    policy: ResourcePackPolicy,
    url: &str,
    hash: &str,
) -> Vec<ResourcePackAction> {
    use ResourcePackAction::*;
    match policy {
        ResourcePackPolicy::Decline => vec![Declined],
        ResourcePackPolicy::PretendLoaded => vec![Accepted, Downloaded, SuccessfullyLoaded],
        ResourcePackPolicy::Download => {
            if reqwest::Url::parse(url).is_err() {
                return vec![InvalidUrl];
            }
            let fetched = async { http.get(url).send().await?.error_for_status()?.bytes().await }.await;
            match fetched {
                Err(_) => vec![Accepted, FailedDownload],
                Ok(bytes) => {
                    let digest = Sha1::digest(&bytes);
                    let actual: String = digest.iter().map(|b| format!("{b:02x}")).collect();
                    if hash.is_empty() || actual.eq_ignore_ascii_case(hash) {
                        vec![Accepted, Downloaded, SuccessfullyLoaded]
                    } else {
                        vec![Accepted, FailedDownload]
                    }
                }
            }
        }
    }
}
