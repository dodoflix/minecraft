//! `net.minecraft.server.level.ServerPlayer` data and NBT.

use super::PlayerData;
use crate::server::network::ConnectionSender;
use crate::world::entity::entity::{self, Entity, EntityData, EntityLookupFn};
use crate::world::entity::living::{self, LivingEntityData};
use crate::world::entity::{EntityDimensions, EntityId};
use mc_core::ident::Identifier;
use mc_core::phys::Vec3;
use mc_core::pos::{BlockPos, SectionPos};
use mc_data::enums::{GameType, Pose};
use mc_nbt::{CompoundTag, ListTag, Tag};
use mc_protocol::codec::{GameProfile, GlobalPos};
use mc_protocol::packets::common::{ChatVisibility, ClientInformation, ParticleStatus};
use mc_protocol::packets::play::Input;
use crate::world::entity::respawn::RespawnConfig;
use std::any::Any;

/// `WardenSpawnTracker` state (the tracker logic belongs to the warden slice).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WardenSpawnTracker {
    pub ticks_since_last_warning: i32,
    pub warning_level: i32,
    pub cooldown_ticks: i32,
}

/// `ServerPlayerGameMode` state (logic: `server/level/server_player_game_mode.rs`).
#[derive(Clone, Debug)]
pub struct ServerPlayerGameMode {
    pub game_mode_for_player: GameType,
    pub previous_game_mode_for_player: Option<GameType>,
    pub is_destroying_block: bool,
    pub destroy_progress_start: i32,
    pub destroy_pos: BlockPos,
    pub destroy_direction: mc_core::direction::Direction,
    pub game_ticks: i32,
    pub has_delayed_destroy: bool,
    pub delayed_destroy_pos: BlockPos,
    pub delayed_tick_start: i32,
    pub last_sent_state: i32,
}

impl Default for ServerPlayerGameMode {
    fn default() -> Self {
        Self {
            game_mode_for_player: GameType::Survival,
            previous_game_mode_for_player: None,
            is_destroying_block: false,
            destroy_progress_start: 0,
            destroy_pos: BlockPos::ZERO,
            destroy_direction: mc_core::direction::Direction::Down,
            game_ticks: 0,
            has_delayed_destroy: false,
            delayed_destroy_pos: BlockPos::ZERO,
            delayed_tick_start: 0,
            last_sent_state: -1,
        }
    }
}

pub struct ServerPlayer {
    pub player: PlayerData,
    /// `None` for a player whose connection is gone / test players.
    pub connection: Option<ConnectionSender>,
    pub game_mode: ServerPlayerGameMode,
    pub client_information: ClientInformation,
    pub last_recorded_health_and_absorption: f32,
    pub last_recorded_food_level: i32,
    pub last_recorded_air_level: i32,
    pub last_recorded_armor: i32,
    pub last_recorded_level: i32,
    pub last_recorded_experience: i32,
    pub last_sent_health: f32,
    pub last_sent_food: i32,
    pub last_food_saturation_zero: bool,
    pub last_sent_exp: i32,
    pub chat_visibility: ChatVisibility,
    pub particle_status: ParticleStatus,
    pub can_chat_color: bool,
    pub last_action_time: i64,
    pub camera: Option<EntityId>,
    pub is_changing_dimension: bool,
    pub seen_credits: bool,
    /// `recipeBook` (`ServerRecipeBook.Packed`), kept as the saved tag until the recipe slice models it.
    pub recipe_book: Option<Tag>,
    pub levitation_start_pos: Option<Vec3>,
    pub levitation_start_time: i32,
    pub disconnected: bool,
    pub requested_view_distance: i32,
    pub language: String,
    pub starting_to_fall_position: Option<Vec3>,
    pub entered_nether_position: Option<Vec3>,
    pub entered_lava_on_vehicle_position: Option<Vec3>,
    pub current_explosion_impact_pos: Option<Vec3>,
    pub last_section_pos: SectionPos,
    pub respawn_config: Option<RespawnConfig>,
    pub text_filtering_enabled: bool,
    pub allows_listing: bool,
    pub spawn_extra_particles_on_fall: bool,
    pub warden_spawn_tracker: WardenSpawnTracker,
    pub raid_omen_position: Option<BlockPos>,
    pub last_known_client_movement: Vec3,
    pub last_client_input: Input,
    pub ender_pearls: Vec<CompoundTag>,
    pub post_effects_dirty: bool,
    pub shoulder_entity_left: CompoundTag,
    pub shoulder_entity_right: CompoundTag,
    pub container_counter: i32,
    pub won_game: bool,
    /// `Dimension` read from the saved tag (the level a player is placed in on login).
    pub saved_dimension: Option<Identifier>,
    /// `level().dimension()`: the dimension of the level this player is in (`Dimension` tag on save).
    pub dimension: Identifier,
    /// `RootVehicle` tag read from the saved data (`loadAndSpawnParentVehicle` consumes it).
    pub saved_root_vehicle: Option<CompoundTag>,
}

impl ServerPlayer {
    pub fn base_id(&self) -> EntityId {
        self.player.living.entity.id
    }

    /// `ServerPlayer(server, level, gameProfile, clientInformation)`; `id` = `level.getNextEntityId()`.
    pub fn new(id: EntityId, dimension: Identifier, game_profile: GameProfile, client_information: ClientInformation) -> Self {
        let mut p = Self {
            player: PlayerData::new(id, game_profile),
            connection: None,
            game_mode: ServerPlayerGameMode::default(),
            client_information: ClientInformation::default(),
            last_recorded_health_and_absorption: f32::MIN_POSITIVE,
            last_recorded_food_level: i32::MIN,
            last_recorded_air_level: i32::MIN,
            last_recorded_armor: i32::MIN,
            last_recorded_level: i32::MIN,
            last_recorded_experience: i32::MIN,
            last_sent_health: -1.0e8,
            last_sent_food: -99_999_999,
            last_food_saturation_zero: true,
            last_sent_exp: -99_999_999,
            chat_visibility: ChatVisibility::Full,
            particle_status: ParticleStatus::All,
            can_chat_color: true,
            last_action_time: 0,
            camera: None,
            is_changing_dimension: false,
            seen_credits: false,
            recipe_book: None,
            levitation_start_pos: None,
            levitation_start_time: 0,
            disconnected: false,
            requested_view_distance: 2,
            language: "en_us".into(),
            starting_to_fall_position: None,
            entered_nether_position: None,
            entered_lava_on_vehicle_position: None,
            current_explosion_impact_pos: None,
            last_section_pos: SectionPos::of(0, 0, 0),
            respawn_config: None,
            text_filtering_enabled: false,
            allows_listing: false,
            spawn_extra_particles_on_fall: false,
            warden_spawn_tracker: WardenSpawnTracker::default(),
            raid_omen_position: None,
            last_known_client_movement: Vec3::ZERO,
            last_client_input: Input::default(),
            ender_pearls: Vec::new(),
            post_effects_dirty: true,
            shoulder_entity_left: CompoundTag::new(),
            shoulder_entity_right: CompoundTag::new(),
            container_counter: 0,
            won_game: false,
            saved_dimension: None,
            dimension,
            saved_root_vehicle: None,
        };
        p.update_options(client_information);
        p
    }

    /// `updateOptions(clientInformation)`.
    pub fn update_options(&mut self, info: ClientInformation) {
        self.language = info.language.clone();
        self.requested_view_distance = i32::from(info.view_distance);
        self.chat_visibility = info.chat_visibility;
        self.can_chat_color = info.chat_colors;
        self.text_filtering_enabled = info.text_filtering_enabled;
        self.allows_listing = info.allows_listing;
        self.particle_status = info.particle_status;
        self.player.living.entity.entity_data.set(super::DATA_PLAYER_MODE_CUSTOMISATION, info.model_customisation as i8);
        self.player.set_main_arm(info.main_hand);
        self.client_information = info;
    }

    /// `ServerPlayer.send(packet)`: no-op without a connection.
    pub fn send<P: mc_protocol::packet::Packet>(&self, packet: &P) {
        if let Some(c) = &self.connection {
            c.send(packet);
        }
    }

    /// `ServerPlayerGameMode.setGameModeForPlayer(new, previous)` (`GameType.updatePlayerAbilities`).
    pub fn set_game_mode_for_player(&mut self, game_mode: GameType, previous: Option<GameType>) {
        self.game_mode.previous_game_mode_for_player = previous;
        self.game_mode.game_mode_for_player = game_mode;
        update_player_abilities(game_mode, &mut self.player.abilities);
    }

    pub fn is_creative(&self) -> bool {
        self.game_mode.game_mode_for_player == GameType::Creative
    }

    /// `loadAndSpawnParentVehicle` input: the `RootVehicle` tag of the saved data, if any.
    pub fn take_saved_root_vehicle(&mut self) -> Option<CompoundTag> {
        self.saved_root_vehicle.take()
    }

    /// `ServerPlayer.saveWithoutId` + the `DataVersion` stamp: the player data as written to `playerdata/<uuid>.dat`
    /// (`PlayerDataStorage.save`). `lookup` resolves the vehicle chain for `RootVehicle`.
    pub fn save_player_data(&self, lookup: &EntityLookupFn<'_>) -> CompoundTag {
        let mut tag = CompoundTag::new();
        Entity::save_without_id(self, &mut tag, lookup);
        tag
    }
}

/// `GameType.updatePlayerAbilities`.
pub fn update_player_abilities(game_type: GameType, abilities: &mut super::Abilities) {
    match game_type {
        GameType::Creative => {
            abilities.mayfly = true;
            abilities.instabuild = true;
            abilities.invulnerable = true;
        }
        GameType::Spectator => {
            abilities.mayfly = true;
            abilities.instabuild = false;
            abilities.invulnerable = true;
            abilities.flying = true;
        }
        _ => {
            abilities.mayfly = false;
            abilities.instabuild = false;
            abilities.invulnerable = false;
            abilities.flying = false;
        }
    }
    abilities.may_build = !matches!(game_type, GameType::Adventure | GameType::Spectator);
}

fn game_type_id(g: GameType) -> i32 {
    match g {
        GameType::Survival => 0,
        GameType::Creative => 1,
        GameType::Adventure => 2,
        GameType::Spectator => 3,
    }
}

/// `GameType.byId` (unknown ids fall back to the default mode).
fn game_type_by_id(id: i32) -> GameType {
    match id {
        1 => GameType::Creative,
        2 => GameType::Adventure,
        3 => GameType::Spectator,
        _ => GameType::Survival,
    }
}

impl Entity for ServerPlayer {
    fn base(&self) -> &EntityData {
        &self.player.living.entity
    }
    fn base_mut(&mut self) -> &mut EntityData {
        &mut self.player.living.entity
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
    fn living(&self) -> Option<&LivingEntityData> {
        Some(&self.player.living)
    }
    fn living_mut(&mut self) -> Option<&mut LivingEntityData> {
        Some(&mut self.player.living)
    }
    fn player(&self) -> Option<&PlayerData> {
        Some(&self.player)
    }
    fn player_mut(&mut self) -> Option<&mut PlayerData> {
        Some(&mut self.player)
    }
    fn is_player(&self) -> bool {
        true
    }
    fn is_always_ticking(&self) -> bool {
        true
    }
    fn is_spectator(&self) -> bool {
        self.game_mode.game_mode_for_player == GameType::Spectator
    }
    fn get_dimensions(&self, pose: Pose) -> EntityDimensions {
        super::avatar_dimensions(pose)
    }
    fn get_encode_id(&self) -> Option<&'static str> {
        Some("minecraft:player")
    }

    fn add_additional_save_data(&self, out: &mut CompoundTag) {
        let p = &self.player;
        living::add_additional_save_data(&p.living, p.get_absorption_amount(), p.inventory.equipment(), out);
        // Player
        out.put_int("DataVersion", mc_world::region::entity_storage::CURRENT_DATA_VERSION);
        out.put("Inventory", p.inventory.save());
        out.put_int("SelectedItemSlot", p.inventory.get_selected_slot());
        out.put_short("SleepTimer", p.sleep_counter as i16);
        out.put_float("XpP", p.experience_progress);
        out.put_int("XpLevel", p.experience_level);
        out.put_int("XpTotal", p.total_experience);
        out.put_int("XpSeed", p.enchantment_seed);
        out.put_int("Score", p.get_score());
        p.food_data.add_additional_save_data(out);
        out.put("abilities", Tag::Compound(p.abilities.pack()));
        out.put("EnderItems", crate::world::container::save_slots(p.ender_chest.get_items().iter().enumerate()));
        if let Some(pos) = &p.last_death_location {
            let mut g = CompoundTag::new();
            g.put_string("dimension", pos.dimension.to_string());
            g.put_int_array("pos", vec![pos.pos.x(), pos.pos.y(), pos.pos.z()]);
            out.put("LastDeathLocation", Tag::Compound(g));
        }
        // ServerPlayer
        let w = &self.warden_spawn_tracker;
        let mut wt = CompoundTag::new();
        wt.put_int("ticks_since_last_warning", w.ticks_since_last_warning);
        wt.put_int("warning_level", w.warning_level);
        wt.put_int("cooldown_ticks", w.cooldown_ticks);
        out.put("warden_spawn_tracker", Tag::Compound(wt));
        out.put_int("playerGameType", game_type_id(self.game_mode.game_mode_for_player));
        if let Some(prev) = self.game_mode.previous_game_mode_for_player {
            out.put_int("previousPlayerGameType", game_type_id(prev));
        }
        out.put_boolean("seenCredits", self.seen_credits);
        put_vec3(out, "entered_nether_pos", self.entered_nether_position);
        put_vec3(out, "last_explosion_impact_pos", self.current_explosion_impact_pos);
        if let Some(book) = &self.recipe_book {
            out.put("recipeBook", book.clone());
        }
        if let Some(r) = &self.respawn_config {
            out.put("respawn", r.encode());
        }
        out.put_boolean("spawn_extra_particles_on_fall", self.spawn_extra_particles_on_fall);
        if let Some(pos) = self.raid_omen_position {
            out.put_int_array("raid_omen_position", vec![pos.x(), pos.y(), pos.z()]);
        }
        if !self.ender_pearls.is_empty() {
            let mut l = ListTag::new();
            for p in &self.ender_pearls {
                l.push(Tag::Compound(p.clone()));
            }
            out.put("ender_pearls", l);
        }
        let mut post = ListTag::new();
        for id in &p.post_effects {
            post.push(Tag::String(id.to_string()));
        }
        out.put("post_effects", post);
        if !self.shoulder_entity_left.is_empty() {
            out.put("ShoulderEntityLeft", Tag::Compound(self.shoulder_entity_left.clone()));
        }
        if !self.shoulder_entity_right.is_empty() {
            out.put("ShoulderEntityRight", Tag::Compound(self.shoulder_entity_right.clone()));
        }
    }

    fn read_additional_save_data(&mut self, input: &CompoundTag) {
        let (absorption, equipment) = living::read_additional_save_data(&mut self.player.living, input);
        self.player.inventory.equipment_mut().set_all(&equipment);
        // Player
        if let Some(list) = input.get_list("Inventory") {
            self.player.inventory.load(list);
        } else {
            self.player.inventory.load(&ListTag::new());
        }
        let _ = self.player.inventory.set_selected_slot(input.get_int_or("SelectedItemSlot", 0));
        {
            use crate::world::container::Container;
            let chest = &mut self.player.ender_chest;
            chest.clear_content();
            if let Some(list) = input.get_list("EnderItems") {
                for (slot, stack) in crate::world::container::load_slots(list) {
                    if slot < chest.get_container_size() {
                        chest.set_item(slot, stack);
                    }
                }
            }
        }
        let uuid = self.player.game_profile.id;
        self.player.living.entity.uuid = uuid;
        self.player.sleep_counter = i32::from(input.get_short_or("SleepTimer", 0));
        self.player.experience_progress = input.get_float_or("XpP", 0.0);
        self.player.experience_level = input.get_int_or("XpLevel", 0);
        self.player.total_experience = input.get_int_or("XpTotal", 0);
        self.player.enchantment_seed = input.get_int_or("XpSeed", 0);
        if self.player.enchantment_seed == 0 {
            use mc_core::random::RandomSource;
            self.player.enchantment_seed = self.player.living.entity.random.next_int();
        }
        self.player.set_score(input.get_int_or("Score", 0));
        self.player.internal_set_absorption_amount(absorption);
        self.player.food_data.read_additional_save_data(input);
        if let Some(a) = input.get_compound("abilities") {
            self.player.abilities.apply(a);
        }
        let walk = f64::from(self.player.abilities.get_walking_speed());
        if let Some(i) = self.player.living.attributes.get_instance(mc_data::Attribute::MOVEMENT_SPEED) {
            i.set_base_value(walk);
        }
        self.player.last_death_location = input.get_compound("LastDeathLocation").and_then(|g| {
            let dimension = Identifier::parse(g.get_string("dimension")?).ok()?;
            let pos = living::read_block_pos(g.get("pos"))?;
            Some(GlobalPos { dimension, pos })
        });
        // ServerPlayer
        self.warden_spawn_tracker = input
            .get_compound("warden_spawn_tracker")
            .map(|w| WardenSpawnTracker {
                ticks_since_last_warning: w.get_int_or("ticks_since_last_warning", 0).max(0),
                warning_level: w.get_int_or("warning_level", 0).max(0),
                cooldown_ticks: w.get_int_or("cooldown_ticks", 0).max(0),
            })
            .unwrap_or_default();
        self.entered_nether_position = entity::read_vec3(input.get("entered_nether_pos"));
        self.current_explosion_impact_pos = entity::read_vec3(input.get("last_explosion_impact_pos"));
        self.seen_credits = input.get_boolean_or("seenCredits", false);
        self.recipe_book = input.get("recipeBook").cloned();
        self.respawn_config = input.get("respawn").and_then(|t| RespawnConfig::decode(t).ok());
        self.spawn_extra_particles_on_fall = input.get_boolean_or("spawn_extra_particles_on_fall", false);
        self.raid_omen_position = living::read_block_pos(input.get("raid_omen_position"));
        let current = input.get("playerGameType").and_then(|t| t.as_number()).map(|n| game_type_by_id(n.int_value()));
        let previous = input.get("previousPlayerGameType").and_then(|t| t.as_number()).map(|n| game_type_by_id(n.int_value()));
        self.set_game_mode_for_player(current.unwrap_or(GameType::Survival), previous);
        self.player.post_effects.clear();
        if let Some(list) = input.get_list("post_effects") {
            self.player.post_effects.extend(list.iter().filter_map(|t| t.as_string()).filter_map(|s| Identifier::parse(s).ok()));
        }
        self.shoulder_entity_left = input.get_compound("ShoulderEntityLeft").cloned().unwrap_or_default();
        self.shoulder_entity_right = input.get_compound("ShoulderEntityRight").cloned().unwrap_or_default();
        self.ender_pearls = input.get_list("ender_pearls").map(|l| l.iter().filter_map(|t| t.as_compound().cloned()).collect()).unwrap_or_default();
        self.saved_dimension = input.get_string("Dimension").and_then(|s| Identifier::parse(s).ok());
        self.saved_root_vehicle = input.get_compound("RootVehicle").cloned();
    }

    fn save_without_id(&self, out: &mut CompoundTag, lookup: &EntityLookupFn<'_>) {
        entity::save_without_id(self, out, lookup);
        out.put_string("Dimension", self.dimension.to_string());
        self.save_parent_vehicle(out, lookup);
    }
}

fn put_vec3(out: &mut CompoundTag, key: &str, v: Option<Vec3>) {
    if let Some(v) = v {
        let mut list = ListTag::new();
        list.push(Tag::double(v.x));
        list.push(Tag::double(v.y));
        list.push(Tag::double(v.z));
        out.put(key, list);
    }
}

impl ServerPlayer {
    /// `saveParentVehicle`: the root vehicle (with all its passengers) when this player rides something and is the
    /// only player in the root vehicle.
    fn save_parent_vehicle(&self, out: &mut CompoundTag, lookup: &EntityLookupFn<'_>) {
        let Some(vehicle_id) = self.base().vehicle else { return };
        let mut root_id = vehicle_id;
        while let Some(parent) = lookup(root_id).and_then(|e| e.base().vehicle) {
            root_id = parent;
        }
        let Some(root) = lookup(root_id) else { return };
        if root_id == self.base().id || count_players(root, lookup) != 1 {
            return;
        }
        let Some(vehicle) = lookup(vehicle_id) else { return };
        let mut wrapper = CompoundTag::new();
        wrapper.put_int_array("Attach", mc_core::uuid_util::uuid_to_int_array(&vehicle.uuid()).to_vec());
        if let Some(root_tag) = entity::save(root, lookup) {
            wrapper.put("Entity", Tag::Compound(root_tag));
        }
        out.put("RootVehicle", Tag::Compound(wrapper));
    }
}

/// Number of players among `e` and its indirect passengers (`countPlayerPassengers` + self).
fn count_players(e: &dyn Entity, lookup: &EntityLookupFn<'_>) -> usize {
    usize::from(e.is_player()) + e.base().passengers.iter().filter_map(|&p| lookup(p)).map(|p| count_players(p, lookup)).sum::<usize>()
}
