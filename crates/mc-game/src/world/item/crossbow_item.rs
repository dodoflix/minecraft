//! `net.minecraft.world.item.CrossbowItem`.

use super::projectile_weapon_item::{ProjectileWeapon, create_arrow_projectile, draw, get_projectile, is_arrow, is_arrow_or_firework, shoot};
use super::{APPROXIMATELY_INFINITE_USE_DURATION, InteractionHand, InteractionResult, ItemBehaviour, ItemStack, ItemStackExt as _, holder_sound};
use crate::register_item_class;
use crate::server::level::ServerLevel;
use crate::stats::Stat;
use crate::world::entity::player::ServerPlayer;
use crate::world::entity::projectile::Projectile;
use crate::world::entity::projectile::arrow::AbstractArrow;
use crate::world::entity::{Entity, EntityId, entity};
use crate::world::item::enchantment_helper as helper;
use crate::world::level::LevelAccessor as _;
use mc_core::math::mth;
use mc_core::phys::Vec3;
use mc_core::random::RandomSource;
use mc_data::{Item, SoundEvent};
use mc_protocol::components::{CHARGED_PROJECTILES, ChargedProjectiles, ItemStackTemplate, ItemUseAnimation};
use mc_protocol::packets::play::SoundSource;
use std::sync::atomic::{AtomicBool, Ordering};

const MAX_CHARGE_DURATION: f32 = 1.25;
pub const DEFAULT_RANGE: i32 = 8;
const START_SOUND_PERCENT: f32 = 0.2;
const MID_SOUND_PERCENT: f32 = 0.5;
const ARROW_POWER: f32 = 3.15;
const FIREWORK_POWER: f32 = 1.6;
pub const MOB_ARROW_POWER: f32 = 1.6;

/// The singleton `CrossbowItem` carries the loading-sound state in instance fields (shared by every user).
pub struct CrossbowItem {
    item: Item,
    start_sound_played: AtomicBool,
    mid_load_sound_played: AtomicBool,
}

/// `CrossbowItem.isCharged(itemStack)`.
pub fn is_charged(stack: &ItemStack) -> bool {
    stack.get(&CHARGED_PROJECTILES).is_some_and(|c| !c.items.is_empty())
}

/// `CrossbowItem.getChargeDuration(crossbow, user)`.
pub fn get_charge_duration(level: &ServerLevel, crossbow: &ItemStack, user: &mut dyn Entity) -> i32 {
    let duration = helper::modify_crossbow_charging_time(&level.registries, crossbow, user, MAX_CHARGE_DURATION);
    mth::floor_f32(duration * 20.0)
}

/// `getPowerForTime(timeHeld, itemStack, holder)`.
fn get_power_for_time(level: &ServerLevel, time_held: i32, crossbow: &ItemStack, holder: &mut dyn Entity) -> f32 {
    let mut pow = time_held as f32 / get_charge_duration(level, crossbow, holder) as f32;
    if pow > 1.0 {
        pow = 1.0;
    }
    pow
}

/// `getShootingPower(projectiles)`.
fn get_shooting_power(projectiles: &ChargedProjectiles) -> f32 {
    if projectiles.items.iter().any(|t| t.item == Item::FIREWORK_ROCKET) { FIREWORK_POWER } else { ARROW_POWER }
}

/// `getShotPitch(random, index)`.
fn get_shot_pitch(random: &mut dyn RandomSource, index: i32) -> f32 {
    if index == 0 { 1.0 } else { get_random_shot_pitch((index & 1) == 1, random) }
}

fn get_random_shot_pitch(high_pitch: bool, random: &mut dyn RandomSource) -> f32 {
    let range_decider = if high_pitch { 0.63 } else { 0.43 };
    1.0 / (random.next_float() * 0.5 + 1.8) + range_decider
}

// ---- the JOML subset `getProjectileShotVector` uses (joml 1.10.9, `joml.useMathFma` off: `fma(a, b, c)` = `a * b + c`) ----

#[derive(Clone, Copy)]
struct V3 {
    x: f32,
    y: f32,
    z: f32,
}

impl V3 {
    fn of(v: Vec3) -> V3 {
        V3 { x: v.x as f32, y: v.y as f32, z: v.z as f32 }
    }

    /// `normalize()`.
    fn normalize(self) -> V3 {
        let scalar = 1.0 / (self.x * self.x + (self.y * self.y + self.z * self.z)).sqrt();
        V3 { x: self.x * scalar, y: self.y * scalar, z: self.z * scalar }
    }

    /// `cross(v)`.
    fn cross(self, v: V3) -> V3 {
        V3 { x: self.y * v.z + -self.z * v.y, y: self.z * v.x + -self.x * v.z, z: self.x * v.y + -self.y * v.x }
    }

    fn length_squared(self) -> f32 {
        self.x * self.x + (self.y * self.y + self.z * self.z)
    }

    /// `rotate(Quaternionf)` = `Quaternionf.transform`.
    fn rotate(self, q: [f32; 4]) -> V3 {
        let [qx, qy, qz, qw] = q;
        let (xx, yy, zz, ww) = (qx * qx, qy * qy, qz * qz, qw * qw);
        let (xy, xz, yz, xw, zw, yw) = (qx * qy, qx * qz, qy * qz, qx * qw, qz * qw, qy * qw);
        let k = 1.0 / (xx + yy + zz + ww);
        let (x, y, z) = (self.x, self.y, self.z);
        V3 {
            x: (xx - yy - zz + ww) * k * x + (2.0 * (xy - zw) * k * y + 2.0 * (xz + yw) * k * z),
            y: 2.0 * (xy + zw) * k * x + ((yy - xx - zz + ww) * k * y + 2.0 * (yz - xw) * k * z),
            z: 2.0 * (xz - yw) * k * x + (2.0 * (yz + xw) * k * y + (zz - xx - yy + ww) * k * z),
        }
    }

    /// `rotateAxis(angle, aX, aY, aZ)`.
    fn rotate_axis(self, angle: f32, ax: f32, ay: f32, az: f32) -> V3 {
        let abs_one = |r: f32| (r.to_bits() & 0x7FFF_FFFF) == 1065353216;
        if ay == 0.0 && az == 0.0 && abs_one(ax) {
            let angle = ax * angle;
            let sin = (f64::from(angle).sin()) as f32;
            let cos = cos_from_sin_f32(sin, angle);
            V3 { x: self.x, y: self.y * cos - self.z * sin, z: self.y * sin + self.z * cos }
        } else if ax == 0.0 && az == 0.0 && abs_one(ay) {
            let angle = ay * angle;
            let sin = (f64::from(angle).sin()) as f32;
            let cos = cos_from_sin_f32(sin, angle);
            V3 { x: self.x * cos + self.z * sin, y: self.y, z: -self.x * sin + self.z * cos }
        } else if ax == 0.0 && ay == 0.0 && abs_one(az) {
            let angle = az * angle;
            let sin = (f64::from(angle).sin()) as f32;
            let cos = cos_from_sin_f32(sin, angle);
            V3 { x: self.x * cos - self.y * sin, y: self.x * sin + self.y * cos, z: self.z }
        } else {
            let hangle = angle * 0.5;
            let sin_angle = (f64::from(hangle).sin()) as f32;
            let (qx, qy, qz) = (ax * sin_angle, ay * sin_angle, az * sin_angle);
            let qw = cos_from_sin_f32(sin_angle, hangle);
            let (w2, x2, y2, z2) = (qw * qw, qx * qx, qy * qy, qz * qz);
            let (zw, xy, xz, yw, yz, xw) = (qz * qw, qx * qy, qx * qz, qy * qw, qy * qz, qx * qw);
            let (x, y, z) = (self.x, self.y, self.z);
            V3 {
                x: (w2 + x2 - z2 - y2) * x + (-zw + xy - zw + xy) * y + (yw + xz + xz + yw) * z,
                y: (xy + zw + zw + xy) * x + (y2 - z2 + w2 - x2) * y + (yz + yz - xw - xw) * z,
                z: (xz - yw + xz - yw) * x + (yz + yz + xw + xw) * y + (z2 - y2 - x2 + w2) * z,
            }
        }
    }
}

/// `org.joml.Math.cosFromSin(float, float)` (`FASTMATH` off).
fn cos_from_sin_f32(sin: f32, angle: f32) -> f32 {
    let pi_over_2 = (std::f64::consts::PI / 2.0) as f32;
    let two_pi = (std::f64::consts::PI * 2.0) as f32;
    let cos = (1.0 - sin * sin).sqrt();
    let a = angle + pi_over_2;
    let mut b = a - ((a / two_pi) as i32) as f32 * two_pi;
    if f64::from(b) < 0.0 {
        b += two_pi;
    }
    if b >= std::f64::consts::PI as f32 { -cos } else { cos }
}

/// `org.joml.Math.cosFromSin(double, double)` (`FASTMATH` off).
fn cos_from_sin_f64(sin: f64, angle: f64) -> f64 {
    let cos = (1.0 - sin * sin).sqrt();
    let a = angle + std::f64::consts::PI / 2.0;
    let mut b = a - f64::from((a / (std::f64::consts::PI * 2.0)) as i32) * (std::f64::consts::PI * 2.0);
    if b < 0.0 {
        b += std::f64::consts::PI * 2.0;
    }
    if b >= std::f64::consts::PI { -cos } else { cos }
}

/// `new Quaternionf().setAngleAxis(angle, x, y, z)` as `[x, y, z, w]`.
fn quaternion_from_angle_axis(angle: f64, x: f64, y: f64, z: f64) -> [f32; 4] {
    let s = (angle * 0.5).sin();
    [(x * s) as f32, (y * s) as f32, (z * s) as f32, cos_from_sin_f64(s, angle * 0.5) as f32]
}

/// `getProjectileShotVector(body, originalVector, angle)`.
fn get_projectile_shot_vector(body: &dyn Entity, original_vector: Vec3, angle: f32) -> V3 {
    let view_vec = V3::of(original_vector).normalize();
    let mut right_vector_pre_rot = view_vec.cross(V3 { x: 0.0, y: 1.0, z: 0.0 });
    if f64::from(right_vector_pre_rot.length_squared()) <= 1.0E-7 {
        let up = get_up_vector(body);
        right_vector_pre_rot = view_vec.cross(V3::of(up));
    }
    let view_vec_3f = view_vec.rotate_axis((std::f64::consts::PI / 2.0) as f32, right_vector_pre_rot.x, right_vector_pre_rot.y, right_vector_pre_rot.z);
    view_vec.rotate_axis(angle * (std::f64::consts::PI / 180.0) as f32, view_vec_3f.x, view_vec_3f.y, view_vec_3f.z)
}

/// `getUpVector(1.0F)` = `calculateUpVector(getViewXRot(1), getViewYRot(1))`.
fn get_up_vector(e: &dyn Entity) -> Vec3 {
    entity::calculate_view_vector(e.get_view_x_rot(1.0) - 90.0, e.get_view_y_rot(1.0))
}

impl ProjectileWeapon for CrossbowItem {
    fn supported_held_projectiles(&self, stack: &ItemStack) -> bool {
        is_arrow_or_firework(stack)
    }

    fn all_supported_projectiles(&self, stack: &ItemStack) -> bool {
        is_arrow(stack)
    }

    fn get_durability_use(&self, projectile: &ItemStack) -> i32 {
        if projectile.item() == Item::FIREWORK_ROCKET { 3 } else { 1 }
    }

    fn shoot_projectile(&self, level: &mut ServerLevel, shooter: &mut dyn Entity, projectile: &mut dyn Projectile, index: i32, power: f32, uncertainty: f32, angle: f32, target_override: Option<EntityId>) {
        let shot_vector = match target_override.and_then(|id| level.get_entity(id)) {
            Some(target) => {
                let xd = target.base().x() - shooter.base().x();
                let zd = target.base().z() - shooter.base().z();
                let distance_to_target = (xd * xd + zd * zd).sqrt();
                let yd = target.base().get_y_progress(0.3333333333333333) - projectile.base().y() + distance_to_target * f64::from(0.2f32);
                get_projectile_shot_vector(&*shooter, Vec3::new(xd, yd, zd), angle)
            }
            None => {
                let up_vector = get_up_vector(&*shooter);
                let up_quaternion = quaternion_from_angle_axis(f64::from(angle * (std::f64::consts::PI / 180.0) as f32), up_vector.x, up_vector.y, up_vector.z);
                let view_vec = shooter.get_view_vector(1.0);
                V3::of(view_vec).rotate(up_quaternion)
            }
        };
        projectile.shoot(f64::from(shot_vector.x), f64::from(shot_vector.y), f64::from(shot_vector.z), power, uncertainty);
        let sound_pitch = get_shot_pitch(&mut shooter.base_mut().random, index);
        let p = shooter.position();
        let source = shooter.get_sound_source();
        level.play_sound_at(None, p.x, p.y, p.z, SoundEvent::ITEM_CROSSBOW_SHOOT, source, 1.0, sound_pitch);
    }

    fn create_projectile(&self, level: &mut ServerLevel, shooter: &dyn Entity, held_item: &ItemStack, projectile: &ItemStack, is_crit: bool) -> Box<dyn Entity> {
        if projectile.item() == Item::FIREWORK_ROCKET {
            // `new FireworkRocketEntity(level, projectile, shooter, x, shooter.getEyeY() - 0.15F, z, true)`
            let (x, z) = (shooter.base().x(), shooter.base().z());
            let y = shooter.base().get_eye_y() - f64::from(0.15f32);
            crate::world::entity::projectile::firework_rocket::FireworkRocketEntity::new_shot_by(level, projectile, shooter, x, y, z, true)
        } else {
            let mut entity = create_arrow_projectile(level, shooter, held_item, projectile, is_crit);
            set_crossbow_hit_sound(&mut *entity);
            entity
        }
    }
}

/// `arrow.setSoundEvent(SoundEvents.CROSSBOW_HIT)` for an `AbstractArrow`.
fn set_crossbow_hit_sound(entity: &mut dyn Entity) {
    use crate::world::entity::projectile::arrow::{Arrow, SpectralArrow, ThrownTrident};
    let any = entity.as_any_mut();
    if let Some(a) = any.downcast_mut::<Arrow>() {
        a.set_sound_event(SoundEvent::ITEM_CROSSBOW_HIT);
    } else if let Some(a) = any.downcast_mut::<SpectralArrow>() {
        a.set_sound_event(SoundEvent::ITEM_CROSSBOW_HIT);
    } else if let Some(a) = any.downcast_mut::<ThrownTrident>() {
        a.set_sound_event(SoundEvent::ITEM_CROSSBOW_HIT);
    }
}

impl CrossbowItem {
    // ==== [Piglins] additions ====
    /// The `CrossbowItem` singleton for callers without an item registry handle (mobs shooting their crossbow: only the
    /// loading-sound flags are instance state, `performShooting` does not touch them).
    pub fn singleton() -> Self {
        CrossbowItem { item: Item::CROSSBOW, start_sound_played: AtomicBool::new(false), mid_load_sound_played: AtomicBool::new(false) }
    }

    /// `performShooting(level, shooter, hand, weapon, power, uncertainty, targetOverride)`.
    pub fn perform_shooting(&self, level: &mut ServerLevel, shooter: &mut dyn Entity, hand: InteractionHand, weapon: &mut ItemStack, power: f32, uncertainty: f32, target_override: Option<EntityId>) {
        let charged = weapon.set(CHARGED_PROJECTILES, ChargedProjectiles::default()).and_then(|v| v.downcast_ref::<ChargedProjectiles>().cloned());
        if let Some(charged) = charged
            && !charged.items.is_empty()
        {
            let projectiles: Vec<ItemStack> = charged.items.iter().map(ItemStackTemplate::create).collect();
            let is_player = shooter.is_player();
            shoot(self, level, shooter, hand, weapon, &projectiles, power, uncertainty, is_player, target_override);
            if let Some(player) = shooter.as_any_mut().downcast_mut::<ServerPlayer>() {
                // CriteriaTriggers::SHOT_CROSSBOW.trigger(player, weapon)
                player.award_stat_once(level, Stat::Used(weapon.item()));
            }
        }
    }

    /// `tryLoadProjectiles(shooter, heldItem)`.
    fn try_load_projectiles(&self, level: &mut ServerLevel, shooter: &mut dyn Entity, held_item: &mut ItemStack) -> bool {
        let mut source = get_projectile(&*shooter, self);
        let drawn = draw(level, held_item, &mut source.stack, shooter);
        source.write_back(shooter);
        if !drawn.is_empty() {
            let templates: Vec<ItemStackTemplate> = drawn.iter().filter_map(|s| ItemStackTemplate::from_stack(s).ok()).collect();
            held_item.set(CHARGED_PROJECTILES, ChargedProjectiles { items: templates });
            true
        } else {
            false
        }
    }
}

impl ItemBehaviour for CrossbowItem {
    fn use_item(&self, level: &mut ServerLevel, player: &mut ServerPlayer, hand: InteractionHand) -> InteractionResult {
        let mut stack = player.get_item_in_hand(hand);
        let charged = stack.get(&CHARGED_PROJECTILES).cloned();
        if let Some(charged) = charged
            && !charged.items.is_empty()
        {
            let power = get_shooting_power(&charged);
            self.perform_shooting(level, player, hand, &mut stack, power, 1.0, None);
            player.set_item_in_hand(hand, stack);
            InteractionResult::CONSUME
        } else if !get_projectile(player, self).stack.is_empty() {
            self.start_sound_played.store(false, Ordering::Relaxed);
            self.mid_load_sound_played.store(false, Ordering::Relaxed);
            player.start_using_item(level, hand);
            InteractionResult::CONSUME
        } else {
            InteractionResult::FAIL
        }
    }

    fn release_using(&self, stack: &mut ItemStack, level: &mut ServerLevel, entity: &mut dyn Entity, remaining_time: i32) -> bool {
        let time_held = self.get_use_duration(stack) - remaining_time;
        get_power_for_time(level, time_held, stack, entity) >= 1.0 && is_charged(stack)
    }

    fn on_use_tick(&self, level: &mut ServerLevel, entity: &mut dyn Entity, stack: &mut ItemStack, ticks_remaining: i32) {
        let sounds = helper::pick_crossbow_charging_sounds(&level.registries, stack).cloned().unwrap_or_else(default_sounds);
        let tick_percent = (self.get_use_duration(stack) - ticks_remaining) as f32 / get_charge_duration(level, stack, entity) as f32;
        let play = |level: &mut ServerLevel, entity: &dyn Entity, holder: &Option<mc_protocol::components::Holder<mc_protocol::components::SoundEvent>>, source, volume, pitch: f32| {
            if let Some(sound) = holder.as_ref().and_then(holder_sound) {
                let p = entity.position();
                level.play_sound_at(None, p.x, p.y, p.z, sound, source, volume, pitch);
            }
        };
        if tick_percent < START_SOUND_PERCENT {
            self.start_sound_played.store(false, Ordering::Relaxed);
            self.mid_load_sound_played.store(false, Ordering::Relaxed);
        }
        if tick_percent >= START_SOUND_PERCENT && !self.start_sound_played.load(Ordering::Relaxed) {
            self.start_sound_played.store(true, Ordering::Relaxed);
            play(level, &*entity, &sounds.start, SoundSource::Players, 0.5, 1.0);
        }
        if tick_percent >= MID_SOUND_PERCENT && !self.mid_load_sound_played.load(Ordering::Relaxed) {
            self.mid_load_sound_played.store(true, Ordering::Relaxed);
            play(level, &*entity, &sounds.mid, SoundSource::Players, 0.5, 1.0);
        }
        if tick_percent >= 1.0 && !is_charged(stack) && self.try_load_projectiles(level, entity, stack) {
            let pitch = 1.0 / (level.random().next_float() * 0.5 + 1.0) + 0.2;
            let source = entity.get_sound_source();
            play(level, &*entity, &sounds.end, source, 1.0, pitch);
        }
    }

    fn get_use_duration(&self, _stack: &ItemStack) -> i32 {
        APPROXIMATELY_INFINITE_USE_DURATION
    }

    fn get_use_animation(&self, _stack: &ItemStack) -> ItemUseAnimation {
        ItemUseAnimation::Crossbow
    }

    fn use_on_release(&self, stack: &ItemStack) -> bool {
        stack.item() == self.item
    }
}

/// `CrossbowItem.DEFAULT_SOUNDS`.
fn default_sounds() -> crate::world::item::enchantment::enchantment_effect_components::ChargingSounds {
    use mc_protocol::components::Holder;
    let reference = |s: SoundEvent| Some(Holder::Reference(s.identifier()));
    crate::world::item::enchantment::enchantment_effect_components::ChargingSounds {
        start: reference(SoundEvent::ITEM_CROSSBOW_LOADING_START),
        mid: reference(SoundEvent::ITEM_CROSSBOW_LOADING_MIDDLE),
        end: reference(SoundEvent::ITEM_CROSSBOW_LOADING_END),
    }
}

register_item_class!("CrossbowItem", |item| Box::new(CrossbowItem { item, start_sound_played: AtomicBool::new(false), mid_load_sound_played: AtomicBool::new(false) }));
