//! Plain data tracked from clientbound packets: entities, tab list, scoreboard, boss bars, titles.
use std::collections::{BTreeMap, HashMap};

use mc_core::phys::Vec3;
use mc_data::EntityType;
use mc_protocol::packets::play::{EntityDataValue, GameType};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq)]
pub struct TrackedEntity {
    pub id: i32,
    pub uuid: Uuid,
    pub entity_type: EntityType,
    pub pos: Vec3,
    pub vel: Vec3,
    pub yaw: f32,
    pub pitch: f32,
    pub head_yaw: f32,
    pub on_ground: bool,
    /// Synched entity data by accessor id.
    pub data: BTreeMap<u8, EntityDataValue>,
    pub vehicle: Option<i32>,
    pub passengers: Vec<i32>,
}

impl TrackedEntity {
    pub fn new(id: i32, uuid: Uuid, entity_type: EntityType, pos: Vec3) -> Self {
        Self {
            id,
            uuid,
            entity_type,
            pos,
            vel: Vec3::ZERO,
            yaw: 0.0,
            pitch: 0.0,
            head_yaw: 0.0,
            on_ground: false,
            data: BTreeMap::new(),
            vehicle: None,
            passengers: Vec::new(),
        }
    }
}

/// `Mth.unpackDegrees`
pub fn unpack_degrees(b: i8) -> f32 {
    (b as i32 * 360) as f32 / 256.0
}

#[derive(Debug, Clone, PartialEq)]
pub struct PlayerInfo {
    pub uuid: Uuid,
    pub name: String,
    pub listed: bool,
    pub latency: i32,
    pub game_mode: GameType,
    pub display_name: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Objective {
    pub name: String,
    pub display_name: String,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Team {
    pub name: String,
    pub display_name: String,
    pub prefix: String,
    pub suffix: String,
    pub members: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BossBar {
    pub id: Uuid,
    pub name: String,
    pub progress: f32,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Titles {
    pub title: Option<String>,
    pub subtitle: Option<String>,
    pub action_bar: Option<String>,
    pub fade_in: i32,
    pub stay: i32,
    pub fade_out: i32,
}

#[derive(Debug, Default)]
pub struct Social {
    pub players: HashMap<Uuid, PlayerInfo>,
    pub objectives: HashMap<String, Objective>,
    /// `(owner, objective)` -> score
    pub scores: HashMap<(String, String), i32>,
    pub display_slots: HashMap<String, String>,
    pub teams: HashMap<String, Team>,
    pub bossbars: HashMap<Uuid, BossBar>,
    pub titles: Titles,
    pub tab_header: String,
    pub tab_footer: String,
}
