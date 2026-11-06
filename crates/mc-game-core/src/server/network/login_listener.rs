//! `ServerLoginPacketListenerImpl`: username validation, offline UUIDs, the online-mode encryption handshake with the
//! session server (`hasJoined`), the login checks that need the player list (game thread), compression, and
//! `LoginFinished` → `LoginAcknowledged`.

use super::connection::{ReadError, java_host_address};
use super::connection_task::{ConnectionContext, Event};
use super::hooks::on_server;
use mc_core::uuid_util::create_offline_player_uuid;
use mc_protocol::auth::digest_server_id;
use mc_protocol::codec::GameProfile;
use mc_protocol::net::NetError;
use mc_protocol::packets::login::{
    ClientboundHelloPacket, ClientboundLoginCompressionPacket, ClientboundLoginFinishedPacket,
    ServerboundHelloPacket, ServerboundKeyPacket, ServerboundLoginPacket,
};
use mc_text::Component;
use rand::RngCore;
use std::sync::atomic::{AtomicU32, Ordering};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};

/// `MAX_TICKS_BEFORE_LOGIN`.
const MAX_TICKS_BEFORE_LOGIN: i32 = 600;
/// `ServerLoginPacketListenerImpl.UNIQUE_THREAD_ID`.
static UNIQUE_THREAD_ID: AtomicU32 = AtomicU32::new(0);

/// `ServerCommonPacketListenerImpl.DISCONNECT_UNEXPECTED_QUERY`.
pub(super) fn disconnect_unexpected_query() -> Component {
    Component::translatable("multiplayer.disconnect.unexpected_query_response", vec![])
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    Hello,
    Key,
    Authenticating,
    Verifying,
    WaitingForDupeDisconnect,
    ProtocolSwitching,
    Accepted,
}

/// What the "User Authenticator #n" thread reports back (it already did the logging vanilla does there).
enum AuthResult {
    /// `startClientVerification(profile)`.
    Verified(GameProfile),
    /// `disconnect(reason)` (already logged).
    Disconnect(Component),
}

/// `StringUtil.isValidPlayerName`: at most 16 UTF-16 units, each in `33..=126`.
pub fn is_valid_player_name(name: &str) -> bool {
    let mut units = 0;
    for c in name.encode_utf16() {
        units += 1;
        if units > 16 || c <= 32 || c >= 127 {
            return false;
        }
    }
    true
}

/// `UUIDUtil.createOfflineProfile`.
pub fn create_offline_profile(name: &str) -> GameProfile {
    GameProfile {
        id: create_offline_player_uuid(name),
        name: name.to_owned(),
        properties: Vec::new(),
    }
}

struct Login {
    state: State,
    tick: i32,
    requested_username: Option<String>,
    authenticated_profile: Option<GameProfile>,
    challenge: [u8; 4],
    auth_tx: UnboundedSender<AuthResult>,
}

impl Login {
    /// `getUserName`.
    fn user_name(&self, ctx: &ConnectionContext, log_ips: bool) -> String {
        let address = ctx.handle.loggable_address(log_ips);
        match &self.requested_username {
            Some(name) => format!("{name} ({address})"),
            None => address,
        }
    }

    /// `disconnect(component)`: logs from the server thread, sends `ClientboundLoginDisconnect` and closes.
    fn disconnect(&self, ctx: &ConnectionContext, log_ips: bool, reason: Component) {
        let user = self.user_name(ctx, log_ips);
        let text = reason.get_string();
        ctx.log_on_server(move || log::info!("Disconnecting {user}: {text}"));
        ctx.sender.disconnect(reason);
    }
}

/// Runs the login protocol. `Some(profile)` once `LoginAcknowledged` was handled (state `ACCEPTED`): the caller starts
/// the configuration phase for that profile; `None` if the connection ended.
pub(super) async fn run(ctx: &mut ConnectionContext) -> Option<GameProfile> {
    let (auth_tx, mut auth_rx): (_, UnboundedReceiver<AuthResult>) = unbounded_channel();
    let mut challenge = [0u8; 4];
    rand::rng().fill_bytes(&mut challenge);
    let mut login = Login {
        state: State::Hello,
        tick: 0,
        requested_username: None,
        authenticated_profile: None,
        challenge,
        auth_tx,
    };
    let settings = ctx.hooks.settings();
    let log_ips = settings.log_ips;

    loop {
        let event = tokio::select! {
            ev = ctx.next_event() => ev,
            Some(result) = auth_rx.recv() => {
                match result {
                    AuthResult::Verified(profile) => {
                        login.authenticated_profile = Some(profile);
                        login.state = State::Verifying;
                    }
                    AuthResult::Disconnect(reason) => ctx.sender.disconnect(reason),
                }
                continue;
            }
        };
        match event {
            Event::Stopped => {
                // `onDisconnect`: logged once the socket is gone, from the server thread.
                ctx.wait_closed().await;
                let user = login.user_name(ctx, log_ips);
                let reason = ctx
                    .handle
                    .disconnection_details()
                    .map_or_else(
                        || Component::translatable("multiplayer.disconnect.generic", vec![]),
                        |d| d.reason,
                    )
                    .get_string();
                ctx.log_on_server(move || log::info!("{user} lost connection: {reason}"));
                return None;
            }
            Event::ReadError(e) => ctx.fault(e),
            Event::Tick => {
                tick(ctx, &mut login, log_ips).await?;
            }
            Event::Payload(p) => match ServerboundLoginPacket::decode_payload(&p) {
                Err(e) => ctx.fault(ReadError::Net(NetError::Decode(e))),
                Ok(packet) => match handle_packet(ctx, &mut login, packet, log_ips) {
                    Err(cause) => ctx.fault_cause(&cause),
                    Ok(()) if login.state == State::Accepted => return login.authenticated_profile,
                    Ok(()) => {}
                },
            },
        }
    }
}

/// `tick()`. `None` = the game thread is gone.
async fn tick(ctx: &mut ConnectionContext, login: &mut Login, log_ips: bool) -> Option<()> {
    if login.state == State::Verifying {
        let profile = login
            .authenticated_profile
            .clone()
            .expect("verifying without profile");
        verify_login_and_finish_connection_setup(ctx, login, profile, log_ips).await?;
    }
    if login.state == State::WaitingForDupeDisconnect {
        let id = login
            .authenticated_profile
            .as_ref()
            .expect("waiting without profile")
            .id;
        let hooks = ctx.hooks.clone();
        let in_world = on_server(&*ctx.game, move |server| {
            hooks.is_player_in_world(server, id)
        })
        .await?;
        if !in_world {
            let profile = login
                .authenticated_profile
                .clone()
                .expect("waiting without profile");
            finish_login_and_wait_for_client(ctx, login, profile);
        }
    }
    let previous = login.tick;
    login.tick += 1;
    if previous == MAX_TICKS_BEFORE_LOGIN {
        login.disconnect(
            ctx,
            log_ips,
            Component::translatable("multiplayer.disconnect.slow_login", vec![]),
        );
    }
    Some(())
}

/// `handleHello` / `handleKey` / `handleCustomQueryPacket` / `handleLoginAcknowledgement` / `handleCookieResponse`.
/// `Err(cause)` is the Java exception `toString()` that vanilla's `exceptionCaught` reports.
fn handle_packet(
    ctx: &mut ConnectionContext,
    login: &mut Login,
    packet: ServerboundLoginPacket,
    log_ips: bool,
) -> Result<(), String> {
    match packet {
        ServerboundLoginPacket::Hello(hello) => handle_hello(ctx, login, hello),
        ServerboundLoginPacket::Key(key) => handle_key(ctx, login, key),
        ServerboundLoginPacket::CustomQueryAnswer(_)
        | ServerboundLoginPacket::CookieResponse(_) => {
            login.disconnect(ctx, log_ips, disconnect_unexpected_query());
            Ok(())
        }
        ServerboundLoginPacket::LoginAcknowledged(_) => {
            if login.state != State::ProtocolSwitching {
                return Err(
                    "java.lang.IllegalStateException: Unexpected login acknowledgement packet"
                        .to_owned(),
                );
            }
            login.state = State::Accepted;
            Ok(())
        }
    }
}

fn handle_hello(
    ctx: &mut ConnectionContext,
    login: &mut Login,
    packet: ServerboundHelloPacket,
) -> Result<(), String> {
    if login.state != State::Hello {
        return Err("java.lang.IllegalStateException: Unexpected hello packet".to_owned());
    }
    if !is_valid_player_name(&packet.name) {
        return Err("java.lang.IllegalStateException: Invalid characters in username".to_owned());
    }
    login.requested_username = Some(packet.name.clone());
    let settings = ctx.hooks.settings();
    match settings.singleplayer_profile {
        Some(profile) if packet.name.eq_ignore_ascii_case(&profile.name) => {
            start_client_verification(login, profile)
        }
        _ => {
            if settings.uses_authentication {
                login.state = State::Key;
                ctx.sender.send(&ClientboundHelloPacket {
                    server_id: String::new(),
                    public_key: ctx.hooks.key_pair().public_key_der().to_vec(),
                    challenge: login.challenge.to_vec(),
                    should_authenticate: true,
                });
            } else {
                start_client_verification(login, create_offline_profile(&packet.name));
            }
        }
    }
    Ok(())
}

fn start_client_verification(login: &mut Login, profile: GameProfile) {
    login.authenticated_profile = Some(profile);
    login.state = State::Verifying;
}

fn handle_key(
    ctx: &mut ConnectionContext,
    login: &mut Login,
    packet: ServerboundKeyPacket,
) -> Result<(), String> {
    if login.state != State::Key {
        return Err("java.lang.IllegalStateException: Unexpected key packet".to_owned());
    }
    let protocol_error = || "java.lang.IllegalStateException: Protocol error".to_owned();
    let key_pair = ctx.hooks.key_pair();
    // `packet.isChallengeValid(challenge, privateKey)`
    match key_pair.decrypt(&packet.encrypted_challenge) {
        Ok(challenge) if challenge == login.challenge => {}
        _ => return Err(protocol_error()),
    }
    let secret = key_pair
        .decrypt_secret_key(&packet.keybytes)
        .map_err(|_| protocol_error())?;
    let digest = digest_server_id("", &secret, key_pair.public_key_der());
    login.state = State::Authenticating;
    ctx.reader.enable_encryption(secret);
    ctx.sender.enable_encryption(secret);

    let settings = ctx.hooks.settings();
    let name = login
        .requested_username
        .clone()
        .expect("player name not initialized");
    let ip = settings
        .prevent_proxy_connections
        .then(|| ctx.handle.remote_address())
        .flatten()
        .map(|addr| java_host_address(&addr.ip(), false));
    spawn_authenticator(
        ctx,
        login,
        name,
        digest,
        ip,
        settings.singleplayer_profile.is_some(),
        settings.log_ips,
    );
    Ok(())
}

/// The "User Authenticator #n" thread: one blocking `hasJoined` call plus the logging vanilla does from that thread.
fn spawn_authenticator(
    ctx: &ConnectionContext,
    login: &Login,
    name: String,
    digest: String,
    ip: Option<String>,
    singleplayer: bool,
    log_ips: bool,
) {
    let session = ctx.hooks.session_client();
    let tx = login.auth_tx.clone();
    let game = ctx.game.clone();
    let hooks = ctx.hooks.clone();
    let runtime = tokio::runtime::Handle::current();
    let user_name = login.user_name(ctx, log_ips);
    let thread_name = format!(
        "User Authenticator #{}",
        UNIQUE_THREAD_ID.fetch_add(1, Ordering::Relaxed) + 1
    );
    let spawned = std::thread::Builder::new()
        .name(thread_name)
        .spawn(move || {
            let disconnect = |reason: Component| {
                log::info!("Disconnecting {user_name}: {}", reason.get_string());
                let _ = tx.send(AuthResult::Disconnect(reason));
            };
            match runtime.block_on(session.has_joined(&name, &digest, ip.as_deref())) {
                Ok(Some(profile)) => {
                    log::info!("UUID of player {} is {}", profile.name, profile.id);
                    game.execute(Box::new(move |server| hooks.report_login_activity(server)));
                    let _ = tx.send(AuthResult::Verified(profile));
                }
                Ok(None) if singleplayer => {
                    log::warn!("Failed to verify username but will let them in anyway!");
                    let _ = tx.send(AuthResult::Verified(create_offline_profile(&name)));
                }
                Ok(None) => {
                    disconnect(Component::translatable(
                        "multiplayer.disconnect.unverified_username",
                        vec![],
                    ));
                    log::error!("Username '{name}' tried to join with an invalid session");
                }
                Err(_) if singleplayer => {
                    log::warn!("Authentication servers are down but will let them in anyway!");
                    let _ = tx.send(AuthResult::Verified(create_offline_profile(&name)));
                }
                Err(_) => {
                    disconnect(Component::translatable(
                        "multiplayer.disconnect.authservers_down",
                        vec![],
                    ));
                    log::error!("Couldn't verify username because servers are unavailable");
                }
            }
        });
    if let Err(e) = spawned {
        log::error!("Couldn't start the user authenticator thread: {e}");
        ctx.sender.disconnect(Component::translatable(
            "multiplayer.disconnect.authservers_down",
            vec![],
        ));
    }
}

/// `verifyLoginAndFinishConnectionSetup`. `None` = the game thread is gone.
async fn verify_login_and_finish_connection_setup(
    ctx: &mut ConnectionContext,
    login: &mut Login,
    profile: GameProfile,
    log_ips: bool,
) -> Option<()> {
    let Some(addr) = ctx.handle.remote_address() else {
        login.disconnect(
            ctx,
            log_ips,
            Component::translatable("multiplayer.disconnect.generic", vec![]),
        );
        return Some(());
    };
    let hooks = ctx.hooks.clone();
    let check_profile = profile.clone();
    let (error, wait_for_disconnection) = {
        let id = profile.id;
        on_server(&*ctx.game, move |server| {
            match hooks.can_player_login(server, addr, &check_profile) {
                Some(error) => (Some(error), false),
                // Vanilla sends the compression packet between the two calls; nothing can observe the difference.
                None => (None, hooks.disconnect_all_players_with_profile(server, id)),
            }
        })
        .await?
    };
    if let Some(error) = error {
        login.disconnect(ctx, log_ips, error);
        return Some(());
    }
    let threshold = ctx.hooks.settings().compression_threshold;
    if threshold >= 0 {
        ctx.sender.send(&ClientboundLoginCompressionPacket {
            compression_threshold: threshold,
        });
        ctx.sender.set_compression(threshold);
        ctx.reader.set_compression(threshold, true);
    }
    if wait_for_disconnection {
        login.state = State::WaitingForDupeDisconnect;
    } else {
        finish_login_and_wait_for_client(ctx, login, profile);
    }
    Some(())
}

fn finish_login_and_wait_for_client(
    ctx: &ConnectionContext,
    login: &mut Login,
    profile: GameProfile,
) {
    login.state = State::ProtocolSwitching;
    ctx.sender.send(&ClientboundLoginFinishedPacket {
        game_profile: profile,
        session_id: ctx.listener.session_id(),
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn player_names() {
        assert!(is_valid_player_name("Steve"));
        assert!(is_valid_player_name(""));
        assert!(is_valid_player_name(&"a".repeat(16)));
        assert!(!is_valid_player_name(&"a".repeat(17)));
        assert!(!is_valid_player_name("a b"));
        assert!(!is_valid_player_name("caf\u{e9}"));
        assert!(!is_valid_player_name("a\u{7f}"));
        assert!(is_valid_player_name("~_.-"));
        // Non-BMP counts as two UTF-16 units and is outside 33..=126 anyway.
        assert!(!is_valid_player_name("\u{1F600}"));
    }
}
