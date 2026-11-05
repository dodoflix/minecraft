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
    /// `connection` in vanilla (`ServerGamePacketListenerImpl`): the play listener state; `connection` above is its
    /// send handle. Set by `PlayerList.placeNewPlayer`.
    pub listener: Option<crate::server::network::game_listener::ServerGamePacketListenerImpl>,
    /// The player list's view of this player (`None` until `placeNewPlayer` gives it a connection).
    pub handle: Option<crate::server::players::PlayerHandle>,
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
    pub recipe_book: crate::world::item::crafting::ServerRecipeBook,
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
    /// Set when digging was refused by spawn protection (`sendSpawnProtectionMessage(pos)` is the listener's).
    /// Stacks `Player.drop` produced; the item-entity slice turns them into entities.
    pub pending_drops: Vec<(crate::world::item::ItemStack, bool)>,
    /// `Dimension` read from the saved tag (the level a player is placed in on login).
    pub saved_dimension: Option<Identifier>,
    /// `level().dimension()`: the dimension of the level this player is in (`Dimension` tag on save).
    pub dimension: Identifier,
    /// `RootVehicle` tag read from the saved data (`loadAndSpawnParentVehicle` consumes it).
    pub saved_root_vehicle: Option<CompoundTag>,
    // ==== [Player] additions (ServerPlayer field) ====
    /// `stats`: the player's statistics (shared with `PlayerList.stats`).
    pub stats: crate::stats::SharedStats,
    /// `timeEntitySatOnShoulder` (game time).
    pub time_entity_sat_on_shoulder: i64,
    // ==== [Level] additions (ServerPlayer field) ====
    // ==== [LivingCore] additions (ServerPlayer field) ====
    /// The server simulates this player's physics like a locally controlled `LocalPlayer` from `last_client_input` (the
    /// oracle/test mode; real vanilla players are client-authoritative and the server only validates their movement).
    pub simulate_local_player: bool,
    // ==== [Damage] additions (ServerPlayer field) ====
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
            handle: None,
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
            recipe_book: Default::default(),
            levitation_start_pos: None,
            levitation_start_time: 0,
            listener: None,
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
            pending_drops: Vec::new(),
            saved_dimension: None,
            dimension,
            saved_root_vehicle: None,
            // ==== [Player] additions (ServerPlayer::new initializer) ====
            stats: crate::stats::SharedStats::new(crate::stats::ServerStatsCounter::detached()),
            time_entity_sat_on_shoulder: 0,
            // ==== [Level] additions (ServerPlayer::new initializer) ====
            // ==== [LivingCore] additions (ServerPlayer::new initializer) ====
            simulate_local_player: false,
            // ==== [Damage] additions (ServerPlayer::new initializer) ====
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

    /// `ServerPlayer.openDialog(dialog)`: a registered dialog (by name) or an inline one.
    pub fn open_dialog(&self, dialog: &mc_protocol::components::Holder<mc_protocol::dialog::Dialog>) {
        match mc_protocol::dialog::Dialog::wire_holder(dialog) {
            Some(dialog) => self.send(&mc_protocol::packets::play::ClientboundShowDialogPacket { dialog }),
            None => log::error!("Can't open a dialog that is not in the dialog registry: {dialog:?}"),
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

use crate::world::entity::living_entity::effects::remove_effect_particles as living_effects_remove_particles;

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
    fn as_dyn(&self) -> &dyn Entity {
        self
    }
    fn as_dyn_mut(&mut self) -> &mut dyn Entity {
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
    /// `getItemBySlot(slot)`: a player's equipment lives in its inventory (`PlayerEquipment`).
    fn get_item_by_slot(&self, slot: crate::world::entity::equipment::EquipmentSlot) -> crate::world::item::ItemStack {
        self.player.inventory.equipment_get(slot).clone()
    }
    fn item_by_slot_mut(&mut self, slot: crate::world::entity::equipment::EquipmentSlot) -> Option<&mut crate::world::item::ItemStack> {
        Some(self.player.inventory.equipment_get_mut(slot))
    }
    /// `ServerPlayer.tick()`.
    fn tick(&mut self, level: &mut crate::server::level::ServerLevel) {
        self.tick_server_player(level);
    }
    /// `ServerPlayer.processPortalCooldown()`: frozen until the client confirmed the dimension change.
    fn process_portal_cooldown(&mut self) {
        if !self.is_changing_dimension && self.base().portal_cooldown > 0 {
            self.base_mut().portal_cooldown -= 1;
        }
    }
    /// `ServerPlayer.removeVehicle()`: the effects of the old vehicle are removed client side, the passenger list resent.
    fn on_vehicle_removed(&mut self, level: &mut crate::server::level::ServerLevel, old_vehicle: crate::world::entity::EntityId, held: Option<&dyn Entity>) {
        let vehicle = held.or_else(|| level.entity_manager.get_any(old_vehicle));
        let Some(vehicle) = vehicle else { return };
        if let Some(living) = vehicle.living() {
            for effect in living.active_effects.keys() {
                self.send(&mc_protocol::packets::play::ClientboundRemoveMobEffectPacket { entity_id: old_vehicle, effect: *effect });
            }
        }
        self.send(&mc_protocol::packets::play::ClientboundSetPassengersPacket { vehicle: old_vehicle, passengers: vehicle.base().passengers.clone() });
    }
    /// `Player.isClientAuthoritative()`.
    fn is_client_authoritative(&self) -> bool {
        !self.simulate_local_player
    }
    /// `LocalPlayer.applyInput()` for a simulated player (keyboard input -> movement vector, jump, sprint).
    fn apply_input(&mut self) {
        if !self.simulate_local_player {
            let l = &mut self.player.living;
            l.xxa *= 0.98;
            l.zza *= 0.98;
            return;
        }
        let input = self.last_client_input;
        let mut forward = f32::from(u8::from(input.forward)) - f32::from(u8::from(input.backward));
        let mut sideways = f32::from(u8::from(input.left)) - f32::from(u8::from(input.right));
        self.set_shift_key_down(input.shift);
        if input.shift && !self.player.abilities.flying {
            let sneak = self.player.living.get_attribute_value(mc_data::Attribute::SNEAKING_SPEED) as f32;
            forward *= sneak;
            sideways *= sneak;
        }
        let l = &mut self.player.living;
        l.zza = forward;
        l.xxa = sideways;
        l.jumping = input.jump;
        self.set_sprinting(input.sprint && input.forward);
    }
    // ==== [LivingCore] additions (impl Entity for ServerPlayer) ====
    /// `ServerPlayer.teleportTo(x, y, z)`: a client teleport (`Relative.DELTA | ROTATION`).
    fn teleport_to(&mut self, level: &mut crate::server::level::ServerLevel, x: f64, y: f64, z: f64) {
        use mc_protocol::packets::play::{PositionMoveRotation, Relatives};
        if self.listener.is_none() {
            let (yr, xr) = (self.base().y_rot(), self.base().x_rot());
            self.snap_to(x, y, z, yr, xr);
            return;
        }
        let _ = level;
        self.connection_teleport_pmr(
            PositionMoveRotation { position: Vec3::new(x, y, z), delta_movement: Vec3::ZERO, y_rot: 0.0, x_rot: 0.0 },
            Relatives::DELTA.union(Relatives::ROTATION),
        );
    }
    /// `Player.updateSwimming()`.
    fn update_swimming(&mut self, level: &crate::server::level::ServerLevel) {
        if self.player.abilities.flying {
            self.set_swimming(false);
        } else {
            // `Entity.updateSwimming`, which reads the raw flag through `isSwimming()`
            let was_swimming = self.is_swimming();
            let in_passenger = self.base().is_passenger();
            let sprinting = self.is_sprinting();
            let swimming = if was_swimming {
                sprinting && self.is_in_water() && !in_passenger
            } else {
                sprinting
                    && self.is_under_water()
                    && !in_passenger
                    && crate::world::level::material::FluidStateExt::is_tag(crate::world::level::BlockGetter::get_fluid_state(level, self.base().block_position()), "water")
            };
            self.set_swimming(swimming);
        }
    }
    /// `Player.getMaxHeadRotationRelativeToBody()`.
    fn get_max_head_rotation_relative_to_body(&self) -> f32 {
        if self.is_blocking() { 15.0 } else { 50.0 }
    }
    /// `Player.doesEmitEquipEvent(slot)`.
    fn does_emit_equip_event(&self, slot: crate::world::entity::equipment::EquipmentSlot) -> bool {
        slot.get_type() == crate::world::entity::equipment::EquipmentSlotType::HumanoidArmor
    }
    /// `Player.aiStep()`.
    fn ai_step(&mut self, level: &mut crate::server::level::ServerLevel) {
        self.player_ai_step(level);
    }
    /// `Player.travel(input)`.
    fn travel(&mut self, level: &mut crate::server::level::ServerLevel, input: Vec3) {
        self.player_travel(level, input);
    }
    /// `Player.isImmobile()`: dead or sleeping.
    fn is_immobile(&self) -> bool {
        self.player.living.is_dead_or_dying() || self.player.living.is_sleeping()
    }
    /// `Player.isAffectedByFluids()`.
    fn is_affected_by_fluids(&self) -> bool {
        !self.player.abilities.flying
    }
    /// `Player.getSpeed()`: the `MOVEMENT_SPEED` attribute.
    fn get_speed(&self) -> f32 {
        self.player.living.get_attribute_value(mc_data::Attribute::MOVEMENT_SPEED) as f32
    }
    /// `Player.getFlyingSpeed()`.
    fn get_flying_speed(&self, _level: &crate::server::level::ServerLevel) -> f32 {
        let sprinting = self.base().get_shared_flag(entity::FLAG_SPRINTING);
        if self.player.abilities.flying && !self.base().is_passenger() {
            if sprinting { self.player.abilities.get_flying_speed() * 2.0 } else { self.player.abilities.get_flying_speed() }
        } else if sprinting {
            0.025999999
        } else {
            0.02
        }
    }
    /// `Player.onClimbable()`.
    fn on_climbable(&self, level: &crate::server::level::ServerLevel) -> bool {
        !self.player.abilities.flying && crate::world::entity::living_entity::travel::on_climbable(self, level)
    }
    /// `Player.canGlide()`.
    fn can_glide(&self) -> bool {
        !self.player.abilities.flying && crate::world::entity::living_entity::fall_flying::can_glide(self)
    }
    /// `Player.getBlockSpeedFactor()`.
    fn modify_block_speed_factor(&self, base: f32) -> f32 {
        if !self.player.abilities.flying && !self.player.living.is_fall_flying() {
            mc_core::math::mth::lerp_f32(self.player.living.get_attribute_value(mc_data::Attribute::MOVEMENT_EFFICIENCY) as f32, base, 1.0)
        } else {
            1.0
        }
    }
    /// `Player.isPickable()`.
    fn is_pickable(&self) -> bool {
        !self.is_spectator() && !self.is_removed()
    }
    /// `Player.canBeSeenAsEnemy()`.
    fn can_be_seen_as_enemy(&self) -> bool {
        !self.player.abilities.invulnerable && !self.base().permanently_invulnerable && !self.is_spectator() && self.is_alive()
    }
    /// `EntityEquipment.tick(this)`.
    fn tick_equipment(&mut self, level: &mut crate::server::level::ServerLevel) {
        self.tick_equipment_items(level);
    }
    /// `updatingUsingItem()`.
    fn updating_using_item(&mut self, level: &mut crate::server::level::ServerLevel) {
        ServerPlayer::updating_using_item(self, level);
    }
    /// `Player.getFallSounds()`.
    fn get_fall_sounds(&self) -> (mc_data::SoundEvent, mc_data::SoundEvent) {
        (mc_data::SoundEvent::ENTITY_PLAYER_SMALL_FALL, mc_data::SoundEvent::ENTITY_PLAYER_BIG_FALL)
    }
    /// `ItemStack.hurtAndBreak(amount, this, slot)` on a player's equipped item.
    fn hurt_and_break_equipment(&mut self, level: &mut crate::server::level::ServerLevel, slot: crate::world::entity::equipment::EquipmentSlot, amount: i32) {
        let mut stack = self.player.inventory.equipment_get(slot).clone();
        crate::world::item::hurt_and_break(&mut stack, amount, level, self, slot);
        self.player.inventory.equipment_set(slot, stack);
    }
    // ==== [LivingCore] end ====
    fn move_simulation_type(&self) -> crate::world::entity::MoveSimulationType {
        crate::world::entity::MoveSimulationType::AuthoritativeSideAndServer
    }
    /// `Player.maybeBackOffFromEdge`: `!abilities.flying && isStayingOnGroundSurface()` (shift key).
    fn stays_on_ground_surface_at_edges(&self) -> bool {
        !self.player.abilities.flying && self.is_shift_key_down()
    }
    fn held_item_for_collision(&self) -> Option<mc_data::Item> {
        let stack = self.player.inventory.get_selected_item();
        (!stack.is_empty()).then(|| stack.item())
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
    /// `getStats().getValue(statType.get(stat))`.
    fn get_stat_value(&self, stat_type: &mc_core::ident::Identifier, stat: &mc_core::ident::Identifier) -> i32 {
        crate::stats::stat::by_names(stat_type, stat).map_or(0, |s| self.stats.lock().get_value(&s))
    }
    /// `Player.isSwimming()`.
    fn is_swimming(&self) -> bool {
        !self.player.abilities.flying && !self.is_spectator() && self.player.living.entity.get_shared_flag(crate::world::entity::entity::FLAG_SWIMMING)
    }
    /// `Player.getScoreboardName()`: the profile name.
    fn get_scoreboard_name(&self) -> String {
        self.player.game_profile.name.clone()
    }
    // ==== [Player] additions (Entity impl) ====
    /// `ServerPlayer.jumpFromGround()`: the jump stat and the exhaustion after `LivingEntity.jumpFromGround`.
    fn jump_from_ground(&mut self, level: &crate::server::level::ServerLevel) {
        crate::world::entity::living_entity::jump::jump_from_ground(self, level);
        self.after_jump_from_ground(level);
    }
    // ==== [Effects] additions (Entity impl) ====
    /// `ServerPlayer.updateInvisibilityStatus()`: spectators are invisible and show no effect particles.
    fn update_invisibility_status(&mut self) {
        if self.is_spectator() {
            living_effects_remove_particles(&mut self.player.living);
            self.player.living.entity.set_shared_flag(crate::world::entity::entity::FLAG_INVISIBLE, true);
        } else {
            crate::world::entity::living_entity::effects::update_invisibility_status(self);
        }
    }
    fn on_effect_added(&mut self, level: &mut crate::server::level::ServerLevel, effect: &crate::world::entity::effect::MobEffectInstance, source: Option<EntityId>) {
        self.server_player_on_effect_added(level, effect, source);
    }
    fn on_effect_updated(&mut self, level: &mut crate::server::level::ServerLevel, effect: &crate::world::entity::effect::MobEffectInstance, do_refresh_attributes: bool, source: Option<EntityId>) {
        self.server_player_on_effect_updated(level, effect, do_refresh_attributes, source);
    }
    fn on_effects_removed(&mut self, level: &mut crate::server::level::ServerLevel, effects: &[crate::world::entity::effect::MobEffectInstance]) {
        self.server_player_on_effects_removed(level, effects);
    }

    // ==== [Damage] Entity overrides (Player / ServerPlayer damage behaviour, see player/damage.rs) ====
    fn is_invulnerable_to(&mut self, level: &mut crate::server::level::ServerLevel, source: &crate::world::damagesource::DamageSource) -> bool {
        super::damage::is_invulnerable_to(self, level, source)
    }
    fn hurt_server(&mut self, level: &mut crate::server::level::ServerLevel, source: &crate::world::damagesource::DamageSource, amount: f32) -> bool {
        super::damage::hurt_server(self, level, source, amount)
    }
    fn die(&mut self, level: &mut crate::server::level::ServerLevel, source: &crate::world::damagesource::DamageSource) {
        super::damage::die(self, level, source);
    }
    fn actually_hurt(&mut self, level: &mut crate::server::level::ServerLevel, source: &crate::world::damagesource::DamageSource, damage: f32) {
        super::damage::actually_hurt(self, level, source, damage);
    }
    fn hurt_armor(&mut self, level: &mut crate::server::level::ServerLevel, source: &crate::world::damagesource::DamageSource, damage: f32) {
        super::damage::hurt_armor(self, level, source, damage);
    }
    fn hurt_helmet(&mut self, level: &mut crate::server::level::ServerLevel, source: &crate::world::damagesource::DamageSource, damage: f32) {
        super::damage::hurt_helmet(self, level, source, damage);
    }
    fn get_hurt_sound(&self, source: &crate::world::damagesource::DamageSource) -> Option<mc_data::SoundEvent> {
        Some(super::damage::get_hurt_sound(source))
    }
    fn get_death_sound(&self) -> Option<mc_data::SoundEvent> {
        Some(mc_data::SoundEvent::ENTITY_PLAYER_DEATH)
    }
    /// `Player.getVoicePitch()`.
    fn get_voice_pitch(&mut self) -> f32 {
        1.0
    }
    fn indicate_damage(&mut self, _level: &mut crate::server::level::ServerLevel, xd: f64, zd: f64) {
        super::damage::indicate_damage(self, xd, zd);
    }
    fn block_using_item(
        &mut self,
        level: &mut crate::server::level::ServerLevel,
        attacker: &crate::world::damagesource::DamageEntity,
        source: &crate::world::damagesource::DamageSource,
        damage: f32,
        fully_blocked: bool,
    ) {
        super::damage::block_using_item(self, level, attacker, source, damage, fully_blocked);
    }
    fn cause_fall_damage(&mut self, level: &mut crate::server::level::ServerLevel, fall_distance: f64, damage_modifier: f32, source: &crate::world::damagesource::DamageSource) -> bool {
        super::damage::cause_fall_damage(self, level, fall_distance, damage_modifier, source)
    }
    fn on_enter_combat(&mut self, _level: &mut crate::server::level::ServerLevel) {
        super::damage::on_enter_combat(self);
    }
    fn on_leave_combat(&mut self, _level: &mut crate::server::level::ServerLevel) {
        super::damage::on_leave_combat(self);
    }
    fn drop_equipment(&mut self, level: &mut crate::server::level::ServerLevel) {
        super::damage::drop_equipment(self, level);
    }
    fn get_base_experience_reward(&mut self, level: &crate::server::level::ServerLevel) -> i32 {
        super::damage::get_base_experience_reward(self, level)
    }
    fn is_always_experience_dropper(&self) -> bool {
        true
    }
    fn snapshot_for_damage(&self) -> Option<Box<dyn Entity>> {
        Some(super::damage::snapshot_for_damage(self))
    }
    fn killed_entity(&mut self, level: &mut crate::server::level::ServerLevel, victim: &crate::world::damagesource::DamageEntity, _source: &crate::world::damagesource::DamageSource) -> bool {
        super::damage::killed_entity(self, level, victim)
    }
    fn award_kill_score(&mut self, level: &mut crate::server::level::ServerLevel, victim: &crate::world::damagesource::DamageEntity, source: &crate::world::damagesource::DamageSource) {
        super::damage::award_kill_score(self, level, victim, source);
    }

    fn start_sleeping(&mut self, level: &mut crate::server::level::ServerLevel, bed_position: BlockPos) -> bool {
        if !super::super::living_entity::sleeping::start_sleeping(self, level, bed_position) {
            return false;
        }
        self.after_start_sleeping(level);
        true
    }
    /// `Player.stopSleeping()`: `stopSleepInBed(true, true)`.
    fn stop_sleeping(&mut self, level: &mut crate::server::level::ServerLevel) {
        self.stop_sleep_in_bed_impl(level, true, true);
    }
    fn stop_sleep_in_bed(&mut self, level: &mut crate::server::level::ServerLevel, force_wake_up: bool, update_level_list: bool) {
        self.stop_sleep_in_bed_impl(level, force_wake_up, update_level_list);
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
        // `recipeBook.pack()`
        out.put("recipeBook", self.recipe_book.pack());
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
        // validated against the recipe manager at join (`ServerRecipeBook::retain_known`)
        self.recipe_book = Default::default();
        if let Some(book) = input.get("recipeBook") {
            self.recipe_book.load_untrusted(book, |_| true);
        }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::server::level::ServerLevel;
    use crate::server::network::Outbound;
    use mc_protocol::codec::read_varint;
    use mc_protocol::packet::Packet;
    use mc_protocol::packets::play::{ClientboundSetExperiencePacket, ClientboundSetHealthPacket};

    #[test]
    fn do_tick_applies_gravity_and_air_drag_like_vanilla() {
        let mut level = ServerLevel::detached(Identifier::parse("minecraft:overworld").unwrap());
        let mut p = ServerPlayer::new(1, Identifier::parse("minecraft:overworld").unwrap(), GameProfile { id: uuid::Uuid::from_u128(1), name: "A".into(), properties: vec![] }, Default::default());
        p.set_pos(0.0, 100.0, 0.0);
        p.do_tick(&mut level);
        // dm.y = (0 - 0.08) * 0.98 (float 0.98 widened to double)
        assert_eq!(p.base().delta_movement().y, -0.08 * f64::from(0.98f32));
        assert_eq!(p.base().y(), 100.0);
        p.do_tick(&mut level);
        assert!(p.base().y() < 100.0);
    }

    #[test]
    fn do_tick_sends_health_and_xp_once_then_only_changes() {
        let mut level = ServerLevel::detached(Identifier::parse("minecraft:overworld").unwrap());
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let mut p = ServerPlayer::new(1, Identifier::parse("minecraft:overworld").unwrap(), GameProfile { id: uuid::Uuid::from_u128(1), name: "A".into(), properties: vec![] }, Default::default());
        p.connection = Some(crate::server::network::ConnectionSender::new(tx));
        p.do_tick(&mut level);
        let mut ids = vec![];
        while let Ok(Outbound::Payload(b)) = rx.try_recv() {
            ids.push(read_varint(&mut &b[..]).unwrap());
        }
        assert_eq!(ids, vec![ClientboundSetHealthPacket::ID, ClientboundSetExperiencePacket::ID]);
        p.do_tick(&mut level);
        assert!(rx.try_recv().is_err(), "nothing changed");
        p.player.food_data.food_level = 10;
        p.do_tick(&mut level);
        assert!(matches!(rx.try_recv(), Ok(Outbound::Payload(_))));
    }
}
