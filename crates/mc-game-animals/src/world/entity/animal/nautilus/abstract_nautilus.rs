//! `net.minecraft.world.entity.animal.nautilus.AbstractNautilus`: a tameable swimming mount with a saddle and body armor,
//! a dash charged by the rider, the breath it lends to its rider and a brain-driven attack on its own.
//!
//! Rust shape of the class hierarchy (the same as `AbstractHorse`): [`AbstractNautilusData`] holds the fields, the
//! [`AbstractNautilus`] trait declares the virtual members `Nautilus` and `ZombieNautilus` differ in, the free functions are
//! the non-virtual bodies, and [`abstract_nautilus_entity!`] forwards the shared `Entity` members to them.
//!
//! Not implemented: the bubble particles of `spawnBubbles` (client-only; the random draws are kept) and the client-side
//! `onPlayerJump` caller (a dedicated server never receives the jump amount, only the start and stop packets).

use super::nautilus_ai;
use linkme::distributed_slice;
use mc_core::math::java::java_float_to_string;
use mc_core::math::mth;
use mc_core::phys::Vec3;
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_data::block::Block;
use mc_data::enums::EntitySpawnReason;
use mc_data::{Attribute, EntityType, GameEvent, Item, MobEffect, SoundEvent};
use mc_game_core::define_accessor;
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::container::SimpleContainer;
use mc_game_core::world::damagesource::DamageSource;
use mc_game_core::world::difficulty_instance::DifficultyInstance;
use mc_game_core::world::entity::ai::memory::memory_module_type::{
    ANGRY_AT, ATTACK_TARGET, CANT_REACH_WALK_TARGET_SINCE,
};
use mc_game_core::world::entity::ai::sensing::sensor;
use mc_game_core::world::entity::effect::MobEffectInstance;
use mc_game_core::world::entity::entity::{DATA_POSE, calculate_view_vector};
use mc_game_core::world::entity::equipment::EquipmentSlot;
use mc_game_core::world::entity::living_entity::util::{self, l, lm};
use mc_game_core::world::entity::mob::{self, DATA_BABY_ID, MobData, tamable};
use mc_game_core::world::entity::player::menu_ctx::PlayerCtx;
use mc_game_core::world::entity::scenario_dump::{SCENARIO_DUMP_EXTRA, ScenarioDumpExtra};
use mc_game_core::world::entity::spawn_group_data::SpawnGroupData;
use mc_game_core::world::entity::{Entity, EntityId, MoverType, ServerPlayer};
use mc_game_core::world::item::{
    InteractionHand, InteractionResult, ItemStack, create_filled_result,
};
use mc_game_core::world::level::material::FluidStateExt;
use mc_game_core::world::level::server_level_accessor::ServerLevelAccessor;

define_accessor!(pub DASH: Boolean = AbstractNautilus);

/// `EFFECT_REFRESH_RATE`.
const EFFECT_REFRESH_RATE: i64 = 40;
/// `RIDDEN_SPEED_MODIFIER_IN_WATER`.
const RIDDEN_SPEED_MODIFIER_IN_WATER: f32 = 0.0325;
/// `RIDDEN_SPEED_MODIFIER_ON_LAND`.
const RIDDEN_SPEED_MODIFIER_ON_LAND: f32 = 0.02;
/// `DASH_COOLDOWN_TICKS`.
const DASH_COOLDOWN_TICKS: i32 = 40;
/// `DASH_MOMENTUM_IN_WATER`.
const DASH_MOMENTUM_IN_WATER: f32 = 1.2;
/// `DASH_MOMENTUM_ON_LAND`.
const DASH_MOMENTUM_ON_LAND: f32 = 0.5;
/// `BUBBLE_SPREAD_FACTOR`.
const BUBBLE_SPREAD_FACTOR: f64 = 0.8;
/// `BUBBLE_PROBABILITY_MULTIPLIER`.
const BUBBLE_PROBABILITY_MULTIPLIER: f64 = 2.0;
/// `BUBBLE_PROBABILITY_MIN`.
const BUBBLE_PROBABILITY_MIN: f32 = 0.15;
/// `BUBBLE_PROBABILITY_MAX`.
const BUBBLE_PROBABILITY_MAX: f32 = 1.0;
/// `NAUTILUS_TOTAL_AIR_SUPPLY`.
pub const TOTAL_AIR_SUPPLY: i32 = 300;
/// The `ANGER_DURATION` of `NautilusAi`.
const ANGER_DURATION: i64 = 400;

/// The fields of `AbstractNautilus`.
pub struct AbstractNautilusData {
    /// `dashCooldown`.
    pub dash_cooldown: i32,
    /// `playerJumpPendingScale`.
    pub player_jump_pending_scale: f32,
    /// `inventory` (`getInventoryColumns()` is 0 for both classes: no chest slots, the saddle and armor are equipment).
    pub inventory: SimpleContainer,
    /// `getFirstPassenger() instanceof Player` (kept by `add_passenger` / `on_passenger_removed`).
    pub first_passenger_is_player: bool,
}

impl Default for AbstractNautilusData {
    fn default() -> Self {
        Self::new()
    }
}

impl AbstractNautilusData {
    /// The field initializers of `AbstractNautilus`.
    pub fn new() -> Self {
        Self {
            dash_cooldown: 0,
            player_jump_pending_scale: 0.0,
            inventory: SimpleContainer::new(0),
            first_passenger_is_player: false,
        }
    }
}

/// `AbstractNautilus` (the virtual members; see the module docs).
pub trait AbstractNautilus: Entity {
    fn nautilus(&self) -> &AbstractNautilusData;
    fn nautilus_mut(&mut self) -> &mut AbstractNautilusData;
    /// `getDashSound()`.
    fn get_dash_sound(&self) -> SoundEvent;
    /// `getDashReadySound()`.
    fn get_dash_ready_sound(&self) -> SoundEvent;
}

/// `new AbstractNautilus(type, level)` for the members of `MobData` (the controls, the water malus and the navigation).
pub fn init_mob(e: &mut (impl AbstractNautilus + Sized)) {
    use mc_game_core::world::entity::ai::control::smooth_swimming_look_control::SmoothSwimmingLookControl;
    use mc_game_core::world::entity::ai::control::smooth_swimming_move_control::SmoothSwimmingMoveControl;
    use mc_game_core::world::entity::ai::navigation::StandardPathNavigation;
    use mc_game_core::world::level::pathfinder::PathType;
    let navigation = StandardPathNavigation::water_bound(&*e);
    let mob = e.mob_mut().expect("mob");
    mob.set_navigation(Box::new(navigation));
    mob.move_control = Some(Box::new(SmoothSwimmingMoveControl::new(
        85, 10, 0.011, 0.0, true,
    )));
    mob.look_control = Some(Box::new(SmoothSwimmingLookControl::new(10)));
    mob.set_pathfinding_malus(PathType::Water, 0.0);
}

/// `MobData` of an `AbstractNautilus`: a tameable animal with the dash flag.
pub fn mob_data(
    ty: EntityType,
    id: EntityId,
    define: impl FnOnce(&mut mc_game_core::world::entity::synched_data::SynchedEntityDataBuilder),
) -> MobData {
    MobData::new(ty, id, mob::MobKind::TamableAnimal, |b| {
        b.define(DASH, false);
        define(b);
    })
}

/// `isDashing()`.
pub fn is_dashing(e: &(impl AbstractNautilus + ?Sized)) -> bool {
    e.base().entity_data.get(DASH)
}

/// `setDashing(isDashing)`: a changed value runs `onSyncedDataUpdated`.
pub fn set_dashing(e: &mut (impl AbstractNautilus + ?Sized), dashing: bool) {
    if e.base_mut().entity_data.set(DASH, dashing) {
        e.on_synced_data_updated(DASH.id());
    }
}

/// `isSaddled()`.
pub fn is_saddled(e: &(impl AbstractNautilus + ?Sized)) -> bool {
    !e.get_item_by_slot(EquipmentSlot::Saddle).is_empty()
}

/// `isAggravated()`.
pub fn is_aggravated(e: &dyn Entity) -> bool {
    e.brain().has_memory_value(ANGRY_AT) || e.brain().has_memory_value(ATTACK_TARGET)
}

/// `isMobControlled()`.
pub fn is_mob_controlled(e: &dyn Entity) -> bool {
    e.mob().is_some_and(|m| m.first_passenger_is_mob)
}

/// `onSyncedDataUpdated(accessor)` (the dash flag restarts the cooldown on a client that sees the dash start).
pub fn on_synced_data_updated(e: &mut (impl AbstractNautilus + ?Sized), accessor_id: u8) {
    if !e.base().first_tick && accessor_id == DASH.id() {
        let n = e.nautilus_mut();
        n.dash_cooldown = if n.dash_cooldown == 0 {
            DASH_COOLDOWN_TICKS
        } else {
            n.dash_cooldown
        };
    }
    if accessor_id == DATA_POSE.id() {
        e.refresh_dimensions();
    }
    if accessor_id == DATA_BABY_ID.id() {
        e.refresh_dimensions();
    }
}

/// `canUseSlot(slot)`.
pub fn can_use_slot(e: &(impl AbstractNautilus + ?Sized), slot: EquipmentSlot) -> bool {
    if slot != EquipmentSlot::Saddle && slot != EquipmentSlot::Body {
        true
    } else {
        e.is_alive() && !e.is_baby() && tamable::is_tame(e.as_dyn())
    }
}

/// `getControllingPassenger()`.
pub fn get_controlling_passenger(e: &(impl AbstractNautilus + ?Sized)) -> Option<EntityId> {
    if is_saddled(e) && e.nautilus().first_passenger_is_player {
        e.base().passengers.first().copied()
    } else {
        mob::controlling_passenger(e.as_dyn())
    }
}

/// `addPassenger(passenger)` bookkeeping on top of the passenger list.
pub fn add_passenger(
    e: &mut (impl AbstractNautilus + ?Sized),
    passenger: &dyn Entity,
    first_passenger_is_player: bool,
) {
    let id = passenger.id();
    let is_player = passenger.is_player();
    let passengers = &mut e.base_mut().passengers;
    let becomes_first = passengers.is_empty() || (is_player && !first_passenger_is_player);
    if becomes_first {
        passengers.insert(0, id);
        e.nautilus_mut().first_passenger_is_player = is_player;
    } else {
        passengers.push(id);
    }
}

/// `removePassenger` bookkeeping: the class of the new first passenger.
pub fn on_passenger_removed(e: &mut (impl AbstractNautilus + ?Sized), level: &ServerLevel) {
    let is_player = e
        .base()
        .passengers
        .first()
        .and_then(|&id| level.get_entity(id))
        .is_some_and(|p| p.is_player());
    e.nautilus_mut().first_passenger_is_player = is_player;
}

/// `getRiddenInput(controller, selfInput)`: strafe from the sideways input, forward and up along the pitch of the rider.
pub fn get_ridden_input(level: &ServerLevel, controller: EntityId) -> Vec3 {
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
    Vec3::new(f64::from(strafe), f64::from(up), f64::from(forward))
}

/// `tickRidden(controller, riddenInput)`: the nautilus turns half way to the head of the rider, pitching at half of it.
pub fn tick_ridden(
    e: &mut (impl AbstractNautilus + ?Sized),
    level: &mut ServerLevel,
    controller: EntityId,
) {
    let Some((x_rot, y_rot)) = level
        .get_entity(controller)
        .map(|c| (c.base().x_rot(), c.base().y_rot()))
    else {
        return;
    };
    // `getRiddenRotation(controller)`: `Vec2(controller.getXRot() * 0.5F, controller.getYRot())`
    let (rot_x, rot_y) = (x_rot * 0.5, y_rot);
    let mut y = e.base().y_rot();
    let diff = mth::wrap_degrees_f32(rot_y - y);
    y += diff * 0.5;
    e.set_rot(y, rot_x);
    e.base_mut().y_rot_o = y;
    lm(e.as_dyn_mut()).y_body_rot = y;
    e.set_y_head_rot(y);
    if e.is_local_instance_authoritative() {
        let scale = e.nautilus().player_jump_pending_scale;
        if scale > 0.0 && !l(e.as_dyn()).jumping {
            execute_riders_jump(e, level, scale, controller);
        }
        e.nautilus_mut().player_jump_pending_scale = 0.0;
    }
}

/// `executeRidersJump(amount, controller)`: the dash along the look of the rider.
pub fn execute_riders_jump(
    e: &mut (impl AbstractNautilus + ?Sized),
    level: &ServerLevel,
    amount: f32,
    controller: EntityId,
) {
    let Some(look) = level.get_entity(controller).map(|c| c.get_look_angle()) else {
        return;
    };
    let momentum = if e.is_in_water() {
        DASH_MOMENTUM_IN_WATER
    } else {
        DASH_MOMENTUM_ON_LAND
    };
    let speed = l(e.as_dyn()).get_attribute_value(Attribute::MOVEMENT_SPEED);
    let factor = e.get_block_speed_factor(level);
    let impulse = f64::from(momentum * amount) * speed * f64::from(factor);
    e.base_mut().add_delta_movement(look.scale(impulse));
    e.nautilus_mut().dash_cooldown = DASH_COOLDOWN_TICKS;
    set_dashing(e, true);
    e.base_mut().needs_sync = true;
}

/// `travelInWater(input, baseGravity, isFalling, oldY)`: no gravity or buoyancy, a drag of 0.9.
pub fn travel_in_water(
    e: &mut (impl AbstractNautilus + ?Sized),
    level: &mut ServerLevel,
    input: Vec3,
) {
    let speed = e.get_speed();
    util::move_relative(e.as_dyn_mut(), speed, input);
    let dm = e.base().delta_movement();
    util::do_move(e.as_dyn_mut(), level, MoverType::SelfMover, dm);
    let dm = e.base().delta_movement();
    e.base_mut().set_delta_movement(dm.scale(0.9));
}

/// `getRiddenSpeed(controller)`.
pub fn get_ridden_speed(e: &(impl AbstractNautilus + ?Sized)) -> f32 {
    let speed = l(e.as_dyn()).get_attribute_value(Attribute::MOVEMENT_SPEED) as f32;
    if e.is_in_water() {
        RIDDEN_SPEED_MODIFIER_IN_WATER * speed
    } else {
        RIDDEN_SPEED_MODIFIER_ON_LAND * speed
    }
}

/// `doPlayerRide(player)`.
pub fn do_player_ride(
    e: &mut (impl AbstractNautilus + ?Sized),
    level: &mut ServerLevel,
    player: &mut ServerPlayer,
) {
    level.start_riding_held(player, e.as_dyn_mut(), false, true);
    if !e.base().is_vehicle()
        && let Some(m) = e.mob_mut()
    {
        m.clear_home();
    }
}

/// `getNautilusRestrictionRadius()`.
fn restriction_radius(e: &(impl AbstractNautilus + ?Sized)) -> i32 {
    if !e.is_baby() && e.get_item_by_slot(EquipmentSlot::Saddle).is_empty() {
        32
    } else {
        16
    }
}

/// `checkRestriction()`: a tamed nautilus stays near where it was left.
pub fn check_restriction(e: &mut (impl AbstractNautilus + ?Sized)) {
    if !mob::leashable::is_leashed(e.as_dyn())
        && !e.base().is_vehicle()
        && tamable::is_tame(e.as_dyn())
    {
        let radius = restriction_radius(e);
        let pos = e.block_position();
        let Some(m) = e.mob_mut() else { return };
        let close = m.home_position.dist_sqr(&pos) < f64::from(radius + 8) * f64::from(radius + 8);
        if !m.has_home() || !close || radius != m.home_radius {
            m.set_home_to(pos, radius);
        }
    }
}

/// `applyEffects(level)`: the breath of the nautilus for a player on its back.
fn apply_effects(e: &(impl AbstractNautilus + ?Sized), level: &mut ServerLevel) {
    let Some(&first) = e.base().passengers.first() else {
        return;
    };
    let Some(player) = level.get_entity(first).filter(|p| p.is_player()) else {
        return;
    };
    let has_effect = player.has_effect(MobEffect::BREATH_OF_THE_NAUTILUS);
    let should_refresh = level.game_time() % EFFECT_REFRESH_RATE == 0;
    if !has_effect || should_refresh {
        level.with_entity(first, |level, p| {
            p.add_effect(
                level,
                MobEffectInstance::new(
                    MobEffect::BREATH_OF_THE_NAUTILUS,
                    60,
                    0,
                    true,
                    true,
                    true,
                    None,
                ),
                None,
            );
        });
    }
}

/// `spawnBubbles()`: the particle is client-only, the draws on the nautilus' random are not.
fn spawn_bubbles(e: &mut (impl AbstractNautilus + ?Sized)) {
    let speed = e.base().delta_movement().length();
    let probability = mth::clamp_f64(
        speed * BUBBLE_PROBABILITY_MULTIPLIER,
        f64::from(BUBBLE_PROBABILITY_MIN),
        f64::from(BUBBLE_PROBABILITY_MAX),
    );
    if f64::from(e.base_mut().random.next_float()) < probability {
        let x_rot = mth::clamp_f32(e.base().x_rot(), -10.0, 10.0);
        let _mouth_direction = calculate_view_vector(x_rot, e.base().y_rot());
        let random = &mut e.base_mut().random;
        let spread = random.next_double() * BUBBLE_SPREAD_FACTOR * (1.0 + speed);
        let _dx = (f64::from(random.next_float()) - 0.5) * spread;
        let _dy = (f64::from(random.next_float()) - 0.5) * spread;
        let _dz = (f64::from(random.next_float()) - 0.5) * spread;
    }
}

/// `tick()` after `Mob.tick()`.
pub fn tick(e: &mut (impl AbstractNautilus + ?Sized), level: &mut ServerLevel) {
    mob::tick::tick(e.as_dyn_mut(), level);
    apply_effects(e, level);
    if is_dashing(e) && e.nautilus().dash_cooldown < 35 {
        set_dashing(e, false);
    }
    if e.nautilus().dash_cooldown > 0 {
        e.nautilus_mut().dash_cooldown -= 1;
        if e.nautilus().dash_cooldown == 0 {
            let sound = e.get_dash_ready_sound();
            util_make_sound(e, level, sound);
        }
    }
    if e.is_in_water() {
        spawn_bubbles(e);
    }
}

fn util_make_sound(
    e: &mut (impl AbstractNautilus + ?Sized),
    level: &mut ServerLevel,
    sound: SoundEvent,
) {
    mc_game_core::world::entity::living_entity::damage::make_sound(e.as_dyn_mut(), level, sound);
}

/// `onPlayerJump(jumpAmount)`: only the client calls it; kept for the pending scale the dash reads.
pub fn on_player_jump(e: &mut (impl AbstractNautilus + ?Sized), jump_amount: i32) {
    if is_saddled(e) && e.nautilus().dash_cooldown <= 0 {
        e.nautilus_mut().player_jump_pending_scale =
            mc_game_core::world::entity::animal::equine::abstract_horse::get_player_jump_pending_scale(jump_amount);
    }
}

/// `handleStartJump(jumpScale)`: the rider released a charged jump, the dash starts.
pub fn handle_start_jump(e: &mut (impl AbstractNautilus + ?Sized), level: &mut ServerLevel) {
    let sound = e.get_dash_sound();
    util_make_sound(e, level, sound);
    e.game_event(level, GameEvent::ENTITY_ACTION);
    set_dashing(e, true);
}

/// `openCustomInventoryScreen(player)`; the screen reads the nautilus, which is out of its level while it runs, so it is
/// lent for the open.
pub fn open_custom_inventory_screen(
    e: &mut (impl AbstractNautilus + ?Sized),
    level: &mut ServerLevel,
    player: &mut ServerPlayer,
) {
    if (!e.base().is_vehicle() || e.base().passengers.contains(&player.id()))
        && tamable::is_tame(e.as_dyn())
    {
        let id = e.id();
        level.lend(e.as_dyn_mut(), |level| {
            player.open_nautilus_inventory(level, id)
        });
    }
}

/// `tryToTame(player)`.
fn try_to_tame(
    e: &mut (impl AbstractNautilus + ?Sized),
    level: &mut ServerLevel,
    player: &mut ServerPlayer,
) {
    if e.base_mut().random.next_int_bounded(3) == 0 {
        tamable::tame(e.as_dyn_mut(), level, player);
        mob::navigation::stop(e.as_dyn_mut(), level);
        level.broadcast_entity_event(e.id(), 7);
    } else {
        level.broadcast_entity_event(e.id(), 6);
    }
    e.play_eating_sound(level);
}

/// `interact(player, hand, location)`: `setPersistenceRequired()` first.
pub fn interact(
    e: &mut (impl AbstractNautilus + ?Sized),
    level: &mut ServerLevel,
    player: &mut ServerPlayer,
    hand: InteractionHand,
    location: Vec3,
) -> InteractionResult {
    e.set_persistence_required();
    mob::interact::interact(e.as_dyn_mut(), level, player, hand, location)
}

/// `mobInteract(player, hand)`.
pub fn mob_interact(
    e: &mut (impl AbstractNautilus + ?Sized),
    level: &mut ServerLevel,
    player: &mut ServerPlayer,
    hand: InteractionHand,
) -> InteractionResult {
    let item_stack = player.get_item_in_hand(hand);
    if e.is_baby() {
        return mob::interact::mob_interact(e.as_dyn_mut(), level, player, hand);
    }
    if tamable::is_tame(e.as_dyn()) && player.is_shift_key_down() {
        e.open_custom_inventory_screen(level, player);
        return InteractionResult::SUCCESS;
    }
    if !item_stack.is_empty() {
        if !tamable::is_tame(e.as_dyn()) && e.is_food(&item_stack) {
            e.use_player_item(level, player, hand);
            try_to_tame(e, level, player);
            return InteractionResult::SUCCESS_SERVER;
        }
        if e.is_food(&item_stack) && e.get_health() < l(e.as_dyn()).get_max_health() {
            tamable::feed(e.as_dyn_mut(), level, player, hand, &item_stack, 2.0, 1.0);
            return InteractionResult::SUCCESS;
        }
        let result = mob::interact::with_hand_stack(player, hand, |s, p| {
            mc_game_core::world::item::interact_living_entity(level, s, p, e.as_dyn_mut(), hand)
        });
        if result.consumes_action() {
            return result;
        }
    }
    if tamable::is_tame(e.as_dyn()) && !player.is_shift_key_down() && !e.is_food(&item_stack) {
        do_player_ride(e, level, player);
        InteractionResult::SUCCESS
    } else {
        mob::interact::mob_interact(e.as_dyn_mut(), level, player, hand)
    }
}

/// `hurtServer(level, source, damage)`: whoever hurt it is angered at.
pub fn hurt_server(
    e: &mut (impl AbstractNautilus + ?Sized),
    level: &mut ServerLevel,
    source: &DamageSource,
    damage: f32,
) -> bool {
    let was_hurt = mc_game_core::world::entity::living_entity::damage::hurt_server(
        e.as_dyn_mut(),
        level,
        source,
        damage,
    );
    if was_hurt
        && let Some(attacker) = source
            .get_entity()
            .and_then(|id| level.get_entity_or_recently_removed(id))
            .filter(|a| a.living().is_some())
    {
        set_anger_target(&*level, e.as_dyn_mut(), attacker);
    }
    was_hurt
}

/// `NautilusAi.setAngerTarget(level, body, target)`.
fn set_anger_target(level: &ServerLevel, body: &mut dyn Entity, target: &dyn Entity) {
    if sensor::is_entity_attackable_ignoring_line_of_sight(level, &*body, target) {
        body.brain_mut().erase_memory(&CANT_REACH_WALK_TARGET_SINCE);
        body.brain_mut()
            .set_memory_with_expiry(&ANGRY_AT, target.uuid(), ANGER_DURATION);
    }
}

/// `checkNautilusSpawnRules(type, level, spawnReason, pos, random)`.
pub fn check_nautilus_spawn_rules(
    _ty: EntityType,
    level: &dyn ServerLevelAccessor,
    _reason: EntitySpawnReason,
    pos: BlockPos,
    _random: &mut dyn RandomSource,
) -> bool {
    let sea_level = level.get_sea_level();
    let min_spawn_level = sea_level - 25;
    pos.y() >= min_spawn_level
        && pos.y() <= sea_level - 5
        && level.get_fluid_state(pos.below()).is_tag("water")
        && level.get_block_state(pos.above()).block() == Block::WATER
}

/// `isFood(itemStack)`: the taming items for a wild adult, the food otherwise.
pub fn is_food(e: &(impl AbstractNautilus + ?Sized), stack: &ItemStack) -> bool {
    let tag = if !tamable::is_tame(e.as_dyn()) && !e.is_baby() {
        "nautilus_taming_items"
    } else {
        "nautilus_food"
    };
    mc_data::tags::get::<Item>(tag).is_some_and(|t| t.contains(stack.item()))
}

/// `usePlayerItem(player, hand, itemStack)`: a bucket food leaves a water bucket behind.
pub fn use_player_item(level: &mut ServerLevel, player: &mut ServerPlayer, hand: InteractionHand) {
    let stack = player.get_item_in_hand(hand);
    if mc_data::tags::get::<Item>("nautilus_bucket_food").is_some_and(|t| t.contains(stack.item()))
    {
        let filled = create_filled_result(
            stack,
            &mut PlayerCtx { player, level },
            ItemStack::new(Item::WATER_BUCKET),
            true,
        );
        player.set_item_in_hand(hand, filled);
    } else {
        mob::interact::use_player_item(level, player, hand);
    }
}

/// `finalizeSpawn(level, difficulty, spawnReason, groupData)`: the attack cooldown is rolled before `Mob.finalizeSpawn`.
pub fn finalize_spawn(
    e: &mut (impl AbstractNautilus + ?Sized),
    level: &mut dyn ServerLevelAccessor,
    difficulty: DifficultyInstance,
    reason: EntitySpawnReason,
    group_data: Option<Box<dyn SpawnGroupData>>,
) -> Option<Box<dyn SpawnGroupData>> {
    nautilus_ai::init_memories(e.as_dyn_mut(), level.random());
    mob::spawn::finalize_spawn(e.as_dyn_mut(), level, difficulty, reason, group_data)
}

/// `canDispenserEquipIntoSlot(slot)`.
pub fn can_dispenser_equip_into_slot(
    e: &(impl AbstractNautilus + ?Sized),
    slot: EquipmentSlot,
) -> bool {
    slot == EquipmentSlot::Body
        || slot == EquipmentSlot::Saddle
        || mob::equipment::can_dispenser_equip_into_slot(e.as_dyn())
}

/// `getEquipSound(slot, stack, equippable)` for the saddle.
pub fn get_equip_sound_override(
    e: &(impl AbstractNautilus + ?Sized),
    slot: EquipmentSlot,
) -> Option<SoundEvent> {
    (slot == EquipmentSlot::Saddle).then(|| {
        if e.is_under_water() {
            SoundEvent::ITEM_NAUTILUS_SADDLE_UNDERWATER_EQUIP
        } else {
            SoundEvent::ITEM_NAUTILUS_SADDLE_EQUIP
        }
    })
}

/// Implements the `Entity` members `Nautilus` and `ZombieNautilus` share, forwarding to this module (what a class overrides
/// - the brain step, the sounds, the dimensions, the breeding, the save data - is written per class).
macro_rules! abstract_nautilus_entity {
    () => {
        fn is_pushed_by_fluid(&self) -> bool {
            false
        }
        fn get_walk_target_value(
            &self,
            _pos: mc_core::pos::BlockPos,
            _level: &dyn mc_game_core::world::level::LevelReader,
        ) -> f32 {
            0.0
        }
        fn check_spawn_obstruction(
            &self,
            level: &dyn mc_game_core::world::level::server_level_accessor::ServerLevelAccessor,
        ) -> bool {
            level.is_unobstructed_entity(self)
        }
        fn can_use_slot(
            &self,
            slot: mc_game_core::world::entity::equipment::EquipmentSlot,
        ) -> bool {
            super::abstract_nautilus::can_use_slot(self, slot)
        }
        fn is_food(&self, stack: &mc_game_core::world::item::ItemStack) -> bool {
            super::abstract_nautilus::is_food(self, stack)
        }
        fn use_player_item(
            &mut self,
            level: &mut mc_game_core::server::level::ServerLevel,
            player: &mut mc_game_core::world::entity::ServerPlayer,
            hand: mc_game_core::world::item::InteractionHand,
        ) {
            super::abstract_nautilus::use_player_item(level, player, hand);
        }
        fn can_dispenser_equip_into_slot(
            &self,
            slot: mc_game_core::world::entity::equipment::EquipmentSlot,
        ) -> bool {
            super::abstract_nautilus::can_dispenser_equip_into_slot(self, slot)
        }
        fn can_add_passenger(&self, _passenger: &dyn mc_game_core::world::entity::Entity) -> bool {
            !self.base().is_vehicle()
        }
        fn get_controlling_passenger(&self) -> Option<mc_game_core::world::entity::EntityId> {
            super::abstract_nautilus::get_controlling_passenger(self)
        }
        fn add_passenger(
            &mut self,
            passenger: &dyn mc_game_core::world::entity::Entity,
            first_passenger_is_player: bool,
        ) {
            super::abstract_nautilus::add_passenger(self, passenger, first_passenger_is_player);
        }
        fn on_passenger_removed(
            &mut self,
            level: &mut mc_game_core::server::level::ServerLevel,
            _passenger: mc_game_core::world::entity::EntityId,
        ) {
            super::abstract_nautilus::on_passenger_removed(self, level);
        }
        fn get_ridden_input(
            &self,
            level: &mc_game_core::server::level::ServerLevel,
            controller: mc_game_core::world::entity::EntityId,
            _self_input: mc_core::phys::Vec3,
        ) -> mc_core::phys::Vec3 {
            super::abstract_nautilus::get_ridden_input(level, controller)
        }
        fn tick_ridden(
            &mut self,
            level: &mut mc_game_core::server::level::ServerLevel,
            controller: mc_game_core::world::entity::EntityId,
            _ridden_input: mc_core::phys::Vec3,
        ) {
            super::abstract_nautilus::tick_ridden(self, level, controller);
        }
        fn travel_in_water(
            &mut self,
            level: &mut mc_game_core::server::level::ServerLevel,
            input: mc_core::phys::Vec3,
            _base_gravity: f64,
            _is_falling: bool,
            _old_y: f64,
        ) {
            super::abstract_nautilus::travel_in_water(self, level, input);
        }
        fn get_ridden_speed(
            &self,
            _level: &mc_game_core::server::level::ServerLevel,
            _controller: mc_game_core::world::entity::EntityId,
        ) -> f32 {
            super::abstract_nautilus::get_ridden_speed(self)
        }
        fn tick(&mut self, level: &mut mc_game_core::server::level::ServerLevel) {
            super::abstract_nautilus::tick(self, level);
        }
        fn can_jump(&self, _level: &mc_game_core::server::level::ServerLevel) -> bool {
            super::abstract_nautilus::is_saddled(self)
        }
        fn is_player_rideable_jumping(&self) -> bool {
            true
        }
        fn on_player_jump(&mut self, jump_amount: i32) {
            super::abstract_nautilus::on_player_jump(self, jump_amount);
        }
        fn handle_start_jump(
            &mut self,
            level: &mut mc_game_core::server::level::ServerLevel,
            _jump_scale: i32,
        ) {
            super::abstract_nautilus::handle_start_jump(self, level);
        }
        fn handle_stop_jump(&mut self) {}
        fn on_synced_data_updated(&mut self, accessor_id: u8) {
            super::abstract_nautilus::on_synced_data_updated(self, accessor_id);
        }
        fn play_step_sound(
            &mut self,
            _level: &mut mc_game_core::server::level::ServerLevel,
            _pos: mc_core::pos::BlockPos,
            _state: mc_data::block::BlockState,
        ) {
        }
        fn interact(
            &mut self,
            level: &mut mc_game_core::server::level::ServerLevel,
            player: &mut mc_game_core::world::entity::ServerPlayer,
            hand: mc_game_core::world::item::InteractionHand,
            location: mc_core::phys::Vec3,
        ) -> mc_game_core::world::item::InteractionResult {
            super::abstract_nautilus::interact(self, level, player, hand, location)
        }
        fn mob_interact(
            &mut self,
            level: &mut mc_game_core::server::level::ServerLevel,
            player: &mut mc_game_core::world::entity::ServerPlayer,
            hand: mc_game_core::world::item::InteractionHand,
        ) -> mc_game_core::world::item::InteractionResult {
            super::abstract_nautilus::mob_interact(self, level, player, hand)
        }
        fn remove_when_far_away(&self, _distance_to_closest_player: f64) -> bool {
            true
        }
        fn requires_custom_persistence(&self) -> bool {
            self.base().is_passenger()
                || mc_game_core::world::entity::mob::leashable::is_leashed(self.as_dyn())
                || mc_game_core::world::entity::mob::tamable::is_tame(self.as_dyn())
        }
        fn hurt_server(
            &mut self,
            level: &mut mc_game_core::server::level::ServerLevel,
            source: &mc_game_core::world::damagesource::DamageSource,
            damage: f32,
        ) -> bool {
            super::abstract_nautilus::hurt_server(self, level, source, damage)
        }
        fn die(
            &mut self,
            level: &mut mc_game_core::server::level::ServerLevel,
            source: &mc_game_core::world::damagesource::DamageSource,
        ) {
            mc_game_core::world::entity::mob::tamable::die(self, level, source);
        }
        fn can_be_affected(
            &self,
            level: &mc_game_core::server::level::ServerLevel,
            effect: &mc_game_core::world::entity::effect::MobEffectInstance,
        ) -> bool {
            effect.effect != mc_data::MobEffect::POISON
                && mc_game_core::world::entity::living_entity::effects::can_be_affected(
                    self.as_dyn(),
                    level,
                    effect,
                )
        }
        fn get_equip_sound_override(
            &self,
            slot: mc_game_core::world::entity::equipment::EquipmentSlot,
        ) -> Option<mc_data::SoundEvent> {
            super::abstract_nautilus::get_equip_sound_override(self, slot)
        }
        fn has_custom_inventory_screen(&self) -> bool {
            true
        }
        fn open_custom_inventory_screen(
            &mut self,
            level: &mut mc_game_core::server::level::ServerLevel,
            player: &mut mc_game_core::world::entity::ServerPlayer,
        ) {
            super::abstract_nautilus::open_custom_inventory_screen(self, level, player);
        }
        fn as_container(&self) -> Option<&dyn mc_game_core::world::container::Container> {
            Some(&self.nautilus().inventory)
        }
        fn as_container_mut(
            &mut self,
        ) -> Option<&mut dyn mc_game_core::world::container::Container> {
            Some(&mut self.nautilus_mut().inventory)
        }
        fn as_mount_inventory(
            &self,
        ) -> Option<&dyn mc_game_core::world::inventory::MountInventory> {
            Some(self)
        }
        fn as_mount_inventory_mut(
            &mut self,
        ) -> Option<&mut dyn mc_game_core::world::inventory::MountInventory> {
            Some(self)
        }
    };
}
pub(super) use abstract_nautilus_entity;

/// `impl MountInventory` for a nautilus class (the screen asks the mob about its slots).
macro_rules! nautilus_mount_inventory {
    ($ty:ty) => {
        impl mc_game_core::world::inventory::MountInventory for $ty {
            fn get_inventory_columns(&self) -> i32 {
                0
            }
            fn can_use_slot(
                &self,
                slot: mc_game_core::world::entity::equipment::EquipmentSlot,
            ) -> bool {
                mc_game_core::world::entity::Entity::can_use_slot(self, slot)
            }
            fn is_equippable_in_slot(
                &self,
                stack: &mc_game_core::world::item::ItemStack,
                slot: mc_game_core::world::entity::equipment::EquipmentSlot,
            ) -> bool {
                mc_game_core::world::entity::Entity::is_equippable_in_slot(self, stack, slot)
            }
            fn get_equipment_slot_item(
                &self,
                slot: mc_game_core::world::entity::equipment::EquipmentSlot,
            ) -> mc_game_core::world::item::ItemStack {
                self.get_item_by_slot(slot)
            }
            fn set_equipment_slot_item(
                &mut self,
                level: &mut mc_game_core::server::level::ServerLevel,
                slot: mc_game_core::world::entity::equipment::EquipmentSlot,
                stack: mc_game_core::world::item::ItemStack,
            ) {
                let empty = stack.is_empty();
                mc_game_core::world::entity::mob::equipment::set_item_slot(
                    self.as_dyn_mut(),
                    level,
                    slot,
                    stack,
                );
                if !empty {
                    mc_game_core::world::entity::mob::equipment::set_guaranteed_drop(
                        self.as_dyn_mut(),
                        slot,
                    );
                    if let Some(m) = self.mob_mut() {
                        m.persistence_required = true;
                    }
                }
            }
        }
    };
}
pub(super) use nautilus_mount_inventory;

/// The `AbstractNautilusData` of a nautilus class.
fn data_of(e: &dyn Entity) -> &AbstractNautilusData {
    if let Some(n) = e.as_any().downcast_ref::<super::nautilus::Nautilus>() {
        &n.nautilus
    } else {
        &e.as_any()
            .downcast_ref::<super::zombie_nautilus::ZombieNautilus>()
            .expect("nautilus entity")
            .nautilus
    }
}

/// `dump` fields of `AbstractNautilus`: the dash cooldown and the pending jump scale.
#[distributed_slice(SCENARIO_DUMP_EXTRA)]
static NAUTILUS_DUMP: ScenarioDumpExtra = ScenarioDumpExtra {
    order: 32,
    applies: |e| {
        matches!(
            e.get_type(),
            EntityType::NAUTILUS | EntityType::ZOMBIE_NAUTILUS
        )
    },
    fields: |e, _level, _ctx| {
        let n = data_of(e);
        vec![
            ("dashcd".into(), n.dash_cooldown.to_string()),
            (
                "jscale".into(),
                java_float_to_string(n.player_jump_pending_scale),
            ),
        ]
    },
};

/// `dump` fields of `ZombieNautilus`: the variant.
#[distributed_slice(SCENARIO_DUMP_EXTRA)]
static ZOMBIE_NAUTILUS_DUMP: ScenarioDumpExtra = ScenarioDumpExtra {
    order: 33,
    applies: |e| e.get_type() == EntityType::ZOMBIE_NAUTILUS,
    fields: |e, _level, _ctx| {
        let zombie = e
            .as_any()
            .downcast_ref::<super::zombie_nautilus::ZombieNautilus>()
            .expect("zombie nautilus");
        vec![("variant".into(), zombie.variant_key().to_string())]
    },
};
