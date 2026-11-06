//! The scriptable bot: connection task (20 TPS client tick loop, packet dispatch) and the actions API.
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use mc_core::direction::Direction;
use mc_core::phys::Vec3;
use mc_core::pos::block_pos::BlockPos;
use mc_data::block::BlockState;
use mc_protocol::net::NetError;
use mc_protocol::packets::play::*;
use tokio::sync::{Notify, mpsc, watch};
use tokio::task::JoinHandle;
use tokio::time::{Instant, MissedTickBehavior, interval};

use crate::conn::{Conn, SharedLog, WirePacket, log_packet, log_undecodable};
use crate::error::{BotError, Result};
use crate::join::{self, BotOptions};
use crate::log::{Dir, PacketLog};
use crate::state::{Out, Phase, State, WalkGoal};
use mc_protocol::components::ItemStack;
use mc_protocol::packets::play::{ClientboundPlayPacket, ServerboundPlayPacket};

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

    /// Encode, log and queue a packet for the connection task. A packet the codec refuses (vanilla's
    /// `EncoderException` while sending) is recorded in the packet log and not sent.
    pub fn send(&self, packet: &ServerboundPlayPacket) {
        match crate::conn::encode(packet) {
            Ok(payload) => {
                log_packet(&self.log, Dir::Out, packet, &payload);
                let _ = self.out.send(payload);
            }
            Err(e) => {
                if let Ok(mut l) = self.log.lock() {
                    l.record(
                        Dir::Out,
                        "play",
                        packet.id(),
                        crate::conn::short_name(packet.name()),
                        format!("<encode error: {e}>"),
                        &[],
                    );
                }
            }
        }
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
        log.lock()
            .map_err(|_| BotError::Protocol("log poisoned".into()))?
            .record_raw = options.record_raw;
        let mut conn = join::open(&options, log.clone()).await?;
        let configured = join::login_and_configure(&mut conn, &options).await?;
        install_registry_context(&configured);
        if let Some((host, port)) = &configured.transfer {
            return Err(BotError::Protocol(format!(
                "server requested a transfer to {host}:{port} during configuration"
            )));
        }
        let (out_tx, out_rx) = mpsc::unbounded_channel();
        let shared = Arc::new(Shared {
            state: Mutex::new(State::new(configured)),
            log,
            notify: Notify::new(),
            out: out_tx,
        });
        let (stop_tx, stop_rx) = watch::channel(false);
        let task = tokio::spawn(run(shared.clone(), conn, out_rx, stop_rx, options.clone()));
        Ok(Bot {
            shared,
            options,
            stop: stop_tx,
            task: Mutex::new(Some(task)),
        })
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
        self.shared
            .log
            .lock()
            .map(|l| l.to_text())
            .unwrap_or_default()
    }

    /// The last `n` packets of the log (for failure messages).
    pub fn log_tail(&self, n: usize) -> String {
        let Ok(l) = self.shared.log.lock() else {
            return String::new();
        };
        let e = l.entries();
        e[e.len().saturating_sub(n)..]
            .iter()
            .map(|x| x.line().chars().take(240).collect::<String>() + "\n")
            .collect()
    }

    pub fn write_log(&self, path: impl AsRef<std::path::Path>) -> std::io::Result<()> {
        self.shared
            .log
            .lock()
            .map_err(|_| std::io::Error::other("log poisoned"))?
            .write_jsonl(path)
    }

    /// Wait until `f` returns `Some`, the connection ends (`Err(Disconnected)`) or `timeout` passes.
    pub async fn wait_for<T>(
        &self,
        timeout: Duration,
        mut f: impl FnMut(&State) -> Option<T>,
    ) -> Result<T> {
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
        self.wait_for(timeout, |s| s.level_load.player_loaded_sent.then_some(()))
            .await
    }

    pub async fn wait_ticks(&self, ticks: u64) -> Result<()> {
        let target = self.with_state(|s| s.tick) + ticks;
        self.wait_for(Duration::from_millis(ticks * 200 + 5000), |s| {
            (s.tick >= target).then_some(())
        })
        .await
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
            s.body.yaw = mc_core::math::mth::wrap_degrees_f32(
                (dz.atan2(dx) * 57.29577951308232) as f32 - 90.0,
            );
            s.body.pitch = mc_core::math::mth::wrap_degrees_f32(
                (-(dy.atan2(dx.hypot(dz))) * 57.29577951308232) as f32,
            );
        });
    }

    /// Walk (auto-jumping over obstacles) to within `radius` of `(x, z)`; fails with `Timeout` when stuck/slow.
    pub async fn walk_to(
        &self,
        x: f64,
        z: f64,
        radius: f64,
        sprint: bool,
        timeout: Duration,
    ) -> Result<()> {
        let ticks = (timeout.as_millis() / 50) as u32;
        self.with_state_mut(|s| {
            s.control.goal_failed = false;
            s.control.goal = Some(WalkGoal {
                x,
                z,
                radius,
                sprint,
                ticks_left: ticks,
            });
        });
        self.wait_for(timeout + Duration::from_secs(5), |s| {
            s.control.goal.is_none().then_some(())
        })
        .await?;
        let failed = self.with_state_mut(|s| std::mem::take(&mut s.control.goal_failed));
        if failed {
            Err(BotError::Timeout(format!(
                "walk_to({x}, {z}) did not arrive"
            )))
        } else {
            Ok(())
        }
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
            self.send(ServerboundPlayPacket::PlayerAbilities(
                ServerboundPlayerAbilitiesPacket { is_flying: flying },
            ));
        }
    }

    // ---- world interaction --------------------------------------------------------------------

    pub fn block_at(&self, pos: BlockPos) -> Option<BlockState> {
        self.with_state(|s| s.world.block_state_at(pos))
    }

    /// Wait for a server `block_update`/`section_blocks_update` of `pos` after `since_tick` (use [`Self::tick`]).
    pub async fn wait_block_update(
        &self,
        pos: BlockPos,
        since_tick: u64,
        timeout: Duration,
    ) -> Result<BlockState> {
        self.wait_for(timeout, |s| {
            s.block_updates
                .iter()
                .rev()
                .find(|u| u.pos == pos && u.tick >= since_tick)
                .map(|u| u.state)
        })
        .await
    }

    pub fn tick(&self) -> u64 {
        self.with_state(|s| s.tick)
    }

    /// Start (and for creative: finish) breaking a block; use [`Self::dig_block`] to wait for the result.
    pub fn start_digging(&self, pos: BlockPos, face: Direction) {
        let mut out = Out::new();
        self.with_state_mut(|s| s.start_dig(pos, face, &mut out));
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
        self.look_at(Vec3::new(
            pos.x() as f64 + 0.5,
            pos.y() as f64 + 0.5,
            pos.z() as f64 + 0.5,
        ));
        self.start_digging(pos, face);
        self.wait_for(timeout, |s| {
            s.block_updates
                .iter()
                .rev()
                .find(|u| u.pos == pos && u.tick >= since && u.state.block().is_air())
                .map(|_| ())
        })
        .await
    }

    /// `use_item_on` with an exact cursor (`click` is the offset inside the clicked block, 0..1).
    pub fn use_item_on(
        &self,
        hand: InteractionHand,
        support: BlockPos,
        face: Direction,
        click: (f32, f32, f32),
        inside: bool,
    ) -> i32 {
        let sequence = self.with_state_mut(|s| s.next_sequence());
        self.send(ServerboundPlayPacket::UseItemOn(
            ServerboundUseItemOnPacket {
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
            },
        ));
        sequence
    }

    /// Place the held block so that it ends up at `target`: clicks the centre of the face of a solid neighbour.
    pub async fn place_block(
        &self,
        target: BlockPos,
        hand: InteractionHand,
        timeout: Duration,
    ) -> Result<BlockState> {
        let (support, face) = self
            .with_state(|s| {
                Direction::VALUES.iter().find_map(|&d| {
                    let n = target.relative(d);
                    let st = s.world.block_state_at(n)?;
                    (!st.block().is_air()
                        && !st.can_be_replaced()
                        && !st.collision_shape().is_empty()
                        && st.get_fluid_state().data().is_empty)
                        .then_some((n, d.get_opposite()))
                })
            })
            .ok_or_else(|| {
                BotError::Protocol(format!("no solid neighbour to place against at {target:?}"))
            })?;
        let cursor = (
            0.5 + 0.5 * face.get_step_x() as f32,
            0.5 + 0.5 * face.get_step_y() as f32,
            0.5 + 0.5 * face.get_step_z() as f32,
        );
        self.look_at(Vec3::new(
            support.x() as f64 + cursor.0 as f64,
            support.y() as f64 + cursor.1 as f64,
            support.z() as f64 + cursor.2 as f64,
        ));
        let since = self.tick();
        self.use_item_on(hand, support, face, cursor, false);
        self.wait_block_update(target, since, timeout).await
    }

    pub fn use_item(&self, hand: InteractionHand) {
        let (sequence, yaw, pitch) =
            self.with_state_mut(|s| (s.next_sequence(), s.body.yaw, s.body.pitch));
        self.send(ServerboundPlayPacket::UseItem(ServerboundUseItemPacket {
            hand,
            sequence,
            y_rot: yaw,
            x_rot: pitch,
        }));
    }

    pub fn attack(&self, entity_id: i32) {
        self.send(ServerboundPlayPacket::Attack(ServerboundAttackPacket {
            entity_id,
        }));
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

    /// `ServerboundSelectTradePacket` (the merchant menu must be open: interact with the villager first).
    pub fn select_trade(&self, index: i32) {
        self.send(ServerboundPlayPacket::SelectTrade(
            ServerboundSelectTradePacket { item: index },
        ));
    }

    /// Ids of the tracked entities of `entity_type` (ascending id = spawn order on both servers).
    pub fn entities_of(&self, entity_type: mc_data::EntityType) -> Vec<i32> {
        self.with_state(|s| {
            let mut ids: Vec<i32> = s
                .entities
                .values()
                .filter(|e| e.entity_type == entity_type)
                .map(|e| e.id)
                .collect();
            ids.sort_unstable();
            ids
        })
    }

    /// Wait for an entity of `entity_type` whose id is not in `known` (an `add_entity` packet after the action that should spawn it).
    pub async fn wait_entity(
        &self,
        entity_type: mc_data::EntityType,
        known: &[i32],
        timeout: Duration,
    ) -> Result<i32> {
        self.wait_for(timeout, |s| {
            let mut ids: Vec<i32> = s
                .entities
                .values()
                .filter(|e| e.entity_type == entity_type && !known.contains(&e.id))
                .map(|e| e.id)
                .collect();
            ids.sort_unstable();
            ids.first().copied()
        })
        .await
    }

    /// One observation line per tracked entity (ids are not stable across servers: lines are sorted by id and identified by type/index).
    /// Format: `ENT type#n pos=x,y,z vel=x,y,z rot=yaw,pitch head=yaw og=0|1 vehicle=- data=ID:VALUE;...` (entity metadata by accessor id).
    pub fn entity_lines(&self) -> Vec<String> {
        self.with_state(|s| {
            let own = s.entity_id;
            let mut ents: Vec<&crate::tracking::TrackedEntity> = s.entities.values().filter(|e| e.id != own).collect();
            ents.sort_by_key(|e| e.id);
            let mut counter: std::collections::BTreeMap<&'static str, usize> = Default::default();
            ents.iter()
                .map(|e| {
                    let name = e.entity_type.name();
                    let n = counter.entry(name).or_insert(0);
                    *n += 1;
                    let data: Vec<String> = e.data.iter().map(|(k, v)| format!("{k}:{v:?}")).collect();
                    format!(
                        "ENT {name}#{n} pos={:?},{:?},{:?} vel={:?},{:?},{:?} rot={:?},{:?} head={:?} og={} vehicle={} data={}",
                        e.pos.x, e.pos.y, e.pos.z, e.vel.x, e.vel.y, e.vel.z, e.yaw, e.pitch, e.head_yaw, u8::from(e.on_ground),
                        e.vehicle.map_or("-".to_string(), |v| v.to_string()),
                        if data.is_empty() { "-".to_string() } else { data.join(";") }
                    )
                })
                .collect()
        })
    }

    /// `OFFER i buyA=item*count buyB=item*count|- sell=item*count uses=U max=M xp=X mult=F demand=D special=S` per offer of the last
    /// `merchant_offers` packet (plus a `OFFERS level= xp=` header); empty when no trading screen was opened.
    pub fn merchant_offer_lines(&self) -> Vec<String> {
        self.with_state(|s| {
            let Some(p) = &s.merchant_offers else { return Vec::new() };
            let mut out = vec![format!("OFFERS count={} level={} xp={} progress={} restock={}", p.offers.len(), p.villager_level, p.villager_xp, u8::from(p.show_progress), u8::from(p.can_restock))];
            for (i, o) in p.offers.iter().enumerate() {
                let cost = |c: &mc_protocol::packets::play::ItemCost| format!("{}*{}", c.item.name(), c.count);
                out.push(format!(
                    "OFFER {i} buyA={} buyB={} sell={}*{} uses={} max={} xp={} mult={:?} demand={} special={}",
                    cost(&o.base_cost_a),
                    o.cost_b.as_ref().map_or("-".to_string(), cost),
                    o.result.item().name(),
                    o.result.count(),
                    o.uses, o.max_uses, o.xp, o.price_multiplier, o.demand, o.special_price_diff
                ));
            }
            out
        })
    }

    // ---- chat / commands ----------------------------------------------------------------------

    fn epoch_millis() -> i64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_millis() as i64)
    }

    fn last_seen_update(&self) -> LastSeenMessagesUpdate {
        let offset = self.with_state_mut(|s| std::mem::take(&mut s.pending_chat_acks));
        LastSeenMessagesUpdate {
            offset,
            acknowledged: Default::default(),
            checksum: 0,
        }
    }

    /// Unsigned chat message (the server must run with `enforce-secure-profile=false`).
    pub fn chat(&self, message: &str) {
        let last_seen = self.last_seen_update();
        self.send(ServerboundPlayPacket::Chat(ServerboundChatPacket {
            message: message.to_string(),
            time_stamp: mc_protocol::codec::Instant(Self::epoch_millis()),
            salt: 0,
            signature: None,
            last_seen_messages: last_seen,
        }));
    }

    /// `chat_command` (no leading slash).
    pub fn command(&self, command: &str) {
        self.send(ServerboundPlayPacket::ChatCommand(
            ServerboundChatCommandPacket {
                command: command.trim_start_matches('/').to_string(),
            },
        ));
    }

    /// `chat_command_signed` without argument signatures (what a client without a chat session sends).
    pub fn command_signed(&self, command: &str) {
        let last_seen = self.last_seen_update();
        self.send(ServerboundPlayPacket::ChatCommandSigned(
            ServerboundChatCommandSignedPacket {
                command: command.trim_start_matches('/').to_string(),
                time_stamp: mc_protocol::codec::Instant(Self::epoch_millis()),
                salt: 0,
                argument_signatures: ArgumentSignatures {
                    entries: Vec::new(),
                },
                last_seen_messages: last_seen,
            },
        ));
    }

    /// Wait for a chat/system message whose text contains `needle`, sent after `since` (index into `chat`).
    pub async fn wait_chat(&self, needle: &str, since: usize, timeout: Duration) -> Result<String> {
        self.wait_for(timeout, |s| {
            s.chat
                .iter()
                .skip(since)
                .find(|l| l.text.contains(needle))
                .map(|l| l.text.clone())
        })
        .await
    }

    pub fn chat_len(&self) -> usize {
        self.with_state(|s| s.chat.len())
    }

    /// Send a command and wait for feedback containing `needle`.
    pub async fn command_expect(
        &self,
        command: &str,
        needle: &str,
        timeout: Duration,
    ) -> Result<String> {
        let since = self.chat_len();
        self.command(command);
        self.wait_chat(needle, since, timeout).await
    }

    // ---- hands, hotbar, inventory ---------------------------------------------------------------

    /// Swing the main hand (`punch`).
    pub fn swing(&self) {
        self.send(ServerboundPlayPacket::Punch(ServerboundPunchPacket));
    }

    pub fn select_hotbar(&self, slot: u8) {
        let slot = slot.min(8);
        self.with_state_mut(|s| {
            s.selected_slot = slot as usize;
        });
        self.send(ServerboundPlayPacket::SetCarriedItem(
            ServerboundSetCarriedItemPacket { slot: slot as i16 },
        ));
    }

    /// Creative inventory: put `stack` into menu slot `slot` of the player inventory menu (36..=44 = hotbar).
    pub fn creative_set_slot(&self, slot: i16, stack: ItemStack) {
        self.with_state_mut(|s| {
            if let Some(x) = s
                .inventory
                .slots
                .get_mut(slot as usize)
                .filter(|_| slot >= 0)
            {
                *x = stack.clone();
            }
        });
        self.send(ServerboundPlayPacket::SetCreativeModeSlot(
            ServerboundSetCreativeModeSlotPacket {
                slot_num: slot,
                item_stack: stack,
            },
        ));
    }

    /// Container click with client-side prediction and hashed stacks (acts on the open menu, else the inventory).
    pub fn click_slot(&self, slot: i16, button: i8, input: ContainerInput) {
        let packet = self.with_state_mut(|s| {
            let p = match &mut s.open_menu {
                Some(m) => m.click(slot, button, input),
                None => s.inventory.click(slot, button, input),
            };
            s.mirror_menu_after_click();
            p
        });
        self.send(ServerboundPlayPacket::ContainerClick(packet));
    }

    /// Move a whole stack from one slot to another like drag-free left clicks (pick up, put down).
    pub fn move_item(&self, from: i16, to: i16) {
        self.click_slot(from, 0, ContainerInput::Pickup);
        self.click_slot(to, 0, ContainerInput::Pickup);
    }

    /// Close the open container (or the inventory screen: the real client sends `container_close` for id 0 too).
    pub fn close_container(&self) {
        let id = self.with_state_mut(|s| {
            let id = s.open_menu.take().map_or(0, |m| m.container_id);
            // a carried stack is dropped back into the inventory by the server; mirror nothing locally
            id
        });
        self.send(ServerboundPlayPacket::ContainerClose(
            ServerboundContainerClosePacket { container_id: id },
        ));
    }

    /// Contents of a menu slot (open menu if any, else the player inventory menu).
    pub fn slot(&self, slot: usize) -> Option<ItemStack> {
        self.with_state(|s| {
            s.open_menu
                .as_ref()
                .unwrap_or(&s.inventory)
                .slots
                .get(slot)
                .cloned()
        })
    }

    pub fn held_item(&self) -> ItemStack {
        self.with_state(|s| s.held_item().clone())
    }

    // ---- observation ---------------------------------------------------------------------------

    pub fn health(&self) -> f32 {
        self.with_state(|s| s.health)
    }

    pub fn food(&self) -> i32 {
        self.with_state(|s| s.food)
    }

    /// Own air supply (synched entity data), `None` until the server sent it.
    pub fn air(&self) -> Option<i32> {
        self.with_state(|s| s.air())
    }

    /// Mark in the [`Observed`] log; pass it to [`Bot::events_since`] / [`Bot::wait_event`] after an action.
    pub fn event_mark(&self) -> usize {
        self.with_state(|s| s.events.len())
    }

    pub fn events_since(&self, mark: usize) -> Vec<crate::state::Observed> {
        self.with_state(|s| s.events[mark.min(s.events.len())..].to_vec())
    }

    /// Wait for the first event after `mark` accepted by `f`.
    pub async fn wait_event<T>(
        &self,
        mark: usize,
        timeout: Duration,
        mut f: impl FnMut(&crate::state::Observed) -> Option<T>,
    ) -> Result<T> {
        self.wait_for(timeout, |s| {
            s.events[mark.min(s.events.len())..].iter().find_map(&mut f)
        })
        .await
    }

    pub fn respawn(&self) {
        self.send(ServerboundPlayPacket::ClientCommand(
            ServerboundClientCommandPacket {
                action: ServerboundClientCommandAction::PerformRespawn,
            },
        ));
    }

    pub fn request_stats(&self) {
        self.send(ServerboundPlayPacket::ClientCommand(
            ServerboundClientCommandPacket {
                action: ServerboundClientCommandAction::RequestStats,
            },
        ));
    }

    /// Drop the held item (`all` = whole stack, like Ctrl+Q).
    pub fn drop_held(&self, all: bool) {
        self.send(ServerboundPlayPacket::PlayerAction(
            ServerboundPlayerActionPacket {
                action: if all {
                    ServerboundPlayerActionAction::DropAllItems
                } else {
                    ServerboundPlayerActionAction::DropItem
                },
                pos: BlockPos::new(0, 0, 0),
                direction: Direction::Down,
                sequence: 0,
            },
        ));
    }
}

impl Drop for Bot {
    fn drop(&mut self) {
        let _ = self.stop.send(true);
    }
}

/// Use the registries the server synced for all data-driven registry ids on the wire
/// (first bot of the process wins; the context is process-global).
fn install_registry_context(configured: &join::Configured) {
    let mut ctx = mc_protocol::registry_context::RegistryContext::new();
    for r in &configured.registries {
        ctx = ctx.with_registry(
            &r.id.to_string(),
            r.entries.iter().map(|(n, _)| n.clone()).collect(),
        );
    }
    let _ = mc_protocol::registry_context::set(ctx);
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
    shared.send(&ServerboundPlayPacket::ConfigurationAcknowledged(
        ServerboundConfigurationAcknowledgedPacket,
    ));
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
    Ok(())
}
