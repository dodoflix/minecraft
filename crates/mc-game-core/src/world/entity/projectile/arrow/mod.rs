//! `net.minecraft.world.entity.projectile.arrow`: `AbstractArrow` (pickup rules, in-ground state, piercing, crit,
//! knockback, enchantment hooks) and its subclasses `Arrow`, `SpectralArrow`, `ThrownTrident`.
//!
//! A concrete arrow embeds an [`AbstractArrowData`] next to its `EntityData`/`ProjectileData` and implements
//! [`AbstractArrow`] (the virtual hooks, defaults = vanilla `AbstractArrow`). The shared behavior lives in this
//! module's free functions (`tick`, `on_hit_entity`, `on_hit_block`, ...), which a subclass override calls as its
//! "super"; `impl_abstract_arrow!` wires the `Entity`/`Projectile` overrides every arrow shares.

pub mod arrow;
pub mod spectral_arrow;
pub mod thrown_trident;

pub use arrow::Arrow;
pub use spectral_arrow::SpectralArrow;
pub use thrown_trident::ThrownTrident;

use super::{
    EntityHitResult, HitResult, HitType, Projectile, ProjectileDeflection, check_left_owner,
    lerp_rotation,
};
use crate::define_accessor;
use crate::server::level::ServerLevel;
use crate::world::entity::movement::collision_context;
use crate::world::entity::player::ServerPlayer;
use crate::world::entity::{Entity, EntityId, MoverType};
use crate::world::item::enchantment::actors::Actors;
use crate::world::item::{ItemStack, ItemStackExt as _};
use crate::world::level::BlockGetter;
use crate::world::level::collision::{ClipBlock, ClipContext, ClipFluid, CollisionGetter};
use mc_core::math::mth;
use mc_core::phys::{AABB, BlockHitResult, Vec3};
use mc_data::block::BlockState;
use mc_data::{EntityType, SoundEvent};
use mc_nbt::{CompoundTag, Tag};
use mc_protocol::packets::play::ClientboundGameEventPacket;

define_accessor!(pub ID_FLAGS: Byte = AbstractArrow);
define_accessor!(pub PIERCE_LEVEL: Byte = AbstractArrow);
define_accessor!(pub IN_GROUND: Boolean = AbstractArrow);

const ARROW_BASE_DAMAGE: f64 = 2.0;
const SHAKE_TIME: i32 = 7;
const FLAG_CRIT: i32 = 1;
const FLAG_NOPHYSICS: i32 = 2;

/// What a hit decided about the victim (trident hit logic).
#[derive(Default)]
pub struct OnHitEntityOutcome {
    pub was_hurt: bool,
    pub receives_side_effects: bool,
}

/// `AbstractArrow.Pickup`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Pickup {
    Disallowed,
    Allowed,
    CreativeOnly,
}

impl Pickup {
    /// `Pickup.byOrdinal(ordinal)` (`LEGACY_CODEC`: a byte; out-of-range ordinals are `DISALLOWED`, vanilla's
    /// `ordinal > values().length` check even lets `3` through to an `ArrayIndexOutOfBounds`: treated as `DISALLOWED`).
    pub fn by_ordinal(ordinal: i32) -> Pickup {
        match ordinal {
            1 => Pickup::Allowed,
            2 => Pickup::CreativeOnly,
            _ => Pickup::Disallowed,
        }
    }

    pub fn ordinal(self) -> i8 {
        self as i8
    }
}

/// The fields `AbstractArrow` adds to `Projectile`.
pub struct AbstractArrowData {
    /// `lastState`.
    pub last_state: Option<BlockState>,
    pub in_ground_time: i32,
    pub pickup: Pickup,
    pub shake_time: i32,
    pub life: i32,
    pub base_damage: f64,
    pub sound_event: SoundEvent,
    /// `piercingIgnoreEntityIds` (an `IntOpenHashSet`: set semantics).
    pub piercing_ignore_entity_ids: Option<Vec<EntityId>>,
    /// `piercedAndKilledEntities`.
    pub pierced_and_killed_entities: Option<Vec<EntityId>>,
    pub pickup_item_stack: ItemStack,
    /// `firedFromWeapon`.
    pub fired_from_weapon: Option<ItemStack>,
}

impl AbstractArrowData {
    /// The field initializers (`soundEvent = getDefaultHitGroundSoundEvent()`, `pickupItemStack = getDefaultPickupItem()`).
    pub fn new(default_hit_ground_sound: SoundEvent, default_pickup_item: ItemStack) -> Self {
        Self {
            last_state: None,
            in_ground_time: 0,
            pickup: Pickup::Disallowed,
            shake_time: 0,
            life: 0,
            base_damage: ARROW_BASE_DAMAGE,
            sound_event: default_hit_ground_sound,
            piercing_ignore_entity_ids: None,
            pierced_and_killed_entities: None,
            pickup_item_stack: default_pickup_item,
            fired_from_weapon: None,
        }
    }
}

/// `defineSynchedData` of `AbstractArrow`.
pub fn define_synched_data(b: &mut crate::world::entity::synched_data::SynchedEntityDataBuilder) {
    b.define(ID_FLAGS, 0);
    b.define(PIERCE_LEVEL, 0);
    b.define(IN_GROUND, false);
}

/// `Math.signum(double)`.
fn java_signum(v: f64) -> f64 {
    if v == 0.0 || v.is_nan() {
        v
    } else {
        v.signum()
    }
}

/// `AbstractArrow` and its overridable hooks.
pub trait AbstractArrow: Projectile {
    fn arrow_data(&self) -> &AbstractArrowData;
    fn arrow_data_mut(&mut self) -> &mut AbstractArrowData;
    fn as_abstract_arrow(&self) -> &dyn AbstractArrow;
    fn as_abstract_arrow_mut(&mut self) -> &mut dyn AbstractArrow;

    /// `getDefaultPickupItem()`.
    fn get_default_pickup_item(&self) -> ItemStack;

    /// `getDefaultHitGroundSoundEvent()`.
    fn get_default_hit_ground_sound_event(&self) -> SoundEvent {
        SoundEvent::ENTITY_ARROW_HIT
    }

    /// `getWaterInertia()`.
    fn get_water_inertia(&self) -> f32 {
        0.6
    }

    /// `setPickupItemStack(itemStack)` (`Arrow` also refreshes its colour).
    fn set_pickup_item_stack(&mut self, stack: ItemStack) {
        set_pickup_item_stack_base(self.as_abstract_arrow_mut(), stack);
    }

    /// `doPostHurtEffects(mob)`: `mob` is out of the level while the hit runs.
    fn do_post_hurt_effects(&mut self, _level: &mut ServerLevel, _mob: &mut dyn Entity) {}

    /// `tickDespawn()`.
    fn tick_despawn(&mut self) {
        tick_despawn_base(self.as_abstract_arrow_mut());
    }

    /// `findHitEntities(from, to)`.
    fn find_hit_entities(&self, level: &ServerLevel, from: Vec3, to: Vec3) -> Vec<EntityHitResult> {
        find_hit_entities_base(self.as_abstract_arrow(), level, from, to)
    }

    /// `onHitEntity(hitResult)`.
    fn arrow_on_hit_entity(&mut self, level: &mut ServerLevel, hit: &EntityHitResult) {
        on_hit_entity(self.as_abstract_arrow_mut(), level, hit);
    }

    /// `hitBlockEnchantmentEffects(level, hitResult, weapon)`.
    fn hit_block_enchantment_effects(
        &mut self,
        level: &mut ServerLevel,
        hit: &BlockHitResult,
        weapon: ItemStack,
    ) {
        hit_block_enchantment_effects_base(self.as_abstract_arrow_mut(), level, hit, weapon);
    }

    /// `tryPickup(player)`.
    fn try_pickup(&mut self, player: &mut ServerPlayer) -> bool {
        try_pickup_base(self.as_abstract_arrow_mut(), player)
    }

    /// `getPickupItem()`.
    fn get_pickup_item(&self) -> ItemStack {
        self.arrow_data().pickup_item_stack.copy()
    }

    // ---- synched flags ----

    fn is_in_ground(&self) -> bool {
        self.base().entity_data.get(IN_GROUND)
    }

    fn set_in_ground(&mut self, in_ground: bool) {
        self.base_mut().entity_data.set(IN_GROUND, in_ground);
    }

    fn set_flag(&mut self, flag: i32, value: bool) {
        let flags = self.base().entity_data.get(ID_FLAGS);
        let flags = if value {
            flags | flag as i8
        } else {
            flags & !(flag as i8)
        };
        self.base_mut().entity_data.set(ID_FLAGS, flags);
    }

    fn is_crit_arrow(&self) -> bool {
        self.base().entity_data.get(ID_FLAGS) & FLAG_CRIT as i8 != 0
    }

    fn set_crit_arrow(&mut self, crit: bool) {
        self.set_flag(FLAG_CRIT, crit);
    }

    /// `getPierceLevel()`.
    fn pierce_level(&self) -> i8 {
        self.base().entity_data.get(PIERCE_LEVEL)
    }

    fn set_pierce_level(&mut self, level: i8) {
        self.base_mut().entity_data.set(PIERCE_LEVEL, level);
    }

    fn set_no_physics(&mut self, no_physics: bool) {
        self.base_mut().no_physics = no_physics;
        self.set_flag(FLAG_NOPHYSICS, no_physics);
    }

    /// `isNoPhysics()` (server side: the field).
    fn is_no_physics(&self) -> bool {
        self.base().no_physics
    }

    fn set_base_damage(&mut self, base_damage: f64) {
        self.arrow_data_mut().base_damage = base_damage;
    }

    fn set_sound_event(&mut self, sound: SoundEvent) {
        self.arrow_data_mut().sound_event = sound;
    }

    /// `setBaseDamageFromMob(power)`.
    fn set_base_damage_from_mob(&mut self, level: &ServerLevel, power: f32) {
        use mc_core::random::RandomSource as _;
        let difficulty = f64::from(level.difficulty().get_id()) * 0.11;
        let damage = f64::from(power * 2.0) + self.base_mut().random.triangle(difficulty, 0.57425);
        self.arrow_data_mut().base_damage = damage;
    }
}

/// The constructors' shared tail: `AbstractArrow(type, x, y, z, level, pickupItemStack, firedFromWeapon)`.
pub fn init_at(
    arrow: &mut dyn AbstractArrow,
    level: &mut ServerLevel,
    x: f64,
    y: f64,
    z: f64,
    mut pickup_item_stack: ItemStack,
    fired_from_weapon: Option<&ItemStack>,
) {
    arrow.arrow_data_mut().pickup_item_stack = pickup_item_stack.copy();
    crate::world::entity::spawn_config::apply_base_implicit_components(arrow, &pickup_item_stack);
    if pickup_item_stack
        .remove(mc_data::DataComponentType::INTANGIBLE_PROJECTILE)
        .is_some()
    {
        arrow.arrow_data_mut().pickup = Pickup::CreativeOnly;
    }
    arrow.set_pos(x, y, z);
    if let Some(weapon) = fired_from_weapon {
        assert!(!weapon.is_empty(), "Invalid weapon firing an arrow");
        arrow.arrow_data_mut().fired_from_weapon = Some(weapon.copy());
        let ammo = arrow.arrow_data().pickup_item_stack.clone();
        let pierce_level =
            crate::world::item::enchantment_helper::get_piercing_count(level, weapon, &ammo);
        if pierce_level > 0 {
            arrow.set_pierce_level(pierce_level as i8);
        }
    }
}

/// `AbstractArrow(type, mob, level, pickupItemStack, firedFromWeapon)`: at the mob's eyes, owned by it.
pub fn init_from_owner(
    arrow: &mut dyn AbstractArrow,
    level: &mut ServerLevel,
    mob: &dyn Entity,
    pickup_item_stack: ItemStack,
    fired_from_weapon: Option<&ItemStack>,
) {
    init_at(
        arrow,
        level,
        mob.base().x(),
        mob.base().get_eye_y() - f64::from(0.1f32),
        mob.base().z(),
        pickup_item_stack,
        fired_from_weapon,
    );
    arrow.set_owner(Some(mob));
}

/// `AbstractArrow.setOwner(owner)`.
pub fn set_owner(arrow: &mut dyn AbstractArrow, owner: Option<&dyn Entity>) {
    {
        let d = arrow.projectile_data_mut();
        d.owner = owner.map(|o| o.uuid());
        d.owner_id = owner.map(|o| o.id());
    }
    if let Some(owner) = owner {
        if owner.is_player() && arrow.arrow_data().pickup == Pickup::Disallowed {
            arrow.arrow_data_mut().pickup = Pickup::Allowed;
        } else if owner.get_type() == EntityType::OMINOUS_ITEM_SPAWNER {
            arrow.arrow_data_mut().pickup = Pickup::Disallowed;
        }
    }
}

/// `AbstractArrow.shoot`: `Projectile.shoot` then `life = 0`.
pub fn shoot(arrow: &mut dyn AbstractArrow, xd: f64, yd: f64, zd: f64, pow: f32, uncertainty: f32) {
    super::shoot_base(arrow, xd, yd, zd, pow, uncertainty);
    arrow.arrow_data_mut().life = 0;
}

pub fn set_pickup_item_stack_base(arrow: &mut dyn AbstractArrow, stack: ItemStack) {
    arrow.arrow_data_mut().pickup_item_stack = if stack.is_empty() {
        arrow.get_default_pickup_item()
    } else {
        stack
    };
}

/// `AbstractArrow.tick()`.
pub fn tick(arrow: &mut dyn AbstractArrow, level: &mut ServerLevel) {
    let physics_enabled = !arrow.is_no_physics();
    let movement = arrow.base().delta_movement();
    let block_pos = arrow.block_position();
    let block_state = level.get_block_state(block_pos);
    if !block_state.is_air() && physics_enabled {
        let shape = crate::world::level::block::state::BlockStateExt::get_collision_shape(
            block_state,
            &*level,
            block_pos,
        );
        if !shape.is_empty() {
            let position = arrow.position();
            for aabb in shape.to_aabbs() {
                if aabb.move_block_pos(&block_pos).contains_vec3(position) {
                    arrow.base_mut().set_delta_movement(Vec3::ZERO);
                    arrow.set_in_ground(true);
                    break;
                }
            }
        }
    }
    if arrow.arrow_data().shake_time > 0 {
        arrow.arrow_data_mut().shake_time -= 1;
    }
    if arrow.is_in_water_or_rain(level) {
        arrow.clear_fire();
    }
    if arrow.is_in_ground() && physics_enabled {
        if arrow.arrow_data().last_state != Some(block_state) && should_fall(arrow, level) {
            start_falling(arrow);
        } else {
            arrow.tick_despawn();
        }
        arrow.arrow_data_mut().in_ground_time += 1;
        if arrow.is_alive() {
            arrow.apply_effects_from_blocks(level);
        }
        let on_fire = arrow.get_remaining_fire_ticks() > 0;
        arrow.set_shared_flag_on_fire(on_fire);
    } else {
        arrow.arrow_data_mut().in_ground_time = 0;
        let original_position = arrow.position();
        if arrow.is_in_water() {
            let inertia = arrow.get_water_inertia();
            apply_inertia(arrow, inertia);
        }
        let y_rot = if !physics_enabled {
            (mth::atan2(-movement.x, -movement.z) * super::RAD_TO_DEG) as f32
        } else {
            (mth::atan2(movement.x, movement.z) * super::RAD_TO_DEG) as f32
        };
        let x_rot =
            (mth::atan2(movement.y, movement.horizontal_distance()) * super::RAD_TO_DEG) as f32;
        let new_x = lerp_rotation(arrow.base().x_rot(), x_rot);
        let new_y = lerp_rotation(arrow.base().y_rot(), y_rot);
        arrow.base_mut().set_x_rot(new_x);
        arrow.base_mut().set_y_rot(new_y);
        check_left_owner(arrow, level);
        if physics_enabled {
            let context = ClipContext::new(
                original_position,
                original_position.add(movement),
                ClipBlock::Collider,
                ClipFluid::None,
                collision_context(arrow.as_dyn()),
            );
            let block_hit = super::util::clip_including_border(level, &context);
            step_move_and_hit(arrow, level, block_hit);
        } else {
            arrow.set_pos_vec(original_position.add(movement));
            arrow.apply_effects_from_blocks(level);
        }
        if !arrow.is_in_water() {
            let drag = arrow.get_air_drag();
            apply_inertia(arrow, drag);
        }
        if physics_enabled && !arrow.is_in_ground() {
            arrow.apply_gravity();
        }
        super::tick(arrow, level);
    }
}

fn apply_inertia(arrow: &mut dyn AbstractArrow, inertia: f32) {
    let m = arrow.base().delta_movement();
    arrow
        .base_mut()
        .set_delta_movement(m.scale(f64::from(inertia)));
}

/// `shouldFall()`.
fn should_fall(arrow: &dyn AbstractArrow, level: &ServerLevel) -> bool {
    let p = arrow.position();
    arrow.is_in_ground()
        && level.no_collision(None, None, &AABB::from_corners(p, p).inflate_scalar(0.06))
}

/// `startFalling()`.
fn start_falling(arrow: &mut dyn AbstractArrow) {
    use mc_core::random::RandomSource as _;
    arrow.set_in_ground(false);
    let m = arrow.base().delta_movement();
    let random = &mut arrow.base_mut().random;
    let (x, y, z) = (
        random.next_float() * 0.2,
        random.next_float() * 0.2,
        random.next_float() * 0.2,
    );
    arrow
        .base_mut()
        .set_delta_movement(m.multiply_xyz(f64::from(x), f64::from(y), f64::from(z)));
    arrow.arrow_data_mut().life = 0;
}

/// `tickDespawn()`.
pub fn tick_despawn_base(arrow: &mut dyn AbstractArrow) {
    arrow.arrow_data_mut().life += 1;
    if arrow.arrow_data().life >= 1200 {
        arrow.discard();
    }
}

fn reset_pierced_entities(arrow: &mut dyn AbstractArrow) {
    let d = arrow.arrow_data_mut();
    if let Some(list) = &mut d.pierced_and_killed_entities {
        list.clear();
    }
    if let Some(set) = &mut d.piercing_ignore_entity_ids {
        set.clear();
    }
}

/// `stepMoveAndHit(blockHitResult)`.
fn step_move_and_hit(
    arrow: &mut dyn AbstractArrow,
    level: &mut ServerLevel,
    block_hit: BlockHitResult,
) {
    while arrow.is_alive() {
        let initial_position = arrow.position();
        let mut entities_hit = arrow.find_hit_entities(level, initial_position, block_hit.location);
        let key = |level: &ServerLevel, id: EntityId| {
            level
                .get_entity(id)
                .map_or(f64::MAX, |e| initial_position.distance_to_sqr(e.position()))
        };
        entities_hit.sort_by(|a, b| key(level, a.entity).total_cmp(&key(level, b.entity)));
        let next_location = entities_hit
            .first()
            .map_or(block_hit.location, |e| e.location);
        arrow.set_pos_vec(next_location);
        arrow.apply_effects_from_blocks_between(level, initial_position, next_location);
        if arrow
            .base()
            .portal_process
            .as_ref()
            .is_some_and(|p| p.is_inside_portal_this_tick())
        {
            arrow.handle_portal(level);
        }
        if entities_hit.is_empty() {
            if arrow.is_alive() && HitResult::Block(block_hit).get_type() != HitType::Miss {
                super::hit_target_or_deflect_self(arrow, level, &HitResult::Block(block_hit));
                arrow.base_mut().needs_sync = true;
            }
            break;
        } else if arrow.is_alive() && !arrow.base().no_physics {
            let deflection = hit_targets_or_deflect_self(arrow, level, &entities_hit);
            arrow.base_mut().needs_sync = true;
            if arrow.pierce_level() > 0 && deflection == ProjectileDeflection::None {
                continue;
            }
            break;
        }
    }
}

/// `hitTargetsOrDeflectSelf(entityHitResults)`.
fn hit_targets_or_deflect_self(
    arrow: &mut dyn AbstractArrow,
    level: &mut ServerLevel,
    hits: &[EntityHitResult],
) -> ProjectileDeflection {
    for e in hits {
        let deflection = super::hit_target_or_deflect_self(arrow, level, &HitResult::Entity(*e));
        if !arrow.is_alive() || deflection != ProjectileDeflection::None {
            return deflection;
        }
    }
    ProjectileDeflection::None
}

/// `findHitEntities(from, to)`: every entity along the way, `canHitEntity` filtered.
pub fn find_hit_entities_base(
    arrow: &dyn AbstractArrow,
    level: &ServerLevel,
    from: Vec3,
    to: Vec3,
) -> Vec<EntityHitResult> {
    let area = arrow
        .get_bounding_box()
        .expand_towards_vec3(arrow.base().delta_movement())
        .inflate_scalar(1.0);
    super::util::get_many_entity_hit_result(
        level,
        arrow.as_dyn(),
        from,
        to,
        &area,
        &|e| arrow.can_hit_entity(level, e),
        false,
        false,
    )
}

/// `findHitEntity(from, to)`.
pub fn find_hit_entity(
    arrow: &dyn AbstractArrow,
    level: &ServerLevel,
    from: Vec3,
    to: Vec3,
) -> Option<EntityHitResult> {
    let area = arrow
        .get_bounding_box()
        .expand_towards_vec3(arrow.base().delta_movement())
        .inflate_scalar(1.0);
    super::util::get_entity_hit_result(level, arrow.as_dyn(), from, to, &area, &|e| {
        arrow.can_hit_entity(level, e)
    })
}

/// `AbstractArrow.canHitEntity(entity)`.
pub fn can_hit_entity(arrow: &dyn AbstractArrow, level: &ServerLevel, target: &dyn Entity) -> bool {
    if target.is_player()
        && let Some(owner) = arrow.get_owner(level)
        && owner.is_player()
        && !crate::world::entity::team::do_teams_allow_damage(level, owner, target)
    {
        return false;
    }
    default_can_hit_entity(arrow, level, target)
        && arrow
            .arrow_data()
            .piercing_ignore_entity_ids
            .as_ref()
            .is_none_or(|s| !s.contains(&target.id()))
}

/// `Projectile.canHitEntity(entity)`.
fn default_can_hit_entity(
    arrow: &dyn AbstractArrow,
    level: &ServerLevel,
    entity: &dyn Entity,
) -> bool {
    if !entity.can_be_hit_by_projectile() {
        return false;
    }
    let owner = arrow.get_owner(level);
    owner.is_none_or(|o| {
        arrow.projectile_data().left_owner || !level.is_passenger_of_same_vehicle(o, entity)
    })
}

/// `Math.ceil` clamp of the arrow damage: `Mth.ceil(Mth.clamp(pow * arrowDamage, 0.0, 2.147483647E9))`.
fn damage_from(pow: f32, arrow_damage: f64) -> i32 {
    mth::ceil_f64(mth::clamp_f64(
        f64::from(pow) * arrow_damage,
        0.0,
        2.147483647E9,
    ))
}

/// `AbstractArrow.onHitEntity(hitResult)`.
pub fn on_hit_entity(
    arrow: &mut dyn AbstractArrow,
    level: &mut ServerLevel,
    hit: &EntityHitResult,
) {
    use crate::world::item::enchantment_helper as helper;
    use mc_core::random::RandomSource as _;
    let victim = hit.entity;
    let pow = arrow.base().delta_movement().length() as f32;
    let mut arrow_damage = arrow.arrow_data().base_damage;
    let owner_id = arrow.get_owner(level).map(|o| o.id());
    let owner_is_living = arrow.get_owner(level).is_some_and(|o| o.living().is_some());
    let source = {
        let owner = arrow.get_owner(level);
        level
            .damage_sources()
            .arrow(arrow.as_dyn(), Some(owner.unwrap_or(arrow.as_dyn())))
    };
    let weapon = arrow.get_weapon_item();
    if let Some(weapon) = &weapon {
        arrow_damage = f64::from(helper::modify_damage(
            level,
            &mut Actors::of(arrow.as_dyn_mut()),
            weapon,
            victim,
            &source,
            arrow_damage as f32,
        ));
    }
    let mut damage = damage_from(pow, arrow_damage);
    let pierce_level = arrow.pierce_level();
    if pierce_level > 0 {
        let d = arrow.arrow_data_mut();
        let ignore = d
            .piercing_ignore_entity_ids
            .get_or_insert_with(|| Vec::with_capacity(5));
        d.pierced_and_killed_entities
            .get_or_insert_with(|| Vec::with_capacity(5));
        if ignore.len() as i32 > i32::from(pierce_level) {
            arrow.discard();
            return;
        }
        if !ignore.contains(&victim) {
            ignore.push(victim);
        }
    }
    if arrow.is_crit_arrow() {
        let increase = i64::from(arrow.base_mut().random.next_int_bounded(damage / 2 + 2));
        damage = (increase + i64::from(damage)).min(i64::from(i32::MAX)) as i32;
    }
    if owner_is_living && let Some(owner) = owner_id {
        level.with_entity(owner, |level, o| {
            if let Some(target) = level.get_entity(victim) {
                o.set_last_hurt_mob(target);
            }
        });
    }
    let on_fire = arrow.is_on_fire();
    let silent = arrow.base().is_silent();
    let mut outcome = HitOutcome::Missing;
    level.with_entity(victim, |level, target| {
        let remaining_fire_ticks = target.get_remaining_fire_ticks();
        if on_fire {
            target.ignite_for_seconds(5.0);
        }
        if target.hurt_server(level, &source, damage as f32) {
            if target.living().is_some() {
                if pierce_level <= 0 {
                    let count = target
                        .base()
                        .entity_data
                        .get(crate::world::entity::living::DATA_ARROW_COUNT_ID);
                    target
                        .base_mut()
                        .entity_data
                        .set(crate::world::entity::living::DATA_ARROW_COUNT_ID, count + 1);
                }
                do_knockback(arrow, level, target, &source);
                helper::do_post_attack_effects_with_item_source(
                    level,
                    &mut Actors::of(target).and(arrow.as_dyn_mut()),
                    victim,
                    &source,
                    weapon.as_ref(),
                );
                arrow.do_post_hurt_effects(level, target);
                if target.is_player()
                    && let Some(owner) = owner_id
                    && !silent
                    && owner != victim
                {
                    level.with_player(owner, |_, player| {
                        player.send(&ClientboundGameEventPacket {
                            event: ClientboundGameEventPacket::PLAY_ARROW_HIT_SOUND,
                            param: 0.0,
                        });
                    });
                }
                if !target.is_alive()
                    && let Some(list) = &mut arrow.arrow_data_mut().pierced_and_killed_entities
                {
                    list.push(victim);
                }
                // `CriteriaTriggers.KILLED_BY_ARROW` when the owner is a ServerPlayer: the pierced list, or the victim if dead
                if let Some(owner) = owner_id {
                    let killed: Option<Vec<EntityId>> =
                        match &arrow.arrow_data().pierced_and_killed_entities {
                            Some(list) => Some(list.clone()),
                            None => (!target.is_alive()).then(|| vec![victim]),
                        };
                    let matches = killed.and_then(|ids| {
                        let player = level.get_entity(owner).and_then(|e| {
                            e.as_any()
                                .downcast_ref::<crate::world::entity::ServerPlayer>()
                        })?;
                        let victims: Vec<&dyn Entity> = ids
                            .iter()
                            .filter_map(|&id| {
                                if id == victim {
                                    Some(&*target)
                                } else {
                                    level.get_entity(id)
                                }
                            })
                            .collect();
                        Some(
                            crate::advancements::triggers::CriteriaTriggers::KILLED_BY_ARROW
                                .matches(&*level, player, &victims, weapon.as_ref()),
                        )
                    });
                    if let Some(matches) = matches {
                        level.with_player(owner, |level, player| matches.award(level, player));
                    }
                }
            }
            outcome = HitOutcome::Hurt;
        } else if target.projectile_receives_side_effects_on_hit(false) {
            target.set_remaining_fire_ticks(remaining_fire_ticks);
            outcome = HitOutcome::Bounced;
        } else {
            outcome = HitOutcome::Ignored;
        }
    });
    match outcome {
        HitOutcome::Hurt => {
            let pitch = 1.2 / (arrow.base_mut().random.next_float() * 0.2 + 0.9);
            let sound = arrow.arrow_data().sound_event;
            arrow.play_sound(level, sound, 1.0, pitch);
            if arrow.pierce_level() <= 0 {
                arrow.discard();
            }
        }
        HitOutcome::Bounced => {
            let owner = arrow.projectile_data().owner;
            arrow.deflect(
                level,
                ProjectileDeflection::Reverse,
                Some(victim),
                owner,
                false,
                Vec3::new(0.2, 0.2, 0.2),
            );
            if arrow.base().delta_movement().length_sqr() < 1.0E-7 {
                if arrow.arrow_data().pickup == Pickup::Allowed {
                    crate::world::entity::item::spawn_at_location(
                        level,
                        arrow.as_dyn(),
                        arrow.get_pickup_item(),
                        Vec3::new(0.0, 0.1f32.into(), 0.0),
                    );
                }
                arrow.discard();
            }
        }
        HitOutcome::Ignored | HitOutcome::Missing => {}
    }
}

enum HitOutcome {
    Missing,
    Hurt,
    Bounced,
    Ignored,
}

/// `doKnockback(mob, damageSource)`: `mob` is out of the level.
pub fn do_knockback(
    arrow: &mut dyn AbstractArrow,
    level: &mut ServerLevel,
    mob: &mut dyn Entity,
    source: &crate::world::damagesource::DamageSource,
) {
    let knockback = match arrow.arrow_data().fired_from_weapon.clone() {
        Some(weapon) => {
            let id = mob.id();
            f64::from(crate::world::item::enchantment_helper::modify_knockback(
                level,
                &mut Actors::of(&mut *mob).and(arrow.as_dyn_mut()),
                &weapon,
                id,
                source,
                0.0,
            ))
        }
        None => 0.0,
    };
    if knockback > 0.0 {
        let resistance = (1.0
            - mob.living().map_or(0.0, |l| {
                l.get_attribute_value(mc_data::Attribute::KNOCKBACK_RESISTANCE)
            }))
        .max(0.0);
        let movement = arrow
            .base()
            .delta_movement()
            .multiply_xyz(1.0, 0.0, 1.0)
            .normalize()
            .scale(knockback * 0.6 * resistance);
        if movement.length_sqr() > 0.0 {
            mob.push_xyz(movement.x, 0.1, movement.z);
        }
    }
}

/// `AbstractArrow.onHitBlock(hitResult)`.
pub fn on_hit_block(arrow: &mut dyn AbstractArrow, level: &mut ServerLevel, hit: &BlockHitResult) {
    use mc_core::random::RandomSource as _;
    arrow.arrow_data_mut().last_state = Some(level.get_block_state(hit.block_pos));
    super::on_hit_block_base(arrow, level, hit);
    if let Some(weapon) = arrow.get_weapon_item() {
        arrow.hit_block_enchantment_effects(level, hit, weapon);
    }
    let movement = arrow.base().delta_movement();
    let offset_direction = Vec3::new(
        java_signum(movement.x),
        java_signum(movement.y),
        java_signum(movement.z),
    );
    let scaled_movement = offset_direction.scale(f64::from(0.05f32));
    let position = arrow.position().subtract(scaled_movement);
    arrow.set_pos_vec(position);
    arrow.base_mut().set_delta_movement(Vec3::ZERO);
    let pitch = 1.2 / (arrow.base_mut().random.next_float() * 0.2 + 0.9);
    let sound = arrow.arrow_data().sound_event;
    arrow.play_sound(level, sound, 1.0, pitch);
    arrow.set_in_ground(true);
    arrow.arrow_data_mut().shake_time = SHAKE_TIME;
    arrow.set_crit_arrow(false);
    arrow.set_pierce_level(0);
    arrow.set_sound_event(SoundEvent::ENTITY_ARROW_HIT);
    reset_pierced_entities(arrow);
}

/// `AbstractArrow.hitBlockEnchantmentEffects`: `EnchantmentHelper.onHitBlock` with the arrow's weapon; when the weapon
/// breaks, `firedFromWeapon` is dropped.
pub fn hit_block_enchantment_effects_base(
    arrow: &mut dyn AbstractArrow,
    level: &mut ServerLevel,
    hit: &BlockHitResult,
    weapon: ItemStack,
) {
    let (weapon, broke) = run_on_hit_block(arrow, level, hit, weapon);
    // the (possibly modified) weapon is the live `firedFromWeapon` object in vanilla
    arrow.arrow_data_mut().fired_from_weapon = if broke { None } else { Some(weapon) };
}

/// `EnchantmentHelper.onHitBlock(level, weapon, owner, this, null, clamped hit position, block state, onBreak)`:
/// returns the weapon after the effects and whether one of them broke it (the `onBreak` callback ran).
pub fn run_on_hit_block(
    arrow: &mut dyn AbstractArrow,
    level: &mut ServerLevel,
    hit: &BlockHitResult,
    mut weapon: ItemStack,
) -> (ItemStack, bool) {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};
    let compensated = hit.block_pos.clamp_location_within(&hit.location);
    let state = level.get_block_state(hit.block_pos);
    let owner = arrow
        .get_owner(level)
        .filter(|o| o.living().is_some())
        .map(|o| o.id());
    let broke = Arc::new(AtomicBool::new(false));
    let flag = broke.clone();
    let id = arrow.id();
    crate::world::item::enchantment_helper::on_hit_block(
        level,
        &mut Actors::of(arrow.as_dyn_mut()),
        &mut weapon,
        owner,
        id,
        None,
        compensated,
        state,
        Box::new(move |_, _| flag.store(true, Ordering::Relaxed)),
    );
    let broke = broke.load(Ordering::Relaxed);
    (weapon, broke)
}

/// `AbstractArrow.playerTouch(player)`.
pub fn player_touch(
    arrow: &mut dyn AbstractArrow,
    level: &mut ServerLevel,
    player: &mut ServerPlayer,
) {
    if (arrow.is_in_ground() || arrow.is_no_physics())
        && arrow.arrow_data().shake_time <= 0
        && arrow.try_pickup(player)
    {
        player.take(level, arrow.as_dyn(), 1);
        arrow.discard();
    }
}

/// `AbstractArrow.tryPickup(player)`.
pub fn try_pickup_base(arrow: &mut dyn AbstractArrow, player: &mut ServerPlayer) -> bool {
    match arrow.arrow_data().pickup {
        Pickup::Disallowed => false,
        Pickup::Allowed => {
            let mut stack = arrow.get_pickup_item();
            let infinite = player.has_infinite_materials();
            player.player.inventory.add(&mut stack, infinite)
        }
        Pickup::CreativeOnly => player.has_infinite_materials(),
    }
}

/// `AbstractArrow.move(moverType, delta)`.
pub fn move_arrow(
    arrow: &mut dyn AbstractArrow,
    level: &mut ServerLevel,
    mover: MoverType,
    delta: Vec3,
) {
    crate::world::entity::movement::move_entity(arrow.as_dyn_mut(), level, mover, delta);
    if mover != MoverType::SelfMover && should_fall(arrow, level) {
        start_falling(arrow);
    }
}

/// `AbstractArrow.addAdditionalSaveData`.
pub fn add_additional_save_data(arrow: &dyn AbstractArrow, out: &mut CompoundTag) {
    super::add_additional_save_data(arrow, out);
    let d = arrow.arrow_data();
    out.put_short("life", d.life as i16);
    if let Some(state) = d.last_state
        && let Ok(tag) = mc_dfu::encode_nbt(
            &crate::world::item::enchantment::value_codecs::block_state_codec(),
            &state,
        )
        .into_result()
    {
        out.put("inBlockState", tag);
    }
    out.put_byte("shake", d.shake_time as i8);
    out.put_boolean("inGround", arrow.is_in_ground());
    out.put_byte("pickup", d.pickup.ordinal());
    out.put_double("damage", d.base_damage);
    out.put_boolean("crit", arrow.is_crit_arrow());
    out.put_byte("PierceLevel", arrow.pierce_level());
    out.put_string("SoundEvent", d.sound_event.name());
    out.put("item", Tag::Compound(save_stack(&d.pickup_item_stack)));
    if let Some(weapon) = &d.fired_from_weapon {
        out.put("weapon", Tag::Compound(save_stack(weapon)));
    }
}

fn save_stack(stack: &ItemStack) -> CompoundTag {
    let mut t = CompoundTag::new();
    stack.save_into(&mut t);
    t
}

/// `AbstractArrow.readAdditionalSaveData`.
pub fn read_additional_save_data(arrow: &mut dyn AbstractArrow, input: &CompoundTag) {
    use crate::world::item::load_item_stack;
    super::read_additional_save_data(arrow, input);
    arrow.arrow_data_mut().life = i32::from(input.get_short_or("life", 0));
    arrow.arrow_data_mut().last_state = input.get("inBlockState").and_then(|t| {
        mc_dfu::parse_nbt(
            &crate::world::item::enchantment::value_codecs::block_state_codec(),
            t,
        )
        .into_result()
        .ok()
    });
    arrow.arrow_data_mut().shake_time = i32::from(input.get_byte_or("shake", 0)) & 255;
    arrow.set_in_ground(input.get_boolean_or("inGround", false));
    arrow.arrow_data_mut().base_damage = input.get_double_or("damage", 2.0);
    arrow.arrow_data_mut().pickup = Pickup::by_ordinal(i32::from(input.get_byte_or("pickup", 0)));
    arrow.set_crit_arrow(input.get_boolean_or("crit", false));
    arrow.set_pierce_level(input.get_byte_or("PierceLevel", 0));
    let default_sound = arrow.get_default_hit_ground_sound_event();
    arrow.arrow_data_mut().sound_event = input
        .get_string("SoundEvent")
        .and_then(SoundEvent::by_name)
        .unwrap_or(default_sound);
    let pickup = input
        .get_compound("item")
        .and_then(load_item_stack)
        .unwrap_or_else(ItemStack::empty);
    arrow.set_pickup_item_stack(pickup);
    arrow.arrow_data_mut().fired_from_weapon =
        input.get_compound("weapon").and_then(load_item_stack);
}

/// `Entity`/`Projectile` overrides every arrow shares. `impl_abstract_arrow!(Type)` goes at module level; it
/// generates `impl Projectile for Type` pieces through `impl_abstract_arrow_projectile!` and the `Entity` pieces through
/// `impl_abstract_arrow_entity!`, which are placed inside the respective impl blocks.
#[macro_export]
macro_rules! impl_abstract_arrow_projectile {
    () => {
        fn pickup_item_stack_origin(&self) -> $crate::world::item::ItemStack {
            self.arrow_data().pickup_item_stack.clone()
        }
        fn set_owner(&mut self, owner: Option<&dyn $crate::world::entity::Entity>) {
            $crate::world::entity::projectile::arrow::set_owner(self, owner);
        }
        fn shoot(&mut self, xd: f64, yd: f64, zd: f64, pow: f32, uncertainty: f32) {
            $crate::world::entity::projectile::arrow::shoot(self, xd, yd, zd, pow, uncertainty);
        }
        fn can_hit_entity(
            &self,
            level: &$crate::server::level::ServerLevel,
            entity: &dyn $crate::world::entity::Entity,
        ) -> bool {
            $crate::world::entity::projectile::arrow::can_hit_entity(self, level, entity)
        }
        fn should_bounce_on_world_border(&self) -> bool {
            true
        }
        fn on_item_break(&mut self, _item: &$crate::world::item::ItemStack) {
            self.arrow_data_mut().fired_from_weapon = None;
        }
        fn on_hit_entity(
            &mut self,
            level: &mut $crate::server::level::ServerLevel,
            hit: &$crate::world::entity::projectile::EntityHitResult,
        ) {
            $crate::world::entity::projectile::arrow::AbstractArrow::arrow_on_hit_entity(
                self, level, hit,
            );
        }
        fn on_hit_block(
            &mut self,
            level: &mut $crate::server::level::ServerLevel,
            hit: &mc_core::phys::BlockHitResult,
        ) {
            $crate::world::entity::projectile::arrow::on_hit_block(self, level, hit);
        }
    };
}

/// The `AbstractArrow` overrides of `Entity` (see `impl_abstract_arrow_projectile!`).
#[macro_export]
macro_rules! impl_abstract_arrow_entity {
    () => {
        $crate::impl_projectile_entity!(arrow);
        fn tracking_kind(&self) -> $crate::world::entity::TrackingKind {
            $crate::world::entity::TrackingKind::Arrow
        }
        fn is_pickable(&self) -> bool {
            $crate::world::entity::projectile::is_pickable(self.get_type())
                && !$crate::world::entity::projectile::arrow::AbstractArrow::is_in_ground(self)
        }
        fn is_attackable(&self) -> bool {
            $crate::world::entity::projectile::is_pickable(self.get_type())
        }
        fn get_slot(&self, slot: i32) -> Option<$crate::world::item::ItemStack> {
            if slot == 0 {
                return Some(
                    $crate::world::entity::projectile::arrow::AbstractArrow::arrow_data(self)
                        .pickup_item_stack
                        .clone(),
                );
            }
            $crate::advancements::predicates::entity::support::get_slot(self, slot)
        }
        fn set_slot(&mut self, slot: i32, stack: $crate::world::item::ItemStack) {
            if slot == 0 {
                $crate::world::entity::projectile::arrow::AbstractArrow::set_pickup_item_stack(
                    self, stack,
                );
            }
        }
        fn snapshot_for_damage(&self) -> Option<Box<dyn $crate::world::entity::Entity>> {
            $crate::world::entity::projectile::snapshot_for_damage(
                self,
                Box::new(Self::new(self.get_type(), self.id())),
            )
        }
        fn get_movement_emission(&self) -> $crate::world::entity::MovementEmission {
            $crate::world::entity::MovementEmission::None
        }
        fn get_default_gravity(&self) -> f64 {
            0.05
        }
        fn get_air_drag(&self) -> f32 {
            0.99
        }
        fn is_pushed_by_fluid(&self) -> bool {
            !$crate::world::entity::projectile::arrow::AbstractArrow::is_in_ground(self)
        }
        fn r#move(
            &mut self,
            level: &mut $crate::server::level::ServerLevel,
            mover: $crate::world::entity::MoverType,
            delta: mc_core::phys::Vec3,
        ) {
            $crate::world::entity::projectile::arrow::move_arrow(self, level, mover, delta);
        }
        fn push_xyz(&mut self, xa: f64, ya: f64, za: f64) {
            if !$crate::world::entity::projectile::arrow::AbstractArrow::is_in_ground(self) {
                $crate::world::entity::projectile::arrow::push_default(self, xa, ya, za);
            }
        }
        fn on_above_bubble_column(
            &mut self,
            level: &mut $crate::server::level::ServerLevel,
            drag_down: bool,
            pos: mc_core::pos::BlockPos,
        ) {
            if !$crate::world::entity::projectile::arrow::AbstractArrow::is_in_ground(self) {
                $crate::world::entity::projectile::on_above_bubble_column(
                    self, level, drag_down, pos,
                );
            }
        }
        fn on_inside_bubble_column(
            &mut self,
            _level: &mut $crate::server::level::ServerLevel,
            drag_down: bool,
        ) {
            if !$crate::world::entity::projectile::arrow::AbstractArrow::is_in_ground(self) {
                $crate::world::entity::projectile::on_inside_bubble_column(self, drag_down);
            }
        }
        fn get_pierce_level(&self) -> i32 {
            i32::from($crate::world::entity::projectile::arrow::AbstractArrow::pierce_level(self))
        }
    };
}

/// `Entity.push(xa, ya, za)` (what `AbstractArrow.push` calls as super).
pub fn push_default(e: &mut dyn Entity, xa: f64, ya: f64, za: f64) {
    if xa.is_finite() && ya.is_finite() && za.is_finite() {
        let b = e.base_mut();
        let m = b.delta_movement();
        b.set_delta_movement(m.add(Vec3::new(xa, ya, za)));
        b.needs_sync = true;
    }
}
