//! `net.minecraft.server.level.ServerBossEvent` (+ `BossEvent`): a boss bar and the online players that see it. Every
//! visible change is sent to those players like vanilla's `ServerBossEvent.broadcast`.

use crate::server::bossevents::{BossBarColor, BossBarOverlay, wire_color, wire_overlay};
use crate::server::network::ConnectionSender;
use crate::world::entity::EntityId;
use mc_protocol::packets::play as wire;
use mc_text::Component;
use uuid::Uuid;

pub struct ServerBossEvent {
    id: Uuid,
    name: Component,
    progress: f32,
    color: BossBarColor,
    overlay: BossBarOverlay,
    darken_screen: bool,
    play_boss_music: bool,
    create_world_fog: bool,
    /// `players` (vanilla `HashSet<ServerPlayer>`, equal by entity id; kept in insertion order): the player's entity id and the
    /// connection its packets go to.
    players: Vec<(EntityId, ConnectionSender)>,
    visible: bool,
}

impl ServerBossEvent {
    pub fn new(id: Uuid, name: Component, color: BossBarColor, overlay: BossBarOverlay) -> Self {
        ServerBossEvent {
            id,
            name,
            progress: 1.0,
            color,
            overlay,
            darken_screen: false,
            play_boss_music: false,
            create_world_fog: false,
            players: Vec::new(),
            visible: true,
        }
    }

    pub fn get_id(&self) -> Uuid {
        self.id
    }
    pub fn get_name(&self) -> &Component {
        &self.name
    }
    pub fn get_progress(&self) -> f32 {
        self.progress
    }
    pub fn get_color(&self) -> BossBarColor {
        self.color
    }
    pub fn get_overlay(&self) -> BossBarOverlay {
        self.overlay
    }
    pub fn should_darken_screen(&self) -> bool {
        self.darken_screen
    }
    pub fn should_play_boss_music(&self) -> bool {
        self.play_boss_music
    }
    pub fn should_create_world_fog(&self) -> bool {
        self.create_world_fog
    }
    pub fn is_visible(&self) -> bool {
        self.visible
    }
    /// `getPlayers()`: the entity ids of the players seeing the bar.
    pub fn get_players(&self) -> impl Iterator<Item = EntityId> + '_ {
        self.players.iter().map(|(id, _)| *id)
    }

    fn properties(&self) -> wire::BossEventProperties {
        wire::BossEventProperties {
            darken_screen: self.darken_screen,
            play_music: self.play_boss_music,
            create_world_fog: self.create_world_fog,
        }
    }

    /// `ClientboundBossEventPacket.createAddPacket`.
    pub fn create_add_packet(&self) -> wire::ClientboundBossEventPacket {
        wire::ClientboundBossEventPacket {
            id: self.id,
            operation: wire::BossEventOperation::Add {
                name: self.name.clone(),
                progress: self.progress,
                color: wire_color(self.color),
                overlay: wire_overlay(self.overlay),
                properties: self.properties(),
            },
        }
    }

    fn broadcast(&self, operation: wire::BossEventOperation) {
        if self.visible {
            let packet = wire::ClientboundBossEventPacket {
                id: self.id,
                operation,
            };
            for (_, connection) in &self.players {
                connection.send(&packet);
            }
        }
    }

    pub fn set_progress(&mut self, progress: f32) {
        if progress != self.progress {
            self.progress = progress;
            self.broadcast(wire::BossEventOperation::UpdateProgress { progress });
        }
    }

    pub fn set_color(&mut self, color: BossBarColor) {
        if color != self.color {
            self.color = color;
            self.broadcast(wire::BossEventOperation::UpdateStyle {
                color: wire_color(color),
                overlay: wire_overlay(self.overlay),
            });
        }
    }

    pub fn set_overlay(&mut self, overlay: BossBarOverlay) {
        if overlay != self.overlay {
            self.overlay = overlay;
            self.broadcast(wire::BossEventOperation::UpdateStyle {
                color: wire_color(self.color),
                overlay: wire_overlay(overlay),
            });
        }
    }

    pub fn set_darken_screen(&mut self, darken_screen: bool) -> &mut Self {
        if darken_screen != self.darken_screen {
            self.darken_screen = darken_screen;
            self.broadcast(wire::BossEventOperation::UpdateProperties {
                properties: self.properties(),
            });
        }
        self
    }

    pub fn set_play_boss_music(&mut self, play_boss_music: bool) -> &mut Self {
        if play_boss_music != self.play_boss_music {
            self.play_boss_music = play_boss_music;
            self.broadcast(wire::BossEventOperation::UpdateProperties {
                properties: self.properties(),
            });
        }
        self
    }

    pub fn set_create_world_fog(&mut self, create_world_fog: bool) -> &mut Self {
        if create_world_fog != self.create_world_fog {
            self.create_world_fog = create_world_fog;
            self.broadcast(wire::BossEventOperation::UpdateProperties {
                properties: self.properties(),
            });
        }
        self
    }

    pub fn set_name(&mut self, name: Component) {
        if name != self.name {
            self.name = name.clone();
            self.broadcast(wire::BossEventOperation::UpdateName { name });
        }
    }

    /// `addPlayer(player)`.
    pub fn add_player(&mut self, player: EntityId, connection: &ConnectionSender) {
        if !self.players.iter().any(|(id, _)| *id == player) {
            self.players.push((player, connection.clone()));
            if self.visible {
                connection.send(&self.create_add_packet());
            }
        }
    }

    /// `removePlayer(player)`.
    pub fn remove_player(&mut self, player: EntityId) {
        if let Some(i) = self.players.iter().position(|(id, _)| *id == player) {
            let (_, connection) = self.players.remove(i);
            if self.visible {
                connection.send(&wire::ClientboundBossEventPacket {
                    id: self.id,
                    operation: wire::BossEventOperation::Remove,
                });
            }
        }
    }

    /// `removeAllPlayers()`.
    pub fn remove_all_players(&mut self) {
        let players: Vec<EntityId> = self.get_players().collect();
        for player in players {
            self.remove_player(player);
        }
    }

    /// `setVisible(visible)`.
    pub fn set_visible(&mut self, visible: bool) {
        if visible != self.visible {
            self.visible = visible;
            for (_, connection) in &self.players {
                if visible {
                    connection.send(&self.create_add_packet());
                } else {
                    connection.send(&wire::ClientboundBossEventPacket {
                        id: self.id,
                        operation: wire::BossEventOperation::Remove,
                    });
                }
            }
        }
    }
}
