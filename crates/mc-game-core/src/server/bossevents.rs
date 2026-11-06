//! `net.minecraft.server.bossevents` data model: `CustomBossEvents` (`data/minecraft/custom_boss_events.dat`) and
//! `CustomBossEvent` state + `Packed` codec. Player tracking packets (`ServerBossEvent.addPlayer`, `onPlayerConnect`)
//! belong to the player-list layer; this keeps the saved player UUID set and the bar state.

use std::collections::BTreeSet;

use indexmap::IndexMap;

use mc_core::ident::Identifier;
use mc_core::math::mth::{clamp_f32, create_insecure_uuid};
use mc_core::random::RandomSource;
use mc_core::uuid_util::{uuid_from_int_array, uuid_to_int_array};
use mc_nbt::{CompoundTag, ListTag, Tag};
use mc_protocol::packets::play as wire;
use mc_text::util::wrap_in_square_brackets;
use mc_text::{ChatFormatting, Component, HoverEvent};
use uuid::Uuid;

use crate::server::players::player_handle::PlayerHandle;

use crate::util::codec::{self, DataResult, DecodeResult};
use crate::util::java_hash_order::JavaHashCapacity;
use crate::world::level::saveddata::{SavedData, SavedDataType};

/// `BossEvent.BossBarColor`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BossBarColor {
    Pink,
    Blue,
    Red,
    Green,
    Yellow,
    Purple,
    White,
}

impl BossBarColor {
    const ALL: [BossBarColor; 7] = [
        BossBarColor::Pink,
        BossBarColor::Blue,
        BossBarColor::Red,
        BossBarColor::Green,
        BossBarColor::Yellow,
        BossBarColor::Purple,
        BossBarColor::White,
    ];

    pub fn get_name(self) -> &'static str {
        ["pink", "blue", "red", "green", "yellow", "purple", "white"][self as usize]
    }

    pub fn get_formatting(self) -> ChatFormatting {
        [
            ChatFormatting::Red,
            ChatFormatting::Blue,
            ChatFormatting::DarkRed,
            ChatFormatting::Green,
            ChatFormatting::Yellow,
            ChatFormatting::DarkBlue,
            ChatFormatting::White,
        ][self as usize]
    }

    pub fn by_name(name: &str) -> Option<BossBarColor> {
        Self::ALL.into_iter().find(|c| c.get_name() == name)
    }
}

/// `BossEvent.BossBarOverlay`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BossBarOverlay {
    Progress,
    Notched6,
    Notched10,
    Notched12,
    Notched20,
}

impl BossBarOverlay {
    const ALL: [BossBarOverlay; 5] = [
        BossBarOverlay::Progress,
        BossBarOverlay::Notched6,
        BossBarOverlay::Notched10,
        BossBarOverlay::Notched12,
        BossBarOverlay::Notched20,
    ];

    pub fn get_name(self) -> &'static str {
        [
            "progress",
            "notched_6",
            "notched_10",
            "notched_12",
            "notched_20",
        ][self as usize]
    }

    pub fn by_name(name: &str) -> Option<BossBarOverlay> {
        Self::ALL.into_iter().find(|c| c.get_name() == name)
    }
}

fn enum_by_name<T>(tag: &Tag, f: fn(&str) -> Option<T>) -> DataResult<T> {
    let s = codec::string(tag)?;
    f(s).ok_or_else(|| format!("Unknown element name:{s}"))
}

/// `CustomBossEvent.Packed`.
#[derive(Debug, Clone, PartialEq)]
pub struct PackedBossEvent {
    pub name: Component,
    pub visible: bool,
    pub value: i32,
    pub max: i32,
    pub color: BossBarColor,
    pub overlay: BossBarOverlay,
    pub darken_screen: bool,
    pub play_boss_music: bool,
    pub create_world_fog: bool,
    pub players: BTreeSet<Uuid>,
}

impl PackedBossEvent {
    fn decode(tag: &Tag) -> DataResult<PackedBossEvent> {
        let m = codec::compound(tag)?;
        Ok(PackedBossEvent {
            name: Component::from_nbt(codec::field(m, "Name")?).map_err(|e| e.to_string())?,
            visible: codec::optional(m, "Visible", codec::boolean)?.unwrap_or(false),
            value: codec::optional(m, "Value", codec::int)?.unwrap_or(0),
            max: codec::optional(m, "Max", codec::int)?.unwrap_or(100),
            color: codec::optional(m, "Color", |t| enum_by_name(t, BossBarColor::by_name))?
                .unwrap_or(BossBarColor::White),
            overlay: codec::optional(m, "Overlay", |t| enum_by_name(t, BossBarOverlay::by_name))?
                .unwrap_or(BossBarOverlay::Progress),
            darken_screen: codec::optional(m, "DarkenScreen", codec::boolean)?.unwrap_or(false),
            play_boss_music: codec::optional(m, "PlayBossMusic", codec::boolean)?.unwrap_or(false),
            create_world_fog: codec::optional(m, "CreateWorldFog", codec::boolean)?
                .unwrap_or(false),
            players: codec::optional(m, "Players", |t| {
                codec::list(t)?
                    .iter()
                    .map(|e| match e {
                        Tag::IntArray(a) if a.len() == 4 => {
                            Ok(uuid_from_int_array([a[0], a[1], a[2], a[3]]))
                        }
                        _ => Err("Expected an int array of length 4".to_string()),
                    })
                    .collect::<DataResult<BTreeSet<_>>>()
            })?
            .unwrap_or_default(),
        })
    }

    fn encode(&self) -> DataResult<Tag> {
        let mut m = CompoundTag::new();
        m.put("Name", self.name.to_nbt().map_err(|e| e.to_string())?);
        if self.visible {
            m.put_boolean("Visible", true);
        }
        if self.value != 0 {
            m.put_int("Value", self.value);
        }
        if self.max != 100 {
            m.put_int("Max", self.max);
        }
        if self.color != BossBarColor::White {
            m.put_string("Color", self.color.get_name());
        }
        if self.overlay != BossBarOverlay::Progress {
            m.put_string("Overlay", self.overlay.get_name());
        }
        if self.darken_screen {
            m.put_boolean("DarkenScreen", true);
        }
        if self.play_boss_music {
            m.put_boolean("PlayBossMusic", true);
        }
        if self.create_world_fog {
            m.put_boolean("CreateWorldFog", true);
        }
        if !self.players.is_empty() {
            let mut list = ListTag::new();
            for p in &self.players {
                list.push(Tag::IntArray(uuid_to_int_array(p).to_vec()));
            }
            m.put("Players", list);
        }
        Ok(Tag::Compound(m))
    }
}

/// `CustomBossEvent` (`ServerBossEvent`): the bar state, the saved player UUIDs and the online players the bar is
/// shown to. Every visible change is sent to the online players like vanilla's `ServerBossEvent.broadcast`.
#[derive(Clone)]
pub struct CustomBossEvent {
    id: Uuid,
    custom_id: Identifier,
    state: PackedBossEvent,
    progress: f32,
    dirty: bool,
    /// `ServerBossEvent.players` (online players that see the bar).
    online: Vec<PlayerHandle>,
    /// Table capacity of vanilla's `HashSet<ServerPlayer>` (iteration order = hash order, see [`Self::online_players`]).
    online_capacity: JavaHashCapacity,
}

pub(crate) fn wire_color(c: BossBarColor) -> wire::BossBarColor {
    [
        wire::BossBarColor::Pink,
        wire::BossBarColor::Blue,
        wire::BossBarColor::Red,
        wire::BossBarColor::Green,
        wire::BossBarColor::Yellow,
        wire::BossBarColor::Purple,
        wire::BossBarColor::White,
    ][c as usize]
}

pub(crate) fn wire_overlay(o: BossBarOverlay) -> wire::BossBarOverlay {
    [
        wire::BossBarOverlay::Progress,
        wire::BossBarOverlay::Notched6,
        wire::BossBarOverlay::Notched10,
        wire::BossBarOverlay::Notched12,
        wire::BossBarOverlay::Notched20,
    ][o as usize]
}

impl std::fmt::Debug for CustomBossEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CustomBossEvent")
            .field("custom_id", &self.custom_id)
            .field("state", &self.state)
            .field("online", &self.online.len())
            .finish()
    }
}

impl CustomBossEvent {
    fn new(id: Uuid, custom_id: Identifier, name: Component) -> CustomBossEvent {
        let mut e = CustomBossEvent {
            id,
            custom_id,
            state: PackedBossEvent {
                name,
                // `ServerBossEvent.visible = true`
                visible: true,
                value: 0,
                max: 100,
                color: BossBarColor::White,
                overlay: BossBarOverlay::Progress,
                darken_screen: false,
                play_boss_music: false,
                create_world_fog: false,
                players: BTreeSet::new(),
            },
            progress: 0.0,
            dirty: false,
            online: Vec::new(),
            online_capacity: JavaHashCapacity::default(),
        };
        e.set_progress(0.0);
        e
    }

    /// `CustomBossEvent.load`: setters run in vanilla's order (so the saved data is dirty afterwards).
    fn load(id: Uuid, custom_id: Identifier, packed: PackedBossEvent) -> CustomBossEvent {
        let mut e = CustomBossEvent::new(id, custom_id, packed.name.clone());
        e.set_visible(packed.visible);
        e.set_value(packed.value);
        e.set_max(packed.max);
        e.set_color(packed.color);
        e.set_overlay(packed.overlay);
        e.set_darken_screen(packed.darken_screen);
        e.set_play_boss_music(packed.play_boss_music);
        e.set_create_world_fog(packed.create_world_fog);
        e.state.players = packed.players;
        e
    }

    pub fn id(&self) -> Uuid {
        self.id
    }
    pub fn custom_id(&self) -> &Identifier {
        &self.custom_id
    }
    pub fn get_name(&self) -> &Component {
        &self.state.name
    }
    pub fn value(&self) -> i32 {
        self.state.value
    }
    pub fn max(&self) -> i32 {
        self.state.max
    }
    pub fn get_progress(&self) -> f32 {
        self.progress
    }
    pub fn is_visible(&self) -> bool {
        self.state.visible
    }
    pub fn get_color(&self) -> BossBarColor {
        self.state.color
    }
    pub fn get_overlay(&self) -> BossBarOverlay {
        self.state.overlay
    }
    pub fn should_darken_screen(&self) -> bool {
        self.state.darken_screen
    }
    pub fn should_play_boss_music(&self) -> bool {
        self.state.play_boss_music
    }
    pub fn should_create_world_fog(&self) -> bool {
        self.state.create_world_fog
    }
    /// The saved player UUIDs (`CustomBossEvent.players`).
    pub fn players(&self) -> &BTreeSet<Uuid> {
        &self.state.players
    }
    /// `ServerBossEvent.getPlayers()`: the online players the bar is shown to, in the iteration order of vanilla's
    /// `HashSet<ServerPlayer>` (`Entity.hashCode()` is the entity id).
    pub fn online_players(&self) -> Vec<PlayerHandle> {
        self.online_capacity
            .iteration_order(self.online.iter().cloned(), |p| p.id)
    }

    fn properties(&self) -> wire::BossEventProperties {
        wire::BossEventProperties {
            darken_screen: self.state.darken_screen,
            play_music: self.state.play_boss_music,
            create_world_fog: self.state.create_world_fog,
        }
    }

    /// `ClientboundBossEventPacket.createAddPacket`.
    pub fn create_add_packet(&self) -> wire::ClientboundBossEventPacket {
        wire::ClientboundBossEventPacket {
            id: self.id,
            operation: wire::BossEventOperation::Add {
                name: self.state.name.clone(),
                progress: self.progress,
                color: wire_color(self.state.color),
                overlay: wire_overlay(self.state.overlay),
                properties: self.properties(),
            },
        }
    }

    fn broadcast(&self, operation: wire::BossEventOperation) {
        if self.state.visible {
            let packet = wire::ClientboundBossEventPacket {
                id: self.id,
                operation,
            };
            for player in &self.online {
                player.connection.send(&packet);
            }
        }
    }

    fn set_progress(&mut self, progress: f32) {
        if progress != self.progress {
            self.progress = progress;
            self.dirty = true;
            self.broadcast(wire::BossEventOperation::UpdateProgress { progress });
        }
    }

    pub fn set_value(&mut self, value: i32) {
        self.state.value = value;
        self.set_progress(clamp_f32(value as f32 / self.state.max as f32, 0.0, 1.0));
        self.dirty = true;
    }

    pub fn set_max(&mut self, max: i32) {
        self.state.max = max;
        self.set_progress(clamp_f32(self.state.value as f32 / max as f32, 0.0, 1.0));
        self.dirty = true;
    }

    pub fn set_name(&mut self, name: Component) {
        if name != self.state.name {
            self.state.name = name.clone();
            self.dirty = true;
            self.broadcast(wire::BossEventOperation::UpdateName { name });
        }
    }

    pub fn set_color(&mut self, color: BossBarColor) {
        if color != self.state.color {
            self.state.color = color;
            self.dirty = true;
            self.broadcast(wire::BossEventOperation::UpdateStyle {
                color: wire_color(color),
                overlay: wire_overlay(self.state.overlay),
            });
        }
    }

    pub fn set_overlay(&mut self, overlay: BossBarOverlay) {
        if overlay != self.state.overlay {
            self.state.overlay = overlay;
            self.dirty = true;
            self.broadcast(wire::BossEventOperation::UpdateStyle {
                color: wire_color(self.state.color),
                overlay: wire_overlay(overlay),
            });
        }
    }

    pub fn set_darken_screen(&mut self, v: bool) {
        if v != self.state.darken_screen {
            self.state.darken_screen = v;
            self.dirty = true;
            self.broadcast(wire::BossEventOperation::UpdateProperties {
                properties: self.properties(),
            });
        }
    }

    pub fn set_play_boss_music(&mut self, v: bool) {
        if v != self.state.play_boss_music {
            self.state.play_boss_music = v;
            self.dirty = true;
            self.broadcast(wire::BossEventOperation::UpdateProperties {
                properties: self.properties(),
            });
        }
    }

    pub fn set_create_world_fog(&mut self, v: bool) {
        if v != self.state.create_world_fog {
            self.state.create_world_fog = v;
            self.dirty = true;
            self.broadcast(wire::BossEventOperation::UpdateProperties {
                properties: self.properties(),
            });
        }
    }

    pub fn set_visible(&mut self, visible: bool) {
        if visible != self.state.visible {
            self.state.visible = visible;
            self.dirty = true;
            for player in &self.online {
                if visible {
                    player.connection.send(&self.create_add_packet());
                } else {
                    player.connection.send(&wire::ClientboundBossEventPacket {
                        id: self.id,
                        operation: wire::BossEventOperation::Remove,
                    });
                }
            }
        }
    }

    fn remove_online(&mut self, player: &PlayerHandle) -> bool {
        match self.online.iter().position(|p| p.same_player(player)) {
            Some(i) => {
                self.online.remove(i);
                if self.state.visible {
                    player.connection.send(&wire::ClientboundBossEventPacket {
                        id: self.id,
                        operation: wire::BossEventOperation::Remove,
                    });
                }
                true
            }
            None => false,
        }
    }

    /// `addPlayer(player)`: shows the bar and saves the player's UUID.
    pub fn add_player(&mut self, player: &PlayerHandle) {
        if !self.online.iter().any(|p| p.same_player(player)) {
            self.online.push(player.clone());
            self.online_capacity.grow_to_fit(self.online.len());
            if self.state.visible {
                player.connection.send(&self.create_add_packet());
            }
        }
        if self.state.players.insert(player.uuid) {
            self.dirty = true;
        }
    }

    /// `removePlayer(player)`.
    pub fn remove_player(&mut self, player: &PlayerHandle) {
        self.remove_online(player);
        if self.state.players.remove(&player.uuid) {
            self.dirty = true;
        }
    }

    /// `removeAllPlayers()`.
    pub fn remove_all_players(&mut self) {
        for player in self.online.clone() {
            self.remove_online(&player);
        }
        if !self.state.players.is_empty() {
            self.state.players.clear();
            self.dirty = true;
        }
    }

    /// `setPlayers(players)`: the bar is shown to exactly `players`; returns whether the set changed.
    pub fn set_players(&mut self, players: &[PlayerHandle]) -> bool {
        let to_remove: Vec<Uuid> = self
            .state
            .players
            .iter()
            .filter(|u| !players.iter().any(|p| p.uuid == **u))
            .copied()
            .collect();
        let to_add: Vec<&PlayerHandle> = players
            .iter()
            .filter(|p| !self.state.players.contains(&p.uuid))
            .collect();
        for uuid in &to_remove {
            if let Some(p) = self.online.iter().find(|p| p.uuid == *uuid).cloned() {
                self.remove_player(&p);
            }
            self.state.players.remove(uuid);
        }
        for p in &to_add {
            self.add_player(p);
        }
        let changed = !to_remove.is_empty() || !to_add.is_empty();
        if changed {
            self.dirty = true;
        }
        changed
    }

    /// `onPlayerConnect(player)`: players saved on the bar see it again when they join.
    pub fn on_player_connect(&mut self, player: &PlayerHandle) {
        if self.state.players.contains(&player.uuid) {
            self.add_player(player);
        }
    }

    /// `onPlayerDisconnect(player)`: `ServerBossEvent.removePlayer` (the saved UUID stays).
    pub fn on_player_disconnect(&mut self, player: &PlayerHandle) {
        self.remove_online(player);
    }

    /// `getDisplayName`: `[name]` coloured by the bar colour, hover/insertion = the custom id.
    pub fn get_display_name(&self) -> Component {
        let id = self.custom_id.to_string();
        wrap_in_square_brackets(self.state.name.clone()).with_style_fn(|s| {
            s.with_color_formatting(Some(self.state.color.get_formatting()))
                .with_hover_event(Some(HoverEvent::show_text(Component::literal(id.clone()))))
                .with_insertion(Some(id.clone()))
        })
    }

    pub fn pack(&self) -> PackedBossEvent {
        self.state.clone()
    }
}

/// `CustomBossEvents`.
#[derive(Debug, Default)]
pub struct CustomBossEvents {
    dirty: bool,
    /// Insertion-ordered; vanilla uses a `HashMap` (iteration order = [`JavaHashCapacity::iteration_order`] by id hash).
    events: IndexMap<Identifier, CustomBossEvent>,
    capacity: JavaHashCapacity,
}

impl SavedData for CustomBossEvents {
    fn is_dirty(&self) -> bool {
        self.dirty || self.events.values().any(|e| e.dirty)
    }
    fn set_dirty_to(&mut self, dirty: bool) {
        self.dirty = dirty;
        if !dirty {
            self.events.values_mut().for_each(|e| e.dirty = false);
        }
    }
}

impl CustomBossEvents {
    pub fn get(&self, id: &Identifier) -> Option<&CustomBossEvent> {
        self.events.get(id)
    }

    /// Mutable access; changes through the event's setters mark this saved data dirty.
    /// `onPlayerConnect(player)`.
    pub fn on_player_connect(&mut self, player: &PlayerHandle) {
        for e in self.events.values_mut() {
            e.on_player_connect(player);
        }
    }

    /// `onPlayerDisconnect(player)`.
    pub fn on_player_disconnect(&mut self, player: &PlayerHandle) {
        for e in self.events.values_mut() {
            e.on_player_disconnect(player);
        }
    }

    pub fn get_mut(&mut self, id: &Identifier) -> Option<&mut CustomBossEvent> {
        self.events.get_mut(id)
    }

    pub fn create(
        &mut self,
        random: &mut dyn RandomSource,
        id: Identifier,
        name: Component,
    ) -> &mut CustomBossEvent {
        self.create_with_uuid(create_insecure_uuid(random), id, name)
    }

    /// `create` with the bar's UUID already drawn from the level random (`createInsecureUUID`).
    pub fn create_with_uuid(
        &mut self,
        uuid: Uuid,
        id: Identifier,
        name: Component,
    ) -> &mut CustomBossEvent {
        let event = CustomBossEvent::new(uuid, id.clone(), name);
        self.events.insert(id.clone(), event);
        self.capacity.grow_to_fit(self.events.len());
        self.set_dirty();
        self.events.get_mut(&id).expect("just inserted")
    }

    pub fn remove(&mut self, id: &Identifier) {
        if self.events.shift_remove(id).is_some() {
            self.set_dirty();
        }
    }

    /// `getIds()` in `HashMap` iteration order.
    pub fn get_ids(&self) -> Vec<&Identifier> {
        self.capacity
            .iteration_order(self.events.keys(), |id| id.java_hash_code())
    }

    /// `getEvents()` in `HashMap` iteration order.
    pub fn get_events(&self) -> Vec<&CustomBossEvent> {
        self.capacity
            .iteration_order(self.events.iter(), |(id, _)| id.java_hash_code())
            .into_iter()
            .map(|(_, e)| e)
            .collect()
    }

    fn decode(tag: &Tag) -> DecodeResult<CustomBossEvents> {
        let entries = codec::unbounded_map(tag, Identifier::read, PackedBossEvent::decode);
        codec::map_decoded(entries, |entries| {
            let events: IndexMap<Identifier, CustomBossEvent> = entries
                .into_iter()
                .map(|(id, packed)| {
                    (
                        id.clone(),
                        CustomBossEvent::load(Uuid::new_v4(), id, packed),
                    )
                })
                .collect();
            let mut capacity = JavaHashCapacity::default();
            capacity.grow_to_fit(events.len());
            CustomBossEvents {
                dirty: false,
                events,
                capacity,
            }
        })
    }

    fn encode(&self) -> DataResult<Tag> {
        let mut m = CompoundTag::new();
        for (id, e) in &self.events {
            m.put(id.to_string(), e.pack().encode()?);
        }
        Ok(Tag::Compound(m))
    }

    /// `CustomBossEvents.TYPE`.
    pub fn type_() -> SavedDataType<CustomBossEvents> {
        SavedDataType::new(
            Identifier::parse("minecraft:custom_boss_events").expect("static id"),
            CustomBossEvents::default,
            CustomBossEvents::decode,
            CustomBossEvents::encode,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codec_round_trip_with_defaults_omitted() {
        let tag = mc_nbt::snbt::parse(
            "{\"minecraft:a\":{Name:\"A\",Color:\"red\",Value:5,Max:10,Players:[[I;1,2,3,4]],Visible:1b},\"b:c\":{Name:{text:\"B\",bold:1b}}}",
        )
        .unwrap();
        let ty = CustomBossEvents::type_();
        let mut events = (ty.decode)(&tag).map_err(|e| e.error).unwrap();
        assert!(
            events.is_dirty(),
            "load runs the setters, which dirty the data like vanilla"
        );
        let a = events.get(&Identifier::parse("a").unwrap()).unwrap();
        assert_eq!(
            (a.value(), a.max(), a.get_progress(), a.get_color()),
            (5, 10, 0.5, BossBarColor::Red)
        );
        assert_eq!((ty.encode)(&events).unwrap(), tag);
        events.set_dirty_to(false);
        assert!(!events.is_dirty());
        events
            .get_mut(&Identifier::parse("a").unwrap())
            .unwrap()
            .set_value(9);
        assert!(events.is_dirty());
    }
}
