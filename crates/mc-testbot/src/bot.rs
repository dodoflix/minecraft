//! The scriptable bot: connection task (20 TPS client tick loop, packet dispatch) and the actions API.
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use mc_core::direction::Direction;
use mc_core::phys::Vec3;
use mc_core::pos::block_pos::BlockPos;
use mc_data::block::BlockState;
use mc_protocol::packets::play::*;
use mc_protocol::net::NetError;
use tokio::sync::{mpsc, watch, Notify};
use tokio::task::JoinHandle;
use tokio::time::{interval, Instant, MissedTickBehavior};

use crate::conn::{log_packet, log_undecodable, short_name, Conn, SharedLog, WirePacket};
use crate::error::{BotError, Result};
use crate::join::{self, BotOptions};
use crate::log::{Dir, PacketLog};
use crate::play_enum::{ClientboundPlayPacket, ServerboundPlayPacket};
use crate::state::{Out, Phase, State, WalkGoal};

/// State shared between the connection task and the API.
pub struct Shared {
    pub state: Mutex<State>,
    pub log: SharedLog,
    notify: Notify,
    out: mpsc::UnboundedSender<Vec<u8>>,
}

impl Shared {
    pub fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Encode, log and queue a packet for the connection task.
    pub fn send(&self, packet: &ServerboundPlayPacket) {
        let payload = crate::conn::encode(packet);
        log_packet(&self.log, Dir::Out, packet, &payload);
        let _ = self.out.send(payload);
    }

    fn wake(&self) {
        self.notify.notify_waiters();
    }
}

pub struct Bot {
    pub shared: Arc<Shared>,
    pub options: BotOptions,
    stop: watch::Sender<bool>,
    task: Mutex<Option<JoinHandle<()>>>,
}

impl Bot {
    /// Handshake, login and configuration like the real client, then start the play loop.
    pub async fn connect(options: BotOptions) -> Result<Bot> {
        let log: SharedLog = Arc::new(Mutex::new(PacketLog::new()));
        log.lock().map_err(|_| BotError::Protocol("log poisoned".into()))?.record_raw = options.record_raw;
        let mut conn = join::open(&options, log.clone()).await?;
        let configured = join::login_and_configure(&mut conn, &options).await?;
        if let Some((host, port)) = &configured.transfer {
            return Err(BotError::Protocol(format!("server requested a transfer to {host}:{port} during configuration")));
        }
        let (out_tx, out_rx) = mpsc::unbounded_channel();
        let shared = Arc::new(Shared { state: Mutex::new(State::new(configured)), log, notify: Notify::new(), out: out_tx });
        let (stop_tx, stop_rx) = watch::channel(false);
        let task = tokio::spawn(run(shared.clone(), conn, out_rx, stop_rx, options.clone()));
        Ok(Bot { shared, options, stop: stop_tx, task: Mutex::new(Some(task)) })
    }

    // ---- inspection ---------------------------------------------------------------------------

    pub fn with_state<T>(&self, f: impl FnOnce(&State) -> T) -> T {
        f(&self.shared.lock())
    }

    pub fn with_state_mut<T>(&self, f: impl FnOnce(&mut State) -> T) -> T {
        f(&mut self.shared.lock())
    }

    pub fn position(&self) -> Vec3 {
        self.with_state(|s| s.body.pos)
    }

    pub fn log_text(&self) -> String {
        self.shared.log.lock().map(|l| l.to_text()).unwrap_or_default()
    }

    pub fn write_log(&self, path: impl AsRef<std::path::Path>) -> std::io::Result<()> {
        self.shared.log.lock().map_err(|_| std::io::Error::other("log poisoned"))?.write_jsonl(path)
    }

    /// Wait until `f` returns `Some`, the connection ends (`Err(Disconnected)`) or `timeout` passes.
    pub async fn wait_for<T>(&self, timeout: Duration, mut f: impl FnMut(&State) -> Option<T>) -> Result<T> {
        let deadline = Instant::now() + timeout;
        loop {
            let notified = self.shared.notify.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            {
                let st = self.shared.lock();
                if let Some(v) = f(&st) {
                    return Ok(v);
                }
                if let Some(reason) = st.disconnected() {
                    return Err(BotError::Disconnected(reason.to_string()));
                }
            }
            tokio::select! {
                _ = &mut notified => {}
                _ = tokio::time::sleep_until(deadline) => return Err(BotError::Timeout("wait_for condition".into())),
            }
        }
    }

    /// Wait for the `login` packet.
    pub async fn wait_joined(&self, timeout: Duration) -> Result<()> {
        self.wait_for(timeout, |s| s.joined().then_some(())).await
    }

    /// Wait until the chunk under the player is loaded, the server confirmed the position and `player_loaded`
    /// was sent (the real client's loading screen is gone).
    pub async fn wait_ready(&self, timeout: Duration) -> Result<()> {
        self.wait_for(timeout, |s| s.level_load.player_loaded_sent.then_some(())).await
    }

    pub async fn wait_ticks(&self, ticks: u64) -> Result<()> {
        let target = self.with_state(|s| s.tick) + ticks;
        self.wait_for(Duration::from_millis(ticks * 200 + 5000), |s| (s.tick >= target).then_some(())).await
    }

    /// Why the connection ended, if it did.
    pub fn disconnect_reason(&self) -> Option<String> {
        self.with_state(|s| s.disconnected().map(str::to_string))
    }

    /// Fail if the server dropped us.
    pub fn assert_connected(&self) -> Result<()> {
        match self.disconnect_reason() {
            Some(r) => Err(BotError::Disconnected(r)),
            None => Ok(()),
        }
    }

    /// Close the connection (the real client's `disconnect`).
    pub async fn disconnect(&self) {
        let _ = self.stop.send(true);
        let task = self.task.lock().ok().and_then(|mut t| t.take());
        if let Some(t) = task {
            let _ = t.await;
        }
    }

    pub fn send(&self, packet: ServerboundPlayPacket) {
        self.shared.send(&packet);
    }

    // ---- movement -----------------------------------------------------------------------------

    pub fn look(&self, yaw: f32, pitch: f32) {
        self.with_state_mut(|s| {
            s.body.yaw = mc_core::math::mth::wrap_degrees_f32(yaw);
            s.body.pitch = pitch.clamp(-90.0, 90.0);
        });
    }

    /// Face a point (`Entity.lookAt` from the eyes).
    pub fn look_at(&self, target: Vec3) {
        self.with_state_mut(|s| {
            let eye = s.body.eye_position();
            let (dx, dy, dz) = (target.x - eye.x, target.y - eye.y, target.z - eye.z);
            s.body.yaw = mc_core::math::mth::wrap_degrees_f32((dz.atan2(dx) * 57.29577951308232) as f32 - 90.0);
            s.body.pitch = mc_core::math::mth::wrap_degrees_f32((-(dy.atan2(dx.hypot(dz))) * 57.29577951308232) as f32);
        });
    }

    /// Walk (auto-jumping over obstacles) to within `radius` of `(x, z)`; fails with `Timeout` when stuck/slow.
    pub async fn walk_to(&self, x: f64, z: f64, radius: f64, sprint: bool, timeout: Duration) -> Result<()> {
        let ticks = (timeout.as_millis() / 50) as u32;
        self.with_state_mut(|s| {
            s.control.goal_failed = false;
            s.control.goal = Some(WalkGoal { x, z, radius, sprint, ticks_left: ticks });
        });
        self.wait_for(timeout + Duration::from_secs(5), |s| s.control.goal.is_none().then_some(())).await?;
        let failed = self.with_state_mut(|s| std::mem::take(&mut s.control.goal_failed));
        if failed { Err(BotError::Timeout(format!("walk_to({x}, {z}) did not arrive"))) } else { Ok(()) }
    }

    /// Release all movement keys and drop the walk goal.
    pub fn stop(&self) {
        self.with_state_mut(|s| {
            s.control.goal = None;
            s.control.input = Default::default();
        });
    }

    pub fn set_sneaking(&self, on: bool) {
        self.with_state_mut(|s| s.control.input.shift = on);
    }

    pub fn set_sprinting(&self, on: bool) {
        self.with_state_mut(|s| s.control.input.sprint = on);
    }

    /// Hold the movement keys (`forward, back, left, right`).
    pub fn set_move_keys(&self, forward: bool, back: bool, left: bool, right: bool) {
        self.with_state_mut(|s| {
            let i = &mut s.control.input;
            (i.forward, i.backward, i.left, i.right) = (forward, back, left, right);
        });
    }

    /// Press the jump key for `ticks` ticks (1 = a single jump).
    pub fn jump(&self, ticks: u32) {
        self.with_state_mut(|s| s.control.jump_ticks = ticks);
    }

    /// Toggle creative flight like the real client (needs `may_fly`); tells the server.
    pub fn set_flying(&self, flying: bool) {
        let ok = self.with_state_mut(|s| {
            let ok = s.body.abilities.may_fly;
            if ok {
                s.body.abilities.flying = flying;
            }
            ok
        });
        if ok {
            self.send(ServerboundPlayPacket::PlayerAbilities(ServerboundPlayerAbilitiesPacket { is_flying: flying }));
        }
    }

    // ---- world interaction --------------------------------------------------------------------

    pub fn block_at(&self, pos: BlockPos) -> Option<BlockState> {
        self.with_state(|s| s.world.block_state_at(pos))
    }

    /// Wait for a server `block_update`/`section_blocks_update` of `pos` after `since_tick` (use [`Self::tick`]).
    pub async fn wait_block_update(&self, pos: BlockPos, since_tick: u64, timeout: Duration) -> Result<BlockState> {
        self.wait_for(timeout, |s| s.block_updates.iter().rev().find(|u| u.pos == pos && u.tick >= since_tick).map(|u| u.state)).await
    }

    pub fn tick(&self) -> u64 {
        self.with_state(|s| s.tick)
    }

    /// Start (and for creative: finish) breaking a block; use [`Self::dig_block`] to wait for the result.
    pub fn start_digging(&self, pos: BlockPos, face: Direction) {
        let mut out = Out::new();
        self.with_state_mut(|s| {
            let tool = s.hand_tool;
            s.start_dig(pos, face, tool, &mut out)
        });
        for p in &out {
            self.shared.send(p);
        }
    }

    /// Stop breaking (`ABORT_DESTROY_BLOCK`).
    pub fn abort_digging(&self) {
        let mut out = Out::new();
        self.with_state_mut(|s| s.abort_dig(&mut out));
        for p in &out {
            self.shared.send(p);
        }
    }

    /// Break a block like the client does (progress per tick, STOP when done) and wait until the *server*
    /// confirmed the block is gone (`block_update` to air).
    pub async fn dig_block(&self, pos: BlockPos, face: Direction, timeout: Duration) -> Result<()> {
        let since = self.tick();
        self.look_at(Vec3::new(pos.x() as f64 + 0.5, pos.y() as f64 + 0.5, pos.z() as f64 + 0.5));
        self.start_digging(pos, face);
        let state = self.wait_block_update(pos, since, timeout).await?;
        if state.block().is_air() { Ok(()) } else { Err(BotError::Protocol(format!("block at {pos:?} became {state}"))) }
    }

    /// `use_item_on` with an exact cursor (`click` is the offset inside the clicked block, 0..1).
    pub fn use_item_on(&self, hand: InteractionHand, support: BlockPos, face: Direction, click: (f32, f32, f32), inside: bool) -> i32 {
        let sequence = self.with_state_mut(|s| s.next_sequence());
        self.send(ServerboundPlayPacket::UseItemOn(ServerboundUseItemOnPacket {
            hand,
            hit_result: BlockHitResult {
                block_pos: support,
                direction: face,
                click_x: click.0,
                click_y: click.1,
                click_z: click.2,
                inside,
                world_border_hit: false,
            },
            sequence,
        }));
        sequence
    }

    /// Place the held block so that it ends up at `target`: clicks the centre of the face of a solid neighbour.
    pub async fn place_block(&self, target: BlockPos, hand: InteractionHand, timeout: Duration) -> Result<BlockState> {
        let (support, face) = self
            .with_state(|s| {
                Direction::VALUES.iter().find_map(|&d| {
                    let n = target.relative(d);
                    let st = s.world.block_state_at(n)?;
                    (!st.block().is_air() && !st.can_be_replaced() && st.get_fluid_state().data().is_empty)
                        .then_some((n, d.get_opposite()))
                })
            })
            .ok_or_else(|| BotError::Protocol(format!("no solid neighbour to place against at {target:?}")))?;
        let cursor = (
            0.5 + 0.5 * face.get_step_x() as f32,
            0.5 + 0.5 * face.get_step_y() as f32,
            0.5 + 0.5 * face.get_step_z() as f32,
        );
        self.look_at(Vec3::new(support.x() as f64 + cursor.0 as f64, support.y() as f64 + cursor.1 as f64, support.z() as f64 + cursor.2 as f64));
        let since = self.tick();
        self.use_item_on(hand, support, face, cursor, false);
        self.wait_block_update(target, since, timeout).await
    }

    pub fn use_item(&self, hand: InteractionHand) {
        let (sequence, yaw, pitch) = self.with_state_mut(|s| (s.next_sequence(), s.body.yaw, s.body.pitch));
        self.send(ServerboundPlayPacket::UseItem(ServerboundUseItemPacket { hand, sequence, y_rot: yaw, x_rot: pitch }));
    }

    pub fn attack(&self, entity_id: i32) {
        self.send(ServerboundPlayPacket::Attack(ServerboundAttackPacket { entity_id }));
    }

    /// `interact` with an entity (`location` relative to the entity).
    pub fn interact(&self, entity_id: i32, hand: InteractionHand, location: Vec3) {
        let sneaking = self.with_state(|s| s.control.input.shift);
        self.send(ServerboundPlayPacket::Interact(ServerboundInteractPacket {
            entity_id,
            hand,
            location,
            using_secondary_action: sneaking,
        }));
    }

    pub fn respawn(&self) {
        self.send(ServerboundPlayPacket::ClientCommand(ServerboundClientCommandPacket {
            action: ServerboundClientCommandAction::PerformRespawn,
        }));
    }

    pub fn request_stats(&self) {
        self.send(ServerboundPlayPacket::ClientCommand(ServerboundClientCommandPacket {
            action: ServerboundClientCommandAction::RequestStats,
        }));
    }

    /// Drop the held item (`all` = whole stack, like Ctrl+Q).
    pub fn drop_held(&self, all: bool) {
        self.send(ServerboundPlayPacket::PlayerAction(ServerboundPlayerActionPacket {
            action: if all { ServerboundPlayerActionAction::DropAllItems } else { ServerboundPlayerActionAction::DropItem },
            pos: BlockPos::new(0, 0, 0),
            direction: Direction::Down,
            sequence: 0,
        }));
    }
}

impl Drop for Bot {
    fn drop(&mut self) {
        let _ = self.stop.send(true);
    }
}

async fn write_payload(conn: &mut Conn, payload: &[u8]) -> std::result::Result<(), NetError> {
    conn.writer.write_payload(payload).await?;
    conn.writer.flush().await
}

/// The connection task: owns the socket, dispatches packets, runs the client tick.
async fn run(
    shared: Arc<Shared>,
    mut conn: Conn,
    mut out_rx: mpsc::UnboundedReceiver<Vec<u8>>,
    mut stop: watch::Receiver<bool>,
    options: BotOptions,
) {
    let mut ticker = interval(Duration::from_millis(50));
    ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);
    let result: Result<()> = async {
        loop {
            tokio::select! {
                biased;
                _ = stop.changed() => return Ok(()),
                Some(payload) = out_rx.recv() => {
                    write_payload(&mut conn, &payload).await?;
                }
                payload = conn.reader.read_payload() => {
                    let payload = payload.map_err(|e| match e { NetError::Eof => BotError::Closed, e => e.into() })?;
                    match ClientboundPlayPacket::decode_payload(&payload) {
                        Ok(packet) => {
                            log_packet(&shared.log, Dir::In, &packet, &payload);
                            let mut out = Out::new();
                            let reconfigure = {
                                let mut st = shared.lock();
                                st.on_packet(packet, &mut out);
                                std::mem::take(&mut st.reconfigure_requested)
                            };
                            for p in &out {
                                shared.send(p);
                            }
                            shared.wake();
                            if reconfigure {
                                reconfigure_phase(&shared, &mut conn, &options, &mut out_rx).await?;
                            }
                        }
                        Err(e) => {
                            log_undecodable(&shared.log, ClientboundPlayPacket::STATE, ClientboundPlayPacket::FLOW, &payload, &e.to_string());
                            return Err(e.into());
                        }
                    }
                }
                _ = ticker.tick() => {
                    let mut out = Out::new();
                    shared.lock().client_tick(&mut out);
                    for p in &out {
                        shared.send(p);
                    }
                    shared.wake();
                }
            }
            if shared.lock().disconnected().is_some() {
                return Ok(());
            }
        }
    }
    .await;
    let reason = match &result {
        Ok(()) => None,
        Err(BotError::Closed) => Some("connection closed by server".to_string()),
        Err(e) => Some(e.to_string()),
    };
    {
        let mut st = shared.lock();
        if let Some(r) = reason {
            st.disconnect(r);
        } else if !matches!(st.phase, Phase::Disconnected(_)) {
            st.disconnect("client closed the connection");
        }
    }
    // flush queued packets (e.g. the final ones) before dropping the socket
    while let Ok(payload) = out_rx.try_recv() {
        if write_payload(&mut conn, &payload).await.is_err() {
            break;
        }
    }
    shared.wake();
}

/// Server-initiated return to the configuration state (`start_configuration`), then back into play.
async fn reconfigure_phase(
    shared: &Arc<Shared>,
    conn: &mut Conn,
    options: &BotOptions,
    out_rx: &mut mpsc::UnboundedReceiver<Vec<u8>>,
) -> Result<()> {
    use mc_protocol::packets::play::ServerboundConfigurationAcknowledgedPacket;
    // everything queued before the switch still goes out in the play protocol
    while let Ok(payload) = out_rx.try_recv() {
        write_payload(conn, &payload).await?;
    }
    shared.send(&ServerboundPlayPacket::ConfigurationAcknowledged(ServerboundConfigurationAcknowledgedPacket));
    if let Ok(payload) = out_rx.try_recv() {
        write_payload(conn, &payload).await?;
    }
    let mut configured = shared.lock().configured.clone();
    configured.transfer = None;
    join::configure(conn, options, &mut configured).await?;
    let mut st = shared.lock();
    st.configured = configured;
    if let Some((host, port)) = st.configured.transfer.clone() {
        st.transfer = Some((host.clone(), port));
        st.disconnect(format!("transfer to {host}:{port}"));
    } else {
        st.phase = Phase::Joining;
    }
    let _ = short_name;
    Ok(())
}
