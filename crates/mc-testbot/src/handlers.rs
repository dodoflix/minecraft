//! Packet handlers beyond the core join/movement set: entities, vitals, containers, chat, scoreboard, …
use mc_core::phys::Vec3;
use mc_data::{EntityType, MobEffect};
use mc_protocol::components::ItemStack;
use mc_protocol::packets::play::*;

use crate::inventory::{Menu, PLAYER_MENU};
use crate::state::{ChatLine, Observed, Out, State};
use crate::tracking::*;

fn text(c: &mc_text::Component) -> String {
    c.get_string()
}

/// The component as JSON (styles, click/hover events, translation keys kept).
fn json(c: &mc_text::Component) -> String {
    c.to_json_string().unwrap_or_default()
}

impl State {
    pub(crate) fn on_other(&mut self, packet: ClientboundPlayPacket, _out: &mut Out) {
        use ClientboundPlayPacket as P;
        match packet {
            // ---- entities ----
            P::AddEntity(p) => {
                let mut e = TrackedEntity::new(p.id, p.uuid, p.r#type, Vec3::new(p.x, p.y, p.z));
                e.vel = p.movement.0;
                e.pitch = unpack_degrees(p.x_rot);
                e.yaw = unpack_degrees(p.y_rot);
                e.head_yaw = unpack_degrees(p.y_head_rot);
                self.entities.insert(p.id, e);
            }
            P::RemoveEntities(p) => {
                for id in p.entity_ids {
                    self.entities.remove(&id);
                }
            }
            P::MoveEntityPos(p) => self.move_entity(p.entity_id, &p.delta, None, p.on_ground),
            P::MoveEntityPosRot(p) => {
                self.move_entity(p.entity_id, &p.delta, Some((unpack_degrees(p.y_rot), unpack_degrees(p.x_rot))), p.on_ground)
            }
            P::MoveEntityRot(p) => {
                if let Some(e) = self.entities.get_mut(&p.entity_id) {
                    e.yaw = unpack_degrees(p.y_rot);
                    e.pitch = unpack_degrees(p.x_rot);
                    e.on_ground = p.on_ground;
                }
            }
            P::TeleportEntity(p) => {
                if p.id == self.entity_id {
                    return;
                }
                if let Some(e) = self.entities.get_mut(&p.id) {
                    let rel = |r: Relative| p.relatives.contains(r);
                    let c = &p.change;
                    e.pos = Vec3::new(
                        if rel(Relative::X) { e.pos.x + c.position.x } else { c.position.x },
                        if rel(Relative::Y) { e.pos.y + c.position.y } else { c.position.y },
                        if rel(Relative::Z) { e.pos.z + c.position.z } else { c.position.z },
                    );
                    e.vel = c.delta_movement;
                    e.yaw = if rel(Relative::YRot) { e.yaw + c.y_rot } else { c.y_rot };
                    e.pitch = if rel(Relative::XRot) { e.pitch + c.x_rot } else { c.x_rot };
                    e.on_ground = p.on_ground;
                }
            }
            P::EntityPositionSync(p) => {
                if let Some(e) = self.entities.get_mut(&p.id) {
                    e.pos = match &p.position {
                        PositionPath::Linear(v) => *v,
                        PositionPath::Stepped(steps) => steps.last().map_or(e.pos, |s| s.position),
                    };
                    e.yaw = p.y_rot;
                    e.pitch = p.x_rot;
                    e.on_ground = p.on_ground;
                }
            }
            P::SetEntityMotion(p) => {
                if p.id == self.entity_id {
                    self.body.vel = p.movement.0;
                } else if let Some(e) = self.entities.get_mut(&p.id) {
                    e.vel = p.movement.0;
                }
            }
            P::RotateHead(p) => {
                if let Some(e) = self.entities.get_mut(&p.entity_id) {
                    e.head_yaw = unpack_degrees(p.y_head_rot);
                }
            }
            P::SetEntityData(p) => {
                let own = p.id == self.entity_id;
                let e = self.entities.entry(p.id).or_insert_with(|| {
                    TrackedEntity::new(p.id, uuid::Uuid::nil(), EntityType::PLAYER, Vec3::ZERO)
                });
                for v in p.packed_items {
                    e.data.insert(v.id, v.value);
                }
                let _ = own;
            }
            P::SetPassengers(p) => {
                for e in self.entities.values_mut() {
                    if e.vehicle == Some(p.vehicle) {
                        e.vehicle = None;
                    }
                }
                for id in &p.passengers {
                    if let Some(e) = self.entities.get_mut(id) {
                        e.vehicle = Some(p.vehicle);
                    }
                }
                if let Some(v) = self.entities.get_mut(&p.vehicle) {
                    v.passengers = p.passengers.clone();
                }
            }
            P::UpdateMobEffect(p) => {
                if p.entity_id == self.entity_id {
                    let amp = p.effect_amplifier + 1;
                    if p.effect == MobEffect::JUMP_BOOST {
                        self.body.jump_boost = amp;
                    } else if p.effect == MobEffect::SPEED {
                        self.body.speed_effect = amp;
                    } else if p.effect == MobEffect::SLOWNESS {
                        self.body.slow_effect = amp;
                    }
                    self.effects.retain(|(e, _)| *e != p.effect);
                    self.effects.push((p.effect, amp));
                }
            }
            P::RemoveMobEffect(p) => {
                if p.entity_id == self.entity_id {
                    if p.effect == MobEffect::JUMP_BOOST {
                        self.body.jump_boost = 0;
                    } else if p.effect == MobEffect::SPEED {
                        self.body.speed_effect = 0;
                    } else if p.effect == MobEffect::SLOWNESS {
                        self.body.slow_effect = 0;
                    }
                    self.effects.retain(|(e, _)| *e != p.effect);
                }
            }
            P::Animate(_) | P::SwingAnimation(_) => {}
            P::UpdateAttributes(p) => {
                if p.entity_id == self.entity_id {
                    for snap in p.attributes {
                        self.attributes.insert(snap.attribute, (snap.base, snap.modifiers));
                    }
                }
            }
            P::EntityEvent(p) => self.events.push(Observed::EntityEvent { tick: self.tick, entity_id: p.entity_id, event_id: p.event_id }),
            P::HurtAnimation(p) => self.events.push(Observed::HurtAnimation { tick: self.tick, entity_id: p.id }),
            P::DamageEvent(p) => self.events.push(Observed::Damage {
                tick: self.tick,
                entity_id: p.entity_id,
                source_type: p.source_type,
                cause_id: p.source_cause_id,
                direct_id: p.source_direct_id,
            }),
            P::PlayerCombatKill(p) => {
                if p.player_id == self.entity_id {
                    self.dead = true;
                }
                self.events.push(Observed::Death { tick: self.tick, player_id: p.player_id, message: p.message.get_string() });
            }
            P::Explode(p) => self.events.push(Observed::Explosion {
                tick: self.tick,
                center: p.center,
                radius: p.radius,
                block_count: p.block_count,
                knockback: p.player_knockback,
            }),
            P::TakeItemEntity(p) => self.events.push(Observed::TakeItem { tick: self.tick, item_id: p.item_id, collector_id: p.player_id, amount: p.amount }),

            // ---- vitals ----
            P::SetHealth(p) => {
                self.health = p.health;
                self.food = p.food;
                self.saturation = p.saturation;
                self.dead = p.health <= 0.0;
            }
            P::SetExperience(p) => {
                self.xp_progress = p.experience_progress;
                self.xp_level = p.experience_level;
                self.xp_total = p.total_experience;
            }
            P::SetHeldSlot(p) => {
                if (0..9).contains(&p.slot) {
                    self.selected_slot = p.slot as usize;
                }
            }
            P::ChangeDifficulty(p) => self.difficulty = Some(p.difficulty),

            // ---- containers ----
            P::OpenScreen(p) => {
                let n = menu_slot_count(p.menu_type.name());
                let mut m = Menu::new(p.container_id, n);
                m.title = Some(text(&p.title));
                m.menu_type = Some(p.menu_type.name().to_string());
                self.open_menu = Some(m);
            }
            P::MerchantOffers(p) => self.merchant_offers = Some(p),
            P::ContainerClose(p) => {
                if self.open_menu.as_ref().is_some_and(|m| m.container_id == p.container_id) {
                    self.open_menu = None;
                }
            }
            P::ContainerSetContent(p) => {
                if let Some(m) = self.menu_mut(p.container_id) {
                    m.state_id = p.state_id;
                    m.slots = p.items;
                    m.cursor = p.carried_item;
                    self.sync_player_menu_from(p.container_id);
                }
            }
            P::ContainerSetSlot(p) => {
                if let Some(m) = self.menu_mut(p.container_id) {
                    m.state_id = p.state_id;
                    if let Some(s) = m.slots.get_mut(p.slot as usize).filter(|_| p.slot >= 0) {
                        *s = p.item_stack;
                    }
                    self.sync_player_menu_from(p.container_id);
                }
            }
            P::ContainerSetData(p) => {
                if let Some(m) = self.menu_mut(p.container_id) {
                    m.data.insert(p.id, p.value);
                }
            }
            P::SetCursorItem(p) => {
                self.inventory.cursor = p.contents.clone();
                if let Some(m) = &mut self.open_menu {
                    m.cursor = p.contents;
                }
            }
            P::SetPlayerInventory(p) => {
                if let Some(s) = self.inventory.slot_of_inventory(p.slot) {
                    self.inventory.slots[s] = p.contents.clone();
                }
                if let Some(m) = &mut self.open_menu
                    && let Some(s) = m.slot_of_inventory(p.slot)
                {
                    m.slots[s] = p.contents;
                }
            }

            // ---- chat / titles ----
            P::PlayerChat(p) => {
                let content = p.unsigned_content.as_ref().map(text).unwrap_or(p.body.content.clone());
                if p.signature.is_some() {
                    self.pending_chat_acks += 1;
                }
                self.chat.push(ChatLine {
                    tick: self.tick,
                    kind: "player_chat",
                    text: content,
                    json: p.unsigned_content.as_ref().map(json).unwrap_or_default(),
                    overlay: false,
                    sender: Some(text(&p.chat_type.name)),
                });
            }
            P::DisguisedChat(p) => self.chat.push(ChatLine {
                tick: self.tick,
                kind: "disguised_chat",
                text: text(&p.message),
                json: json(&p.message),
                overlay: false,
                sender: Some(text(&p.chat_type.name)),
            }),
            P::SystemChat(p) => {
                self.chat.push(ChatLine { tick: self.tick, kind: "system_chat", text: text(&p.content), json: json(&p.content), overlay: p.overlay, sender: None })
            }
            P::ClearTitles(p) => {
                let t = &mut self.social.titles;
                t.title = None;
                t.subtitle = None;
                t.action_bar = None;
                if p.reset_times {
                    (t.fade_in, t.stay, t.fade_out) = (10, 70, 20);
                }
            }
            P::SetTitleText(p) => self.social.titles.title = Some(text(&p.text)),
            P::SetSubtitleText(p) => self.social.titles.subtitle = Some(text(&p.text)),
            P::SetActionBarText(p) => self.social.titles.action_bar = Some(text(&p.text)),
            P::SetTitlesAnimation(p) => {
                let t = &mut self.social.titles;
                (t.fade_in, t.stay, t.fade_out) = (p.fade_in, p.stay, p.fade_out);
            }
            P::TabList(p) => {
                self.social.tab_header = text(&p.header);
                self.social.tab_footer = text(&p.footer);
            }
            P::Commands(p) => {
                let root = usize::try_from(p.root_index).ok().and_then(|i| p.entries.get(i));
                self.commands = root
                    .map(|r| {
                        r.children
                            .iter()
                            .filter_map(|c| match &p.entries.get(*c as usize)?.stub {
                                Some(ClientboundCommandsNodeStub::Literal(n)) => Some(n.clone()),
                                _ => None,
                            })
                            .collect()
                    })
                    .unwrap_or_default();
            }

            // ---- tab list / scoreboard / boss bars ----
            P::PlayerInfoUpdate(p) => {
                use ClientboundPlayerInfoUpdateAction as A;
                for en in p.entries {
                    let info = self.social.players.entry(en.profile_id).or_insert_with(|| PlayerInfo {
                        uuid: en.profile_id,
                        name: String::new(),
                        listed: false,
                        latency: 0,
                        game_mode: GameType::Survival,
                        display_name: None,
                    });
                    if p.actions.contains(A::AddPlayer)
                        && let Some(profile) = &en.profile
                    {
                        info.name = profile.name.clone();
                    }
                    if p.actions.contains(A::UpdateGameMode) {
                        info.game_mode = en.game_mode;
                    }
                    if p.actions.contains(A::UpdateListed) {
                        info.listed = en.listed;
                    }
                    if p.actions.contains(A::UpdateLatency) {
                        info.latency = en.latency;
                    }
                    if p.actions.contains(A::UpdateDisplayName) {
                        info.display_name = en.display_name.as_ref().map(text);
                    }
                }
            }
            P::PlayerInfoRemove(p) => {
                for id in p.profile_ids {
                    self.social.players.remove(&id);
                }
            }
            P::BossEvent(p) => {
                use BossEventOperation as O;
                match p.operation {
                    O::Add { name, progress, .. } => {
                        self.social.bossbars.insert(p.id, BossBar { id: p.id, name: text(&name), progress });
                    }
                    O::Remove => {
                        self.social.bossbars.remove(&p.id);
                    }
                    O::UpdateProgress { progress } => {
                        if let Some(b) = self.social.bossbars.get_mut(&p.id) {
                            b.progress = progress;
                        }
                    }
                    O::UpdateName { name } => {
                        if let Some(b) = self.social.bossbars.get_mut(&p.id) {
                            b.name = text(&name);
                        }
                    }
                    O::UpdateStyle { .. } | O::UpdateProperties { .. } => {}
                }
            }
            P::SetObjective(p) => match p.method {
                ObjectiveMethod::Add(s) | ObjectiveMethod::Change(s) => {
                    self.social.objectives.insert(
                        p.objective_name.clone(),
                        Objective { name: p.objective_name, display_name: text(&s.display_name) },
                    );
                }
                ObjectiveMethod::Remove => {
                    self.social.objectives.remove(&p.objective_name);
                    self.social.scores.retain(|(_, o), _| *o != p.objective_name);
                }
                ObjectiveMethod::Other(_) => {}
            },
            P::SetScore(p) => {
                self.social.scores.insert((p.owner, p.objective_name), p.score);
            }
            P::ResetScore(p) => match p.objective_name {
                Some(o) => {
                    self.social.scores.remove(&(p.owner, o));
                }
                None => self.social.scores.retain(|(owner, _), _| *owner != p.owner),
            },
            P::SetDisplayObjective(p) => {
                let slot = format!("{:?}", p.slot);
                if p.objective_name.is_empty() {
                    self.social.display_slots.remove(&slot);
                } else {
                    self.social.display_slots.insert(slot, p.objective_name);
                }
            }
            P::SetPlayerTeam(p) => {
                let teams = &mut self.social.teams;
                match p.method {
                    TeamMethod::Add { parameters, players } => {
                        teams.insert(
                            p.name.clone(),
                            Team {
                                name: p.name,
                                display_name: text(&parameters.display_name),
                                prefix: text(&parameters.player_prefix),
                                suffix: text(&parameters.player_suffix),
                                members: players,
                            },
                        );
                    }
                    TeamMethod::Remove => {
                        teams.remove(&p.name);
                    }
                    TeamMethod::Change { parameters } => {
                        if let Some(t) = teams.get_mut(&p.name) {
                            t.display_name = text(&parameters.display_name);
                            t.prefix = text(&parameters.player_prefix);
                            t.suffix = text(&parameters.player_suffix);
                        }
                    }
                    TeamMethod::Join { players } => {
                        if let Some(t) = teams.get_mut(&p.name) {
                            t.members.extend(players);
                        }
                    }
                    TeamMethod::Leave { players } => {
                        if let Some(t) = teams.get_mut(&p.name) {
                            t.members.retain(|m| !players.contains(m));
                        }
                    }
                    TeamMethod::Other(_) => {}
                }
            }
            _ => {}
        }
    }

    fn move_entity(&mut self, id: i32, delta: &VecDelta, rot: Option<(f32, f32)>, on_ground: bool) {
        let Some(e) = self.entities.get_mut(&id) else { return };
        let (mut xa, mut ya, mut za) = (0i64, 0i64, 0i64);
        match delta {
            VecDelta::Linear { xa: x, ya: y, za: z } => (xa, ya, za) = (*x as i64, *y as i64, *z as i64),
            VecDelta::Stepped(steps) => {
                for s in steps {
                    xa += s.xa as i64;
                    ya += s.ya as i64;
                    za += s.za as i64;
                }
            }
        }
        // `VecDeltaCodec.decode`: axes with a zero delta keep the exact current coordinate
        let dec = |cur: f64, d: i64| if d == 0 { cur } else { ((cur * 4096.0).round() as i64 + d) as f64 / 4096.0 };
        e.pos = Vec3::new(dec(e.pos.x, xa), dec(e.pos.y, ya), dec(e.pos.z, za));
        if let Some((y, x)) = rot {
            e.yaw = y;
            e.pitch = x;
        }
        e.on_ground = on_ground;
    }

    fn menu_mut(&mut self, container_id: i32) -> Option<&mut Menu> {
        if container_id == PLAYER_MENU {
            return Some(&mut self.inventory);
        }
        self.open_menu.as_mut().filter(|m| m.container_id == container_id)
    }

    /// The player's inventory menu mirrors the tail of an open container and vice versa; keep both views of the
    /// 36 inventory slots equal after a full-content/slot update of either.
    fn sync_player_menu_from(&mut self, container_id: i32) {
        let Some(open) = &mut self.open_menu else { return };
        if container_id == PLAYER_MENU {
            for i in 0..36 {
                if let (Some(a), Some(b)) = (self.inventory.slot_of_inventory(i), open.slot_of_inventory(i)) {
                    open.slots[b] = self.inventory.slots[a].clone();
                }
            }
        } else if open.container_id == container_id {
            for i in 0..36 {
                if let (Some(a), Some(b)) = (self.inventory.slot_of_inventory(i), open.slot_of_inventory(i)) {
                    self.inventory.slots[a] = open.slots[b].clone();
                }
            }
        }
    }

    /// Stack currently held in the selected hotbar slot.
    pub fn held_item(&self) -> &ItemStack {
        self.inventory.hotbar(self.selected_slot)
    }
}

/// Slot count of a menu type (`MenuType` registry name) including the 36 player inventory slots.
pub fn menu_slot_count(menu_type: &str) -> usize {
    let name = menu_type.strip_prefix("minecraft:").unwrap_or(menu_type);
    let own = match name {
        "generic_9x1" => 9,
        "generic_9x2" => 18,
        "generic_9x3" | "shulker_box" => 27,
        "generic_9x4" => 36,
        "generic_9x5" => 45,
        "generic_9x6" => 54,
        "generic_3x3" | "crafter_3x3" => 9,
        "anvil" | "grindstone" | "cartography_table" | "merchant" | "smithing" => match name {
            "smithing" => 4,
            "merchant" | "anvil" | "grindstone" | "cartography_table" => 3,
            _ => 3,
        },
        "beacon" => 1,
        "blast_furnace" | "furnace" | "smoker" => 3,
        "brewing_stand" => 5,
        "crafting" => 10,
        "enchantment" => 2,
        "hopper" => 5,
        "lectern" => 1,
        "loom" => 4,
        "stonecutter" => 2,
        _ => 0,
    };
    own + 36
}
