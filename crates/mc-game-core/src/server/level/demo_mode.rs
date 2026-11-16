//! `DemoMode`: the demo server's `ServerPlayerGameMode` (`MinecraftServer.createGameModeForPlayer` with `--demo`). The
//! state is `ServerPlayerGameMode::demo`; the block break and use overrides are in `server_player_game_mode.rs`.

use crate::server::level::ServerLevel;
use crate::world::entity::player::server_player::{DemoState, ServerPlayer};
use mc_protocol::packets::play::ClientboundGameEventPacket;
use mc_text::Component;

/// `DemoMode.TOTAL_PLAY_TICKS`.
const TOTAL_PLAY_TICKS: i64 = 120_500;

impl ServerPlayer {
    /// `new DemoMode(player)` instead of `new ServerPlayerGameMode(player)`.
    pub fn use_demo_mode(&mut self) {
        self.game_mode.demo = Some(DemoState::default());
    }

    fn send_demo_event(&self, param: f32) {
        self.send(&ClientboundGameEventPacket {
            event: ClientboundGameEventPacket::DEMO_EVENT,
            param,
        });
    }

    /// The part of `DemoMode.tick()` after `super.tick()`.
    pub(super) fn tick_demo(&mut self, level: &ServerLevel) {
        let Some(mut demo) = self.game_mode.demo else {
            return;
        };
        demo.game_mode_ticks += 1;
        let time = level.game_time();
        let day = time / 24000 + 1;
        if !demo.displayed_intro && demo.game_mode_ticks > 20 {
            demo.displayed_intro = true;
            self.send_demo_event(0.0);
        }
        demo.demo_has_ended = time > TOTAL_PLAY_TICKS;
        if demo.demo_has_ended {
            demo.demo_ended_reminder += 1;
        }
        if time % 24000 == 500 {
            if day <= 6 {
                if day == 6 {
                    self.send_demo_event(104.0);
                } else {
                    self.send_system_message_overlay(
                        Component::translatable(format!("demo.day.{day}"), vec![]),
                        false,
                    );
                }
            }
        } else if day == 1 {
            match time {
                100 => self.send_demo_event(101.0),
                175 => self.send_demo_event(102.0),
                250 => self.send_demo_event(103.0),
                _ => {}
            }
        } else if day == 5 && time % 24000 == 22000 {
            self.send_system_message_overlay(
                Component::translatable("demo.day.warning", vec![]),
                false,
            );
        }
        self.game_mode.demo = Some(demo);
    }

    /// `DemoMode.outputDemoReminder()`.
    pub(super) fn output_demo_reminder(&mut self) {
        let Some(demo) = &mut self.game_mode.demo else {
            return;
        };
        if demo.demo_ended_reminder > 100 {
            demo.demo_ended_reminder = 0;
            self.send_system_message_overlay(
                Component::translatable("demo.reminder", vec![]),
                false,
            );
        }
    }
}
