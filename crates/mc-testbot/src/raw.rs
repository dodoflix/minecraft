//! A bare packet-level play client: no world model and no physics. It answers what keeps a session alive (keep-alive,
//! chunk batches, teleports) and records one transcript line per received packet, for scripted sequences whose server
//! replies are compared between the vanilla server and ours.
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use mc_protocol::codec::read_varint;
use mc_protocol::net::{PacketReader, PacketWriter};
use mc_protocol::packet::{ConnectionState, PacketFlow, packet_name};
use mc_protocol::packets::play::*;
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use tokio::sync::mpsc::{UnboundedReceiver, unbounded_channel};

use crate::error::{BotError, Result};
use crate::join::{BotOptions, login_and_configure, open};
use crate::log::PacketLog;

/// Packets whose payload is bulky or varies between runs: the transcript keeps their name only.
const NAME_ONLY: &[&str] = &[
    "minecraft:level_chunk_with_light",
    "minecraft:update_tags",
    "minecraft:update_recipes",
    "minecraft:commands",
    "minecraft:update_advancements",
    "minecraft:set_entity_data",
    "minecraft:update_attributes",
];

pub struct RawClient {
    writer: PacketWriter<OwnedWriteHalf>,
    rx: UnboundedReceiver<Vec<u8>>,
    /// Where the client believes it is: the last position it sent or the last teleport it applied.
    pub pos: (f64, f64, f64),
    /// One `<packet name> <debug text, 160 chars>` line per received packet and `## <title>` section markers.
    pub transcript: String,
    /// The light payload of every `level_chunk_with_light` (`false`) and `light_update` (`true`) packet, in arrival
    /// order, with the chunk it is for.
    pub light: Vec<LightPayload>,
}

/// What a `level_chunk_with_light` or `light_update` packet carried for the light engine.
pub struct LightPayload {
    pub update: bool,
    pub x: i32,
    pub z: i32,
    pub data: ClientboundLightUpdatePacketData,
}

impl RawClient {
    /// Logs in and finishes configuration; the play packets are then read in the background and delivered by [`Self::pump`].
    pub async fn connect(opts: &BotOptions) -> Result<RawClient> {
        let mut conn = open(opts, Arc::new(Mutex::new(PacketLog::new()))).await?;
        login_and_configure(&mut conn, opts).await?;
        let mut reader: PacketReader<OwnedReadHalf> = conn.reader;
        let (tx, rx) = unbounded_channel();
        tokio::spawn(async move {
            while let Ok(payload) = reader.read_payload().await {
                if tx.send(payload).is_err() {
                    break;
                }
            }
        });
        Ok(RawClient {
            writer: conn.writer,
            rx,
            pos: (0.0, 0.0, 0.0),
            transcript: String::new(),
            light: Vec::new(),
        })
    }

    /// Starts a transcript section.
    pub fn section(&mut self, title: &str) {
        self.transcript.push_str(&format!("## {title}\n"));
    }

    pub async fn send(&mut self, packet: ServerboundPlayPacket) -> Result<()> {
        let mut buf = Vec::new();
        packet.encode_payload(&mut buf)?;
        self.writer.write_payload(&buf).await?;
        self.writer.flush().await?;
        Ok(())
    }

    /// Sends a position update and remembers it.
    pub async fn move_to(&mut self, x: f64, y: f64, z: f64, ground: bool) -> Result<()> {
        self.pos = (x, y, z);
        self.send(ServerboundPlayPacket::MovePlayerPos(
            ServerboundMovePlayerPosPacket {
                x,
                y,
                z,
                on_ground: ground,
                horizontal_collision: false,
            },
        ))
        .await
    }

    /// [`Self::pump`] until `count` packets named `name` were received (an event wait); fails after `timeout`.
    pub async fn pump_until(&mut self, name: &str, count: usize, timeout: Duration) -> Result<()> {
        let end = Instant::now() + timeout;
        while self
            .transcript
            .lines()
            .filter(|l| l.starts_with(name))
            .count()
            < count
        {
            if Instant::now() >= end {
                return Err(BotError::Timeout(format!(
                    "{count} x {name} not received within {timeout:?}"
                )));
            }
            self.pump(Duration::from_millis(50), |_| None).await?;
        }
        Ok(())
    }

    /// Reads and answers packets for `dur`, one 50 ms client tick at a time; `tick(n)` is called every tick and may
    /// return a position to send. Fails when the connection ends, with the server's disconnect packet if it sent one.
    pub async fn pump(
        &mut self,
        dur: Duration,
        tick: impl FnMut(u32) -> Option<(f64, f64, f64, bool)>,
    ) -> Result<()> {
        self.pump_ticks(dur, tick).await.map_err(|e| {
            match self
                .transcript
                .lines()
                .find(|l| l.starts_with("minecraft:disconnect"))
            {
                Some(kick) => BotError::Disconnected(kick.to_owned()),
                None => e,
            }
        })
    }

    async fn pump_ticks(
        &mut self,
        dur: Duration,
        mut tick: impl FnMut(u32) -> Option<(f64, f64, f64, bool)>,
    ) -> Result<()> {
        let end = Instant::now() + dur;
        let mut n = 0u32;
        while Instant::now() < end {
            let tick_end = Instant::now() + Duration::from_millis(50);
            while Instant::now() < tick_end {
                let left = tick_end.saturating_duration_since(Instant::now());
                match tokio::time::timeout(left, self.rx.recv()).await {
                    Ok(Some(payload)) => self.handle(&payload).await?,
                    Ok(None) => return Err(BotError::Closed),
                    Err(_) => break,
                }
            }
            if let Some((x, y, z, ground)) = tick(n) {
                self.move_to(x, y, z, ground).await?;
            }
            self.send(ServerboundPlayPacket::ClientTickEnd(
                ServerboundClientTickEndPacket,
            ))
            .await?;
            n += 1;
        }
        Ok(())
    }

    async fn handle(&mut self, payload: &[u8]) -> Result<()> {
        let id = read_varint(&mut &payload[..])?;
        let name = packet_name(ConnectionState::Play, PacketFlow::Clientbound, id).unwrap_or("?");
        let packet = match ClientboundPlayPacket::decode_payload(payload) {
            Ok(p) => p,
            Err(e) => {
                self.transcript
                    .push_str(&format!("{name} <decode error {e}>\n"));
                return Ok(());
            }
        };
        let detail = if NAME_ONLY.contains(&name) {
            String::new()
        } else {
            format!("{packet:?}").chars().take(160).collect()
        };
        self.transcript.push_str(&format!("{name} {detail}\n"));
        match &packet {
            ClientboundPlayPacket::LevelChunkWithLight(p) => self.light.push(LightPayload {
                update: false,
                x: p.x,
                z: p.z,
                data: p.light_data.clone(),
            }),
            ClientboundPlayPacket::LightUpdate(p) => self.light.push(LightPayload {
                update: true,
                x: p.x,
                z: p.z,
                data: p.light_data.clone(),
            }),
            _ => {}
        }
        match packet {
            ClientboundPlayPacket::KeepAlive(k) => {
                self.send(ServerboundPlayPacket::KeepAlive(
                    ServerboundKeepAlivePacket { id: k.id },
                ))
                .await
            }
            ClientboundPlayPacket::ChunkBatchFinished(_) => {
                self.send(ServerboundPlayPacket::ChunkBatchReceived(
                    ServerboundChunkBatchReceivedPacket {
                        desired_chunks_per_tick: 9.0,
                    },
                ))
                .await
            }
            ClientboundPlayPacket::PlayerPosition(tp) => {
                let rel = |r: Relative| tp.relatives.contains(r);
                let at = tp.change.position;
                let p = mc_core::phys::Vec3::new(
                    if rel(Relative::X) {
                        self.pos.0 + at.x
                    } else {
                        at.x
                    },
                    if rel(Relative::Y) {
                        self.pos.1 + at.y
                    } else {
                        at.y
                    },
                    if rel(Relative::Z) {
                        self.pos.2 + at.z
                    } else {
                        at.z
                    },
                );
                self.send(ServerboundPlayPacket::AcceptTeleportation(
                    ServerboundAcceptTeleportationPacket {
                        id: tp.id,
                        x: p.x,
                        y: p.y,
                        z: p.z,
                        y_rot: tp.change.y_rot,
                        x_rot: tp.change.x_rot,
                    },
                ))
                .await?;
                self.pos = (p.x, p.y, p.z);
                self.send(ServerboundPlayPacket::MovePlayerPosRot(
                    ServerboundMovePlayerPosRotPacket {
                        x: p.x,
                        y: p.y,
                        z: p.z,
                        y_rot: tp.change.y_rot,
                        x_rot: tp.change.x_rot,
                        on_ground: false,
                        horizontal_collision: false,
                    },
                ))
                .await?;
                self.send(ServerboundPlayPacket::ClientTickEnd(
                    ServerboundClientTickEndPacket,
                ))
                .await
            }
            _ => Ok(()),
        }
    }
}
