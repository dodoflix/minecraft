//! `net.minecraft.world.entity.animal.happyghast.HappyGhast`: a ghastling that floats on a brain and grows into a harnessed
//! flying mount for up to four riders, which stays still while a player stands on it and holds leashes at four points.

use super::happy_ghast_ai;
use linkme::distributed_slice;
use mc_core::math::{java, mth};
use mc_core::phys::{AABB, Vec3};
use mc_core::pos::BlockPos;
use mc_data::enums::{EntitySpawnReason, Pose};
use mc_data::{Attribute, EntityType, Item, SoundEvent};
use mc_game_core::define_accessor;
use mc_game_core::registry::RegistryAccess;
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::damagesource::DamageSource;
use mc_game_core::world::entity::ai::brain::{Brain, Provider};
use mc_game_core::world::entity::ai::control::body_rotation_control::{
    BodyRotationControl, BodyRotationControlBase, base_client_tick,
};
use mc_game_core::world::entity::ai::control::flying_move_control::FlyingMoveControl;
use mc_game_core::world::entity::ai::control::look_control::{
    LookControl, LookControlBase, StandardLookControl,
};
use mc_game_core::world::entity::ai::goal::{FloatGoal, Goal, GoalFlags, ItemPredicate, TemptGoal};
use mc_game_core::world::entity::ai::memory::BrainPacked;
use mc_game_core::world::entity::ai::navigation::{
    NavClass, NavigationData, PathNavigation, StandardPathNavigation, is_clear_for_movement_between,
};
use mc_game_core::world::entity::ai::sensing::sensor_type::{
    FOOD_TEMPTATIONS, HURT_BY, NEAREST_ADULT_ANY_TYPE, NEAREST_LIVING_ENTITIES, NEAREST_PLAYERS,
};
use mc_game_core::world::entity::dimensions::EntityDimensions;
use mc_game_core::world::entity::equipment::EquipmentSlot;
use mc_game_core::world::entity::factory::{
    self, ENTITY_FACTORIES, EntityRegistration, EntitySpawnRequest,
};
use mc_game_core::world::entity::living_entity::travel::travel_flying_speeds;
use mc_game_core::world::entity::mob::{self, MobData, MobKind, ageable, leashable};
use mc_game_core::world::entity::monster::ghast_movement::{
    GhastMoveControl, RandomFloatAroundGoal, face_movement_direction,
};
use mc_game_core::world::entity::movement;
use mc_game_core::world::entity::scenario_dump::{SCENARIO_DUMP_EXTRA, ScenarioDumpExtra};
use mc_game_core::world::entity::{Entity, EntityId, ServerPlayer};
use mc_game_core::world::item::{InteractionHand, InteractionResult, ItemStack};
use mc_game_core::world::level::LevelReader;
use mc_game_core::world::level::precipitation::Precipitation;
use mc_nbt::CompoundTag;
use mc_protocol::packets::play::{ClientboundEntityPositionSyncPacket, PositionPath, SoundSource};
use std::sync::Arc;

define_accessor!(pub IS_LEASH_HOLDER: Boolean = HappyGhast);
define_accessor!(pub STAYS_STILL: Boolean = HappyGhast);

/// `MAX_SCALE`.
const MAX_SCALE: f32 = 1.0;
/// `BABY_SCALE`.
const BABY_SCALE: f32 = 0.2375;
/// `SMALL_RESTRICTION_RADIUS`.
const SMALL_RESTRICTION_RADIUS: i32 = 32;
/// `LARGE_RESTRICTION_RADIUS`.
const LARGE_RESTRICTION_RADIUS: i32 = 64;
/// `RESTRICTION_RADIUS_BUFFER`.
const RESTRICTION_RADIUS_BUFFER: i32 = 16;
/// `FAST_HEALING_TICKS`.
const FAST_HEALING_TICKS: i32 = 20;
/// `SLOW_HEALING_TICKS`.
const SLOW_HEALING_TICKS: i32 = 600;
/// `MAX_PASSANGERS`.
pub const MAX_PASSENGERS: usize = 4;
/// `STILL_TIMEOUT_ON_LOAD_GRACE_PERIOD`.
const STILL_TIMEOUT_ON_LOAD_GRACE_PERIOD: i32 = 60;
/// `MAX_STILL_TIMEOUT`.
const MAX_STILL_TIMEOUT: i32 = 10;

/// `BRAIN_PROVIDER`.
static BRAIN_PROVIDER: Provider = Provider::with_activities(
    &[
        &NEAREST_LIVING_ENTITIES,
        &HURT_BY,
        &FOOD_TEMPTATIONS,
        &NEAREST_ADULT_ANY_TYPE,
        &NEAREST_PLAYERS,
    ],
    happy_ghast_ai::get_activities,
);

/// `BABY_DIMENSIONS`: the adult dimensions scaled down with a fixed eye height.
fn baby_dimensions() -> EntityDimensions {
    EntityDimensions::of_type(EntityType::HAPPY_GHAST)
        .scale(BABY_SCALE)
        .with_eye_height(0.46875)
}

/// `HappyGhast`.
pub struct HappyGhast {
    pub mob: MobData,
    /// `leashHolderTime`.
    leash_holder_time: i32,
    /// `serverStillTimeout`.
    server_still_timeout: i32,
    /// `getFirstPassenger() instanceof Player` (kept by `add_passenger` / `on_passenger_removed`).
    first_passenger_is_player: bool,
    /// What `readAdditionalSaveData` leaves for [`Entity::read_level_dependent_data`] when it ran with a level.
    pending_load: Option<PendingLoad>,
}

/// The level-dependent part of `readAdditionalSaveData`: `setAge` crossing zero runs `ageBoundaryReached()` before
/// `still_timeout` is set.
struct PendingLoad {
    age_boundary_crossed: bool,
    still_timeout: i32,
}

/// `this::isOnStillTimeout` as the `BooleanSupplier` of the move control.
fn is_stopped(mob: &dyn Entity) -> bool {
    mob.as_any()
        .downcast_ref::<HappyGhast>()
        .is_some_and(HappyGhast::is_on_still_timeout)
}

impl HappyGhast {
    /// `new HappyGhast(type, level)`.
    pub fn new(ty: EntityType, id: EntityId, _registries: &RegistryAccess) -> Self {
        let mob = MobData::new(ty, id, MobKind::Animal, |b| {
            b.define(IS_LEASH_HOLDER, false);
            b.define(STAYS_STILL, false);
        });
        let mut ghast = HappyGhast {
            mob,
            leash_holder_time: 0,
            server_still_timeout: 0,
            first_passenger_is_player: false,
            pending_load: None,
        };
        // `LivingEntity.<init>`: `this.brain = this.makeBrain(Brain.Packed.EMPTY)`
        let brain = ghast.make_brain(&BrainPacked::EMPTY);
        ghast.mob.living.brain = brain;
        let navigation = StandardPathNavigation::ground(&ghast);
        ghast.mob.set_navigation(Box::new(navigation));
        ghast.register_goals();
        ghast.set_adult_controls();
        ghast.mob.body_rotation_control = Some(Box::<HappyGhastBodyRotationControl>::default());
        ghast
    }

    /// `this.moveControl = new GhastMoveControl<>(this, true, this::isOnStillTimeout)` and the look control.
    fn set_adult_controls(&mut self) {
        self.mob.move_control = Some(Box::new(GhastMoveControl::new(true, Arc::new(is_stopped))));
        self.mob.look_control = Some(Box::<HappyGhastLookControl>::default());
    }

    /// `registerGoals()`.
    fn register_goals(&mut self) {
        let float = HappyGhastFloatGoal {
            inner: FloatGoal::new(self),
        };
        let goals = &mut self.mob.goal_selector;
        goals.add_goal(3, Box::new(float));
        goals.add_goal(
            4,
            Box::new(TemptGoal::for_non_pathfinders_of_mob(
                1.0,
                Arc::new(|mob: &dyn Entity| -> ItemPredicate {
                    let food_only = mob
                        .as_any()
                        .downcast_ref::<HappyGhast>()
                        .is_none_or(|g| g.is_wearing_body_armor() || g.is_baby());
                    let tag = if food_only {
                        "happy_ghast_food"
                    } else {
                        "happy_ghast_tempt_items"
                    };
                    Arc::new(move |stack: &ItemStack| {
                        mc_data::tags::get::<Item>(tag).is_some_and(|t| t.contains(stack.item()))
                    })
                }),
                false,
                7.0,
            )),
        );
        goals.add_goal(5, Box::new(RandomFloatAroundGoal::new(16)));
    }

    /// `adultGhastSetup()`.
    fn adult_ghast_setup(&mut self, level: &mut ServerLevel) {
        self.set_adult_controls();
        let navigation = StandardPathNavigation::ground(&*self);
        self.mob.set_navigation(Box::new(navigation));
        let mut goals = std::mem::take(&mut self.mob.goal_selector);
        goals.remove_all_goals(self, level, |_| true);
        self.mob.goal_selector = goals;
        self.register_goals();
        Brain::stop_all(level, self);
        self.mob.living.brain.clear_memories();
    }

    /// `babyGhastSetup()` (`level` is the level that stops the goals of an adult that is just turning into a ghastling).
    fn baby_ghast_setup(&mut self, level: Option<&mut ServerLevel>) {
        self.mob.move_control = Some(Box::new(FlyingMoveControl::new(180, true)));
        self.mob.look_control = Some(Box::<StandardLookControl>::default());
        let mut navigation = BabyFlyingPathNavigation {
            data: NavigationData::new(&*self, NavClass::Flying),
        };
        navigation.set_can_open_doors(false);
        navigation.set_can_float(true);
        navigation.set_required_path_length(&*self, 48.0);
        self.mob.set_navigation(Box::new(navigation));
        let mut goals = std::mem::take(&mut self.mob.goal_selector);
        match level {
            Some(level) => {
                self.set_server_still_timeout(Some(&mut *level), 0);
                goals.remove_all_goals(self, level, |_| true);
            }
            None => {
                self.set_server_still_timeout(None, 0);
                goals.remove_all_goals_not_running(|_| true);
            }
        }
        self.mob.goal_selector = goals;
    }

    /// `setServerStillTimeout(serverStillTimeout)`: the first tick of standing still sends the exact position to the trackers.
    fn set_server_still_timeout(&mut self, level: Option<&mut ServerLevel>, timeout: i32) {
        if self.server_still_timeout <= 0
            && timeout > 0
            && let Some(level) = level
        {
            let position = self.base().position();
            self.base_mut().packet_position_codec.set_base(position);
            let b = self.base();
            let packet = ClientboundEntityPositionSyncPacket {
                id: self.id(),
                position: PositionPath::Linear(self.tracking_position()),
                y_rot: b.y_rot(),
                x_rot: b.x_rot(),
                on_ground: b.on_ground(),
            };
            level.send_to_tracking_players(self.id(), &packet);
        }
        self.server_still_timeout = timeout;
        // `syncStayStillFlag()`
        let stays_still = timeout > 0;
        self.base_mut().entity_data.set(STAYS_STILL, stays_still);
    }

    /// The level-dependent part of `addPassenger(passenger)`; `first`: the ghast was no vehicle before.
    fn passenger_added(&mut self, level: &mut ServerLevel, first: bool) {
        if first {
            self.play_sound(
                level,
                SoundEvent::ENTITY_HAPPY_GHAST_HARNESS_GOGGLES_DOWN,
                1.0,
                1.0,
            );
        }
        if !self.scan_player_above_ghast(level) {
            self.set_server_still_timeout(Some(level), 0);
        } else if self.server_still_timeout > MAX_STILL_TIMEOUT {
            self.set_server_still_timeout(Some(level), MAX_STILL_TIMEOUT);
        }
    }

    /// `staysStill()`.
    pub fn stays_still(&self) -> bool {
        self.base().entity_data.get(STAYS_STILL)
    }

    /// `isLeashHolder()`.
    pub fn is_leash_holder(&self) -> bool {
        self.base().entity_data.get(IS_LEASH_HOLDER)
    }

    /// `isOnStillTimeout()`.
    pub fn is_on_still_timeout(&self) -> bool {
        self.stays_still() || self.server_still_timeout > 0
    }

    /// `isWearingBodyArmor()`: a valid harness in the body slot.
    pub fn is_wearing_body_armor(&self) -> bool {
        let stack = self.get_item_by_slot(EquipmentSlot::Body);
        !stack.is_empty() && self.is_equippable_in_slot(&stack, EquipmentSlot::Body)
    }

    /// `scanPlayerAboveGhast()`: a player that does not ride a happy ghast stands on top of this one.
    fn scan_player_above_ghast(&self, level: &ServerLevel) -> bool {
        let bb = self.base().bounding_box();
        let detection_box = AABB::new(
            bb.min_x - 1.0,
            bb.max_y - f64::from(1.0E-5f32),
            bb.min_z - 1.0,
            bb.max_x + 1.0,
            bb.max_y + bb.get_ysize() / 2.0,
            bb.max_z + 1.0,
        );
        for &player_id in level.players() {
            let Some(player) = level.get_entity(player_id) else {
                continue;
            };
            if player.is_spectator() {
                continue;
            }
            let root_id = player.get_root_vehicle(level);
            if root_id == self.id() {
                continue;
            }
            if let Some(root) = level.get_entity(root_id)
                && !root.as_any().is::<HappyGhast>()
                && detection_box.contains_vec3(root.position())
            {
                return true;
            }
        }
        false
    }

    /// `getHappyGhastRestrictionRadius()`.
    fn restriction_radius(&self) -> i32 {
        if !self.is_baby() && self.get_item_by_slot(EquipmentSlot::Body).is_empty() {
            LARGE_RESTRICTION_RADIUS
        } else {
            SMALL_RESTRICTION_RADIUS
        }
    }

    /// `checkRestriction()`.
    fn check_restriction(&mut self) {
        if !leashable::is_leashed(self) && !self.base().is_vehicle() {
            let radius = self.restriction_radius();
            let pos = self.block_position();
            let home = &mut self.mob;
            if !home.has_home()
                || !home
                    .home_position
                    .closer_than(&pos, f64::from(radius + RESTRICTION_RADIUS_BUFFER))
                || radius != home.home_radius
            {
                home.set_home_to(pos, radius);
            }
        }
    }

    /// `continuousHeal()`.
    fn continuous_heal(&mut self, level: &mut ServerLevel) {
        let living = &self.mob.living;
        if self.is_alive() && living.death_time == 0 && living.get_max_health() != self.get_health()
        {
            let pos = self.block_position();
            let is_fast_healing =
                is_in_clouds(self, level) || level.precipitation_at(pos) != Precipitation::None;
            let interval = if is_fast_healing {
                FAST_HEALING_TICKS
            } else {
                SLOW_HEALING_TICKS
            };
            if self.base().tick_count % interval == 0 {
                self.heal(level, 1.0);
            }
        }
    }

    /// `doPlayerRide(player)`.
    fn do_player_ride(&mut self, level: &mut ServerLevel, player: &mut ServerPlayer) {
        level.start_riding_held(player, self, false, true);
    }
}

/// `Entity.isInClouds()`: inside the cloud layer of the dimension's cloud attributes.
fn is_in_clouds(entity: &dyn Entity, level: &ServerLevel) -> bool {
    use mc_game_core::world::attribute::attributes::{CLOUD_COLOR, CLOUD_HEIGHT};
    use mc_game_core::world::attribute::value::AttrValue;
    let position = entity.position();
    let AttrValue::Argb(color) = level.environment_value(CLOUD_COLOR, &position) else {
        return false;
    };
    if color[3] == 0.0 {
        return false;
    }
    let cloud_bottom = level.environment_value(CLOUD_HEIGHT, &position).as_float();
    if (entity.base().y() + f64::from(entity.base().get_bb_height())) < f64::from(cloud_bottom) {
        return false;
    }
    let cloud_top = cloud_bottom + 4.0;
    entity.base().y() <= f64::from(cloud_top)
}

/// `HappyGhast.HappyGhastFloatGoal`: a `FloatGoal` that does not run while the ghast stands still.
struct HappyGhastFloatGoal {
    inner: FloatGoal,
}

impl Goal for HappyGhastFloatGoal {
    fn name(&self) -> &'static str {
        "HappyGhast$HappyGhastFloatGoal"
    }

    fn can_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        !is_stopped(mob) && self.inner.can_use(mob, level)
    }

    fn requires_update_every_tick(&self) -> bool {
        self.inner.requires_update_every_tick()
    }

    fn tick(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        self.inner.tick(mob, level);
    }

    fn flags(&self) -> GoalFlags {
        self.inner.flags()
    }
}

/// `HappyGhast.HappyGhastLookControl`.
#[derive(Default)]
struct HappyGhastLookControl {
    base: LookControlBase,
}

impl LookControl for HappyGhastLookControl {
    fn base(&self) -> &LookControlBase {
        &self.base
    }

    fn base_mut(&mut self) -> &mut LookControlBase {
        &mut self.base
    }

    fn tick(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        if is_stopped(mob) {
            let y_rot = mob.base().y_rot();
            let close_angle = mth::wrap_degrees90(y_rot);
            mob.set_y_rot(y_rot - close_angle);
            let y_rot = mob.base().y_rot();
            mob.set_y_head_rot(y_rot);
        } else if self.base.look_at_cooldown > 0 {
            self.base.look_at_cooldown -= 1;
            let xdd = self.base.wanted_x - mob.base().x();
            let zdd = self.base.wanted_z - mob.base().z();
            let pi = std::f32::consts::PI;
            mob.set_y_rot(-(mth::atan2(xdd, zdd) as f32) * (180.0 / pi));
            let y_rot = mob.base().y_rot();
            let living = mob.living_mut().expect("living");
            living.y_body_rot = y_rot;
            living.y_head_rot = living.y_body_rot;
        } else {
            face_movement_direction(mob, level);
        }
    }
}

/// `HappyGhast.HappyGhastBodyRotationControl`.
#[derive(Default)]
struct HappyGhastBodyRotationControl {
    base: BodyRotationControlBase,
}

impl BodyRotationControl for HappyGhastBodyRotationControl {
    fn base(&self) -> &BodyRotationControlBase {
        &self.base
    }

    fn base_mut(&mut self) -> &mut BodyRotationControlBase {
        &mut self.base
    }

    fn client_tick(&mut self, mob: &mut dyn Entity) {
        if mob.base().is_vehicle() {
            let y_rot = mob.base().y_rot();
            let living = mob.living_mut().expect("living");
            living.y_head_rot = y_rot;
            living.y_body_rot = living.y_head_rot;
        }
        base_client_tick(&mut self.base, mob);
    }
}

/// `HappyGhast.BabyFlyingPathNavigation`.
struct BabyFlyingPathNavigation {
    data: NavigationData,
}

impl PathNavigation for BabyFlyingPathNavigation {
    fn data(&self) -> &NavigationData {
        &self.data
    }

    fn data_mut(&mut self) -> &mut NavigationData {
        &mut self.data
    }

    fn can_move_directly(
        &self,
        mob: &dyn Entity,
        level: &ServerLevel,
        start_pos: Vec3,
        stop_pos: Vec3,
    ) -> bool {
        is_clear_for_movement_between(mob, level, start_pos, stop_pos, false)
    }
}

impl Entity for HappyGhast {
    mc_game_core::mob_entity_accessors!(mob);

    fn sanitize_scale(&self, scale: f32) -> f32 {
        java::min_f32(scale, MAX_SCALE)
    }

    fn get_encode_id(&self) -> Option<&'static str> {
        Some("minecraft:happy_ghast")
    }

    fn make_brain(&mut self, packed: &BrainPacked) -> Brain {
        BRAIN_PROVIDER.make_brain(self, packed)
    }

    fn age_boundary_reached(&mut self, level: &mut ServerLevel) {
        if self.is_baby() {
            self.baby_ghast_setup(Some(&mut *level));
        } else {
            self.adult_ghast_setup(level);
        }
        ageable::age_boundary_reached(self, level);
    }

    fn check_fall_damage(
        &mut self,
        _level: &mut ServerLevel,
        _ya: f64,
        _on_ground: bool,
        _on_state: mc_data::block::BlockState,
        _pos: BlockPos,
    ) {
    }

    fn on_climbable(&self, _level: &ServerLevel) -> bool {
        false
    }

    fn travel(&mut self, level: &mut ServerLevel, input: Vec3) {
        let speed = (mob::attribute_value(self, Attribute::FLYING_SPEED) as f32) * 5.0 / 3.0;
        travel_flying_speeds(self, level, input, speed, speed, speed);
    }

    fn get_walk_target_value(&self, pos: BlockPos, level: &dyn LevelReader) -> f32 {
        if !level.is_empty_block(pos) {
            0.0
        } else if level.is_empty_block(pos.below()) && !level.is_empty_block(pos.below_n(2)) {
            10.0
        } else {
            5.0
        }
    }

    fn can_breathe_underwater(&self) -> bool {
        self.is_baby()
            || mc_data::tags::get::<EntityType>("can_breathe_under_water")
                .is_some_and(|t| t.contains(self.get_type()))
    }

    fn should_stay_close_to_leash_holder(&self) -> bool {
        false
    }

    fn play_step_sound(
        &mut self,
        _level: &mut ServerLevel,
        _pos: BlockPos,
        _state: mc_data::block::BlockState,
    ) {
    }

    fn get_voice_pitch(&mut self) -> f32 {
        1.0
    }

    fn get_sound_source(&self) -> SoundSource {
        SoundSource::Neutral
    }

    fn get_ambient_sound_interval(&self) -> i32 {
        // `Animal.getAmbientSoundInterval()`
        let interval = 120;
        if self.base().is_vehicle() {
            interval * 6
        } else {
            interval
        }
    }

    fn get_ambient_sound(&self) -> Option<SoundEvent> {
        Some(if self.is_baby() {
            SoundEvent::ENTITY_GHASTLING_AMBIENT
        } else {
            SoundEvent::ENTITY_HAPPY_GHAST_AMBIENT
        })
    }

    fn get_hurt_sound(&self, _source: &DamageSource) -> Option<SoundEvent> {
        Some(if self.is_baby() {
            SoundEvent::ENTITY_GHASTLING_HURT
        } else {
            SoundEvent::ENTITY_HAPPY_GHAST_HURT
        })
    }

    fn get_death_sound(&self) -> Option<SoundEvent> {
        Some(if self.is_baby() {
            SoundEvent::ENTITY_GHASTLING_DEATH
        } else {
            SoundEvent::ENTITY_HAPPY_GHAST_DEATH
        })
    }

    fn get_sound_volume(&self) -> f32 {
        if self.is_baby() { 1.0 } else { 4.0 }
    }

    fn get_max_spawn_cluster_size(&self) -> i32 {
        1
    }

    fn get_default_dimensions(&self, _pose: Pose) -> EntityDimensions {
        if self.is_baby() {
            baby_dimensions()
        } else {
            EntityDimensions::of_type(self.base().entity_type)
        }
    }

    fn get_breed_offspring(
        &mut self,
        level: &mut ServerLevel,
        _partner: EntityId,
    ) -> Option<Box<dyn Entity>> {
        factory::create(
            EntityType::HAPPY_GHAST,
            level,
            EntitySpawnRequest {
                reason: EntitySpawnReason::Breeding,
                ignore_checks: false,
            },
        )
    }

    fn can_fall_in_love(&self) -> bool {
        false
    }

    fn get_age_scale(&self) -> f32 {
        if self.is_baby() { BABY_SCALE } else { 1.0 }
    }

    fn is_food(&self, stack: &ItemStack) -> bool {
        mc_data::tags::get::<Item>("happy_ghast_food").is_some_and(|t| t.contains(stack.item()))
    }

    fn can_use_slot(&self, slot: EquipmentSlot) -> bool {
        slot != EquipmentSlot::Body || (self.is_alive() && !self.is_baby())
    }

    fn can_dispenser_equip_into_slot(&self, slot: EquipmentSlot) -> bool {
        slot == EquipmentSlot::Body
    }

    fn mob_interact(
        &mut self,
        level: &mut ServerLevel,
        player: &mut ServerPlayer,
        hand: InteractionHand,
    ) -> InteractionResult {
        if self.is_baby() {
            return mob::interact::mob_interact(self, level, player, hand);
        }
        if !player.get_item_in_hand(hand).is_empty() {
            let result = mob::interact::with_hand_stack(player, hand, |stack, player| {
                mc_game_core::world::item::interact_living_entity(level, stack, player, self, hand)
            });
            if result.consumes_action() {
                return result;
            }
        }
        if self.is_wearing_body_armor() && !player.is_shift_key_down() {
            self.do_player_ride(level, player);
            InteractionResult::SUCCESS
        } else {
            mob::interact::mob_interact(self, level, player, hand)
        }
    }

    fn add_passenger(&mut self, passenger: &dyn Entity, first_passenger_is_player: bool) {
        let id = passenger.id();
        let is_player = passenger.is_player();
        let passengers = &mut self.base_mut().passengers;
        if passengers.is_empty() || (is_player && !first_passenger_is_player) {
            passengers.insert(0, id);
            self.first_passenger_is_player = is_player;
        } else {
            passengers.push(id);
        }
    }

    fn on_passenger_added(&mut self, level: &mut ServerLevel) {
        // the harness goggles come down for the first rider (`!isVehicle()` before the passenger joins)
        let first = self.base().passengers.len() == 1;
        self.passenger_added(level, first);
    }

    fn on_passenger_removed(&mut self, level: &mut ServerLevel, _passenger: EntityId) {
        match self.base().passengers.first() {
            None => self.first_passenger_is_player = false,
            // a first passenger the level cannot hand out keeps the recorded flag
            Some(&id) => {
                if let Some(first) = level.get_entity_or_lent(id) {
                    self.first_passenger_is_player = first.is_player();
                }
            }
        }
        self.set_server_still_timeout(Some(&mut *level), MAX_STILL_TIMEOUT);
        if !self.base().is_vehicle() {
            self.mob.clear_home();
            self.play_sound(
                level,
                SoundEvent::ENTITY_HAPPY_GHAST_HARNESS_GOGGLES_UP,
                1.0,
                1.0,
            );
        }
    }

    fn can_add_passenger(&self, _passenger: &dyn Entity) -> bool {
        self.base().passengers.len() < MAX_PASSENGERS
    }

    /// `getDismountLocationForPassenger(passenger)`: always on top of the ghast, `Animal`'s search is not run.
    fn get_dismount_location_for_passenger_setting_pose(
        &self,
        level: &ServerLevel,
        passenger: &mut dyn Entity,
    ) -> Vec3 {
        self.get_dismount_location_for_passenger(level, &*passenger)
    }

    fn get_controlling_passenger(&self) -> Option<EntityId> {
        if self.is_wearing_body_armor()
            && !self.is_on_still_timeout()
            && self.first_passenger_is_player
        {
            self.base().passengers.first().copied()
        } else {
            mob::controlling_passenger(self)
        }
    }

    fn get_ridden_input(
        &self,
        level: &ServerLevel,
        controller: EntityId,
        _self_input: Vec3,
    ) -> Vec3 {
        let Some(c) = level.get_entity(controller) else {
            return Vec3::ZERO;
        };
        let Some(living) = c.living() else {
            return Vec3::ZERO;
        };
        let strafe = living.xxa;
        let mut forward = 0.0f32;
        let mut up = 0.0f32;
        if living.zza != 0.0 {
            let x_rot = c.base().x_rot() * (std::f64::consts::PI / 180.0) as f32;
            let mut forward_look = mth::cos(f64::from(x_rot));
            let mut up_look = -mth::sin(f64::from(x_rot));
            if living.zza < 0.0 {
                forward_look *= -0.5;
                up_look *= -0.5;
            }
            up = up_look;
            forward = forward_look;
        }
        if living.jumping {
            up += 0.5;
        }
        Vec3::new(f64::from(strafe), f64::from(up), f64::from(forward))
            .scale(f64::from(3.9f32) * mob::attribute_value(self, Attribute::FLYING_SPEED))
    }

    fn tick_ridden(&mut self, level: &mut ServerLevel, controller: EntityId, _ridden_input: Vec3) {
        let Some((x_rot, y_rot)) = level
            .get_entity(controller)
            .map(|c| (c.base().x_rot(), c.base().y_rot()))
        else {
            return;
        };
        // `getRiddenRotation(controller)`: `Vec2(controller.getXRot() * 0.5F, controller.getYRot())`
        let (rot_x, rot_y) = (x_rot * 0.5, y_rot);
        let mut y = self.base().y_rot();
        let diff = mth::wrap_degrees_f32(rot_y - y);
        y += diff * 0.08;
        self.set_rot(y, rot_x);
        self.base_mut().y_rot_o = y;
        let living = &mut self.mob.living;
        living.y_body_rot = y;
        living.y_head_rot = y;
    }

    fn custom_server_ai_step(&mut self, level: &mut ServerLevel) {
        if self.is_baby() {
            Brain::tick(level, self);
            happy_ghast_ai::update_activity(self);
        }
        self.check_restriction();
        mob::animal::custom_server_ai_step(self);
    }

    fn tick(&mut self, level: &mut ServerLevel) {
        mob::tick::tick(self, level);
        if self.leash_holder_time > 0 {
            self.leash_holder_time -= 1;
        }
        let is_leash_holder = self.leash_holder_time > 0;
        self.base_mut()
            .entity_data
            .set(IS_LEASH_HOLDER, is_leash_holder);
        if self.server_still_timeout > 0 {
            if self.base().tick_count > STILL_TIMEOUT_ON_LOAD_GRACE_PERIOD {
                self.server_still_timeout -= 1;
            }
            let timeout = self.server_still_timeout;
            self.set_server_still_timeout(Some(&mut *level), timeout);
        }
        if self.scan_player_above_ghast(level) {
            self.set_server_still_timeout(Some(level), MAX_STILL_TIMEOUT);
        }
    }

    fn ai_step(&mut self, level: &mut ServerLevel) {
        let still = self.is_on_still_timeout();
        self.set_requires_precise_position(still);
        mob::tick::ai_step(self, level);
        self.continuous_heal(level);
    }

    fn can_be_collided_with(&self, other: Option<&dyn Entity>) -> bool {
        if !self.is_baby() && self.is_alive() {
            if self.base().is_vehicle() && other.is_some_and(|o| o.as_any().is::<HappyGhast>()) {
                true
            } else {
                self.is_on_still_timeout()
            }
        } else {
            false
        }
    }

    fn is_flying_vehicle(&self) -> bool {
        !self.is_baby()
    }

    fn support_quad_leash_as_holder(&self) -> bool {
        true
    }

    fn get_leash_offset(&self) -> Vec3 {
        Vec3::ZERO
    }

    fn leash_elastic_distance(&self) -> f64 {
        10.0
    }

    fn leash_snap_distance(&self) -> f64 {
        16.0
    }

    fn on_elastic_leash_pull(&mut self, _level: &mut ServerLevel) {
        self.check_fall_distance_accumulation();
        self.mob.move_control_mut().set_wait();
    }

    fn notify_leash_holder(&mut self, leashee: &dyn Entity) {
        if leashee.support_quad_leash() {
            self.leash_holder_time = 5;
        }
    }

    fn add_additional_save_data(&self, out: &mut CompoundTag) {
        mob::nbt::add_additional_save_data(self, out);
        out.put_int("still_timeout", self.server_still_timeout);
    }

    fn read_additional_save_data(&mut self, level: Option<&ServerLevel>, input: &CompoundTag) {
        let was_baby = self.is_baby();
        mob::nbt::read_additional_save_data(self, level, input);
        let age_boundary_crossed = was_baby != self.is_baby();
        let still_timeout = input.get_int_or("still_timeout", 0);
        if level.is_some() {
            // the goals, brain and trackers need the level mutably
            self.pending_load = Some(PendingLoad {
                age_boundary_crossed,
                still_timeout,
            });
            return;
        }
        // `setAge` crossing into the baby range runs `ageBoundaryReached()`
        if age_boundary_crossed && self.is_baby() {
            self.baby_ghast_setup(None);
        }
        self.set_server_still_timeout(None, still_timeout);
    }

    fn read_level_dependent_data(&mut self, level: &mut ServerLevel) {
        if let Some(pending) = self.pending_load.take() {
            if pending.age_boundary_crossed {
                // `refreshDimensions()` of `setAge` already moved the entity out of the blocks its new size overlaps
                movement::apply_pending_fudge(self, level);
                self.age_boundary_reached(level);
            }
            self.set_server_still_timeout(Some(&mut *level), pending.still_timeout);
        }
        // `addPassenger` of the passengers `loadPassengersRecursive` attached
        if std::mem::take(&mut self.mob.passengers_loaded) {
            self.passenger_added(level, true);
        }
    }
}

/// The scenario dump fields of a happy ghast: `still stillt lholder lhtime`.
#[distributed_slice(SCENARIO_DUMP_EXTRA)]
static HAPPY_GHAST_DUMP: ScenarioDumpExtra = ScenarioDumpExtra {
    order: 30,
    applies: |e| e.as_any().is::<HappyGhast>(),
    fields: |e, _level, _ctx| {
        let ghast = e.as_any().downcast_ref::<HappyGhast>().expect("applies");
        vec![
            ("still".into(), u8::from(ghast.stays_still()).to_string()),
            ("stillt".into(), ghast.server_still_timeout.to_string()),
            (
                "lholder".into(),
                u8::from(ghast.is_leash_holder()).to_string(),
            ),
            ("lhtime".into(), ghast.leash_holder_time.to_string()),
        ]
    },
};

#[distributed_slice(ENTITY_FACTORIES)]
static HAPPY_GHAST: EntityRegistration = EntityRegistration {
    entity_type: EntityType::HAPPY_GHAST,
    create: |ty, id, registries| Box::new(HappyGhast::new(ty, id, registries)),
};
