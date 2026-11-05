//! `net.minecraft.world.entity.player.*` + `ServerPlayer`: `Avatar`/`Player` data and save/load.
pub mod abilities;
pub mod experience;
pub mod food;
pub mod game_mode;
pub mod shoulder;
pub mod stats;
pub mod damage;
pub mod food_data;
pub mod handle_sync;
pub mod drop;
pub mod effects;
pub mod inventory;
pub mod item_use;
pub mod player_systems;
pub mod menu_ctx;
pub mod movement_stats;
pub mod pickup;
pub mod menu_open;
pub mod tick;
pub mod travel;
pub mod server_player;

pub use abilities::Abilities;
pub use food_data::FoodData;
pub use server_player::ServerPlayer;
use crate::world::container::SimpleContainer;
use crate::world::inventory::PlayerMenus;
use inventory::Inventory;

use super::dimensions::{EntityAttachments, EntityDimensions};
use super::living::LivingEntityData;
use crate::define_accessor;
use mc_core::ident::Identifier;
use mc_core::phys::Vec3;
use mc_data::enums::Pose;
use mc_protocol::codec::{GameProfile, GlobalPos};
use mc_protocol::packets::common::HumanoidArm;

pub const MAX_HEALTH: i32 = 20;
pub const SLEEP_DURATION: i32 = 100;
pub const WAKE_UP_DURATION: i32 = 10;
pub const DEFAULT_BLOCK_INTERACTION_RANGE: f32 = 4.5;
pub const DEFAULT_ENTITY_INTERACTION_RANGE: f32 = 3.0;
pub const CREATIVE_ENTITY_INTERACTION_RANGE_MODIFIER_VALUE: f32 = 2.0;

define_accessor!(pub DATA_PLAYER_MAIN_HAND: HumanoidArm = Avatar);
define_accessor!(pub DATA_PLAYER_MODE_CUSTOMISATION: Byte = Avatar);
define_accessor!(pub DATA_PLAYER_ABSORPTION_ID: Float = Player);
define_accessor!(pub DATA_SCORE_ID: Int = Player);
define_accessor!(pub DATA_SHOULDER_PARROT_LEFT: OptionalUnsignedInt = Player);
define_accessor!(pub DATA_SHOULDER_PARROT_RIGHT: OptionalUnsignedInt = Player);

/// `Avatar.POSES` / `Avatar.getDefaultDimensions(pose)`.
pub fn avatar_dimensions(pose: Pose) -> EntityDimensions {
    let (width, height, eye_height, fixed) = mc_data::misc::AVATAR_POSE_DIMENSIONS
        .iter()
        .find(|(p, ..)| *p == pose)
        .map(|&(_, w, h, e, f)| (w, h, e, f))
        // `POSES.getOrDefault(pose, STANDING_DIMENSIONS)`
        .unwrap_or((0.6, 1.8, 1.62, false));
    let mut dims = if fixed { EntityDimensions::fixed(width, height) } else { EntityDimensions::scalable(width, height) }.with_eye_height(eye_height);
    if matches!(pose, Pose::Standing | Pose::Crouching) || !AVATAR_POSES_KNOWN.contains(&pose) {
        // `attach(VEHICLE, DEFAULT_VEHICLE_ATTACHMENT)`; the other attachments keep their fallbacks
        let base = EntityAttachments::create_default(width, height);
        dims.attachments = base.with_vehicle(Vec3::new(0.0, 0.6, 0.0));
    }
    dims
}

const AVATAR_POSES_KNOWN: [Pose; 7] = [Pose::Standing, Pose::Sleeping, Pose::FallFlying, Pose::Swimming, Pose::SpinAttack, Pose::Crouching, Pose::Dying];

/// `Player` fields (the `Avatar` accessors live in the synched data).
pub struct PlayerData {
    pub living: LivingEntityData,
    pub game_profile: GameProfile,
    pub inventory: Inventory,
    /// `enderChestInventory` (27 slots).
    pub ender_chest: crate::world::inventory::PlayerEnderChestContainer,
    pub menus: PlayerMenus,
    /// `cooldowns` (`ServerItemCooldowns`; see `ServerPlayer::flush_item_cooldown_packets`).
    pub cooldowns: crate::world::item::ItemCooldowns,
    pub abilities: Abilities,
    pub food_data: FoodData,
    pub experience_level: i32,
    pub total_experience: i32,
    pub experience_progress: f32,
    pub enchantment_seed: i32,
    pub sleep_counter: i32,
    pub take_xp_delay: i32,
    pub jump_trigger_time: i32,
    pub was_underwater: bool,
    pub last_level_up_time: i32,
    pub reduced_debug_info: bool,
    pub last_death_location: Option<GlobalPos>,
    pub post_effects: Vec<Identifier>,
    pub hurt_dir: f32,
    // ==== [Player] additions (PlayerData field) ====
    // ==== [LivingCore] additions (PlayerData field) ====
    // ==== [Damage] additions (PlayerData field) ====
}

impl PlayerData {
    /// `Player(level, gameProfile)` (`super(EntityTypes.PLAYER, level)` then `setUUID(profile.id())`).
    pub fn new(id: super::EntityId, game_profile: GameProfile) -> Self {
        let mut living = LivingEntityData::new(mc_data::EntityType::PLAYER, id, super::entity::TOTAL_AIR_SUPPLY, |b| {
            b.define(DATA_PLAYER_MAIN_HAND, HumanoidArm::Right);
            b.define(DATA_PLAYER_MODE_CUSTOMISATION, 0);
            b.define(DATA_PLAYER_ABSORPTION_ID, 0.0);
            b.define(DATA_SCORE_ID, 0);
            b.define(DATA_SHOULDER_PARROT_LEFT, None);
            b.define(DATA_SHOULDER_PARROT_RIGHT, None);
        });
        living.entity.uuid = game_profile.id;
        Self {
            living,
            game_profile,
            inventory: Inventory::new(),
            ender_chest: crate::world::inventory::PlayerEnderChestContainer::new(),
            menus: PlayerMenus::new(),
            cooldowns: crate::world::item::ItemCooldowns::new(),
            abilities: Abilities::default(),
            food_data: FoodData::default(),
            experience_level: 0,
            total_experience: 0,
            experience_progress: 0.0,
            enchantment_seed: 0,
            sleep_counter: 0,
            take_xp_delay: 0,
            jump_trigger_time: 0,
            was_underwater: false,
            last_level_up_time: 0,
            reduced_debug_info: false,
            last_death_location: None,
            post_effects: Vec::new(),
            hurt_dir: 0.0,
            // ==== [Player] additions (PlayerData::new initializer) ====
            // ==== [LivingCore] additions (PlayerData::new initializer) ====
            // ==== [Damage] additions (PlayerData::new initializer) ====
        }
    }

    pub fn get_main_arm(&self) -> HumanoidArm {
        self.living.entity.entity_data.get(DATA_PLAYER_MAIN_HAND)
    }

    pub fn set_main_arm(&mut self, arm: HumanoidArm) {
        self.living.entity.entity_data.set(DATA_PLAYER_MAIN_HAND, arm);
    }

    /// `getScore()` / `setScore(score)`.
    pub fn get_score(&self) -> i32 {
        self.living.entity.entity_data.get(DATA_SCORE_ID)
    }

    pub fn set_score(&mut self, score: i32) {
        self.living.entity.entity_data.set(DATA_SCORE_ID, score);
    }

    /// `getAbsorptionAmount()` (synched for players).
    pub fn get_absorption_amount(&self) -> f32 {
        self.living.entity.entity_data.get(DATA_PLAYER_ABSORPTION_ID)
    }

    pub fn internal_set_absorption_amount(&mut self, amount: f32) {
        self.living.entity.entity_data.set(DATA_PLAYER_ABSORPTION_ID, amount);
    }

    /// `isModelPartShown(part)` with the part's mask.
    pub fn is_model_part_shown(&self, mask: u8) -> bool {
        (self.living.entity.entity_data.get(DATA_PLAYER_MODE_CUSTOMISATION) as u8 & mask) == mask
    }

    pub fn is_sleeping_in_bed(&self) -> bool {
        self.living.is_sleeping()
    }
}
