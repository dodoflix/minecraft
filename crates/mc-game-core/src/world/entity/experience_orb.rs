//! `net.minecraft.world.entity.ExperienceOrb`.

use super::entity::{Entity, EntityData, MovementEmission};
use super::factory::{ENTITY_FACTORIES, EntityRegistration};
use super::{EntityId, MoverType, ServerPlayer, movement};
use crate::define_accessor;
use crate::server::level::ServerLevel;
use crate::world::level::BlockGetter;
use crate::world::level::LevelAccessor;
use crate::world::level::block::BlockStateExt;
use crate::world::level::collision::CollisionGetter;
use crate::world::level::material::FluidStateExt;
use linkme::distributed_slice;
use mc_core::phys::shapes::Shapes;
use mc_core::phys::{AABB, Vec3};
use mc_core::random::RandomSource;
use mc_data::EntityType;
use mc_nbt::CompoundTag;
use std::any::Any;

const LIFETIME: i32 = 6000;
const MAX_FOLLOW_DIST: f64 = 8.0;
const ORB_GROUPS_PER_AREA: i32 = 40;
const DEFAULT_HEALTH: i32 = 5;

define_accessor!(pub DATA_VALUE: Int = ExperienceOrb);

pub struct ExperienceOrb {
    base: EntityData,
    pub age: i32,
    pub health: i32,
    pub count: i32,
    /// `followingPlayer` (id of a player of the level).
    following_player: Option<EntityId>,
}

impl ExperienceOrb {
    /// `ExperienceOrb(type, level)`.
    pub fn new(ty: EntityType, id: EntityId) -> Self {
        let base = EntityData::new(ty, id, super::entity::TOTAL_AIR_SUPPLY, |b| {
            b.define(DATA_VALUE, 0);
        });
        Self {
            base,
            age: 0,
            health: DEFAULT_HEALTH,
            count: 1,
            following_player: None,
        }
    }

    /// `ExperienceOrb(level, x, y, z, value)`.
    pub fn at(level: &ServerLevel, x: f64, y: f64, z: f64, value: i32) -> Self {
        Self::with_direction(level, Vec3::new(x, y, z), Vec3::ZERO, value)
    }

    /// `ExperienceOrb(level, pos, roughly, value)`: RNG order = yaw, then the three motion components.
    pub fn with_direction(level: &ServerLevel, pos: Vec3, roughly: Vec3, value: i32) -> Self {
        let mut orb = Self::new(EntityType::EXPERIENCE_ORB, level.next_entity_id());
        orb.set_pos_vec(pos);
        let yaw = orb.base.random.next_float() * 360.0;
        orb.base.set_y_rot(yaw);
        let r = &mut orb.base.random;
        let mut random_movement = Vec3::new(
            (r.next_double() * 0.2 - 0.1) * 2.0,
            r.next_double() * 0.2 * 2.0,
            (r.next_double() * 0.2 - 0.1) * 2.0,
        );
        if roughly.length_sqr() > 0.0 && roughly.dot(random_movement) < 0.0 {
            random_movement = random_movement.scale(-1.0);
        }
        let size = orb.base.bounding_box().get_size();
        orb.set_pos_vec(pos.add(roughly.normalize().scale(size * 0.5)));
        orb.base.set_delta_movement(random_movement);
        if !level.no_collision(None, None, &orb.base.bounding_box()) {
            orb.unstuck_if_possible(level, size);
        }
        orb.set_value(value);
        orb
    }

    /// `unstuckIfPossible(maxDistance)`.
    fn unstuck_if_possible(&mut self, level: &ServerLevel, max_distance: f64) {
        let height = f64::from(self.base.get_bb_height());
        let width = f64::from(self.base.get_bb_width());
        let center = self.base.position().add(Vec3::new(0.0, height / 2.0, 0.0));
        let allowed_centers = Shapes::create(AABB::of_size(
            center,
            max_distance,
            max_distance,
            max_distance,
        ));
        let context = movement::collision_context(self);
        let border = level.world_border_ref();
        let free = level.find_free_position(
            Some(&context),
            &|aabb| border.is_none_or(|b| b.is_within_bounds_aabb(aabb)),
            &allowed_centers,
            center,
            width,
            height,
            width,
        );
        if let Some(pos) = free {
            self.set_pos_vec(pos.add(Vec3::new(0.0, -height / 2.0, 0.0)));
        }
    }

    pub fn get_value(&self) -> i32 {
        self.base.entity_data.get(DATA_VALUE)
    }

    fn set_value(&mut self, value: i32) {
        self.base.entity_data.set(DATA_VALUE, value);
    }

    /// `getIcon()` (client texture index; `ExperienceOrb` data only carries the value).
    pub fn get_icon(&self) -> i32 {
        let value = self.get_value();
        match value {
            2477.. => 10,
            1237.. => 9,
            617.. => 8,
            307.. => 7,
            149.. => 6,
            73.. => 5,
            37.. => 4,
            17.. => 3,
            7.. => 2,
            3.. => 1,
            _ => 0,
        }
    }

    /// `getExperienceValue(maxValue)`.
    pub fn get_experience_value(max_value: i32) -> i32 {
        match max_value {
            2477.. => 2477,
            1237.. => 1237,
            617.. => 617,
            307.. => 307,
            149.. => 149,
            73.. => 73,
            37.. => 37,
            17.. => 17,
            7.. => 7,
            3.. => 3,
            _ => 1,
        }
    }

    /// `award(level, pos, amount)`.
    pub fn award(level: &mut ServerLevel, pos: &Vec3, amount: i32) {
        Self::award_with_direction(level, pos, Vec3::ZERO, amount);
    }

    /// `awardWithDirection(level, pos, roughDirection, amount)`.
    pub fn award_with_direction(
        level: &mut ServerLevel,
        pos: &Vec3,
        rough_direction: Vec3,
        mut amount: i32,
    ) {
        while amount > 0 {
            let new_count = Self::get_experience_value(amount);
            amount -= new_count;
            if !Self::try_merge_to_existing(level, *pos, new_count) {
                let orb = Self::with_direction(level, *pos, rough_direction, new_count);
                level.add_fresh_entity(Box::new(orb));
            }
        }
    }

    /// `tryMergeToExisting(level, pos, value)`.
    fn try_merge_to_existing(level: &mut ServerLevel, pos: Vec3, value: i32) -> bool {
        let area = AABB::of_size(pos, 1.0, 1.0, 1.0);
        let id = level.random().next_int_bounded(ORB_GROUPS_PER_AREA);
        let orbs = level.get_entities_of_class(
            &area,
            |e| e.as_any().is::<ExperienceOrb>(),
            |e| {
                e.as_any()
                    .downcast_ref::<ExperienceOrb>()
                    .is_some_and(|o| can_merge_static(o, id, value))
            },
        );
        let Some(&first) = orbs.first() else {
            return false;
        };
        level.with_entity(first, |_, e| {
            if let Some(orb) = e.as_any_mut().downcast_mut::<ExperienceOrb>() {
                orb.count += 1;
                orb.age = 0;
            }
        });
        true
    }

    /// `canMerge(orb)`: `orb != this` holds because this orb is taken out of the level while it ticks.
    fn can_merge(&self, orb: &ExperienceOrb) -> bool {
        can_merge_static(orb, self.base.id, self.get_value())
    }

    /// `merge(orb)`.
    fn merge(&mut self, orb: &mut ExperienceOrb) {
        self.count += orb.count;
        self.age = self.age.min(orb.age);
        orb.discard();
    }

    fn scan_for_merges(&mut self, level: &mut ServerLevel) {
        let area = self.base.bounding_box().inflate_scalar(0.5);
        let orbs = level.get_entities_of_class(
            &area,
            |e| e.as_any().is::<ExperienceOrb>(),
            |e| {
                e.as_any()
                    .downcast_ref::<ExperienceOrb>()
                    .is_some_and(|o| self.can_merge(o))
            },
        );
        for id in orbs {
            level.with_entity(id, |_, e| {
                if let Some(orb) = e.as_any_mut().downcast_mut::<ExperienceOrb>() {
                    self.merge(orb);
                }
            });
        }
    }

    /// `followNearbyPlayer()`.
    fn follow_nearby_player(&mut self, level: &ServerLevel) {
        let following_valid = self
            .following_player
            .and_then(|id| level.get_entity(id))
            .is_some_and(|p| {
                !p.is_spectator() && p.base().distance_to_sqr_vec(self.base.position()) <= 64.0
            });
        if !following_valid {
            let pos = self.base.position();
            let nearest = level
                .get_nearest_player(pos.x, pos.y, pos.z, MAX_FOLLOW_DIST, |p| !p.is_spectator());
            self.following_player = nearest.filter(|&id| {
                level.get_entity(id).is_some_and(|p| {
                    !p.is_spectator() && !p.living().is_some_and(|l| l.get_health() <= 0.0)
                })
            });
        }
        if let Some(player) = self.following_player.and_then(|id| level.get_entity(id)) {
            let (pp, here) = (player.base().position(), self.base.position());
            let delta = Vec3::new(
                pp.x - here.x,
                pp.y + f64::from(player.base().eye_height) / 2.0 - here.y,
                pp.z - here.z,
            );
            let length = delta.length_sqr();
            let power = 1.0 - length.sqrt() / 8.0;
            let dm = self.base.delta_movement();
            self.base
                .set_delta_movement(dm.add(delta.normalize().scale(power * power * 0.1)));
        }
    }

    /// `setUnderwaterMovement()`.
    fn set_underwater_movement(&mut self) {
        let m = self.base.delta_movement();
        self.base.set_delta_movement_xyz(
            m.x * f64::from(0.99f32),
            (m.y + f64::from(5.0E-4f32)).min(f64::from(0.06f32)),
            m.z * f64::from(0.99f32),
        );
    }

    /// `repairPlayerItems(player, amount)`: mending. Returns the experience that is left over.
    fn repair_player_items(
        &self,
        level: &mut ServerLevel,
        player: &mut ServerPlayer,
        amount: i32,
    ) -> i32 {
        super::player::pickup::repair_player_items(level, player, amount)
    }
}

fn can_merge_static(orb: &ExperienceOrb, id: i32, value: i32) -> bool {
    !orb.base.is_removed()
        && orb.base.id.wrapping_sub(id) % ORB_GROUPS_PER_AREA == 0
        && orb.get_value() == value
}

impl Entity for ExperienceOrb {
    fn base(&self) -> &EntityData {
        &self.base
    }
    fn base_mut(&mut self) -> &mut EntityData {
        &mut self.base
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
    fn get_movement_emission(&self) -> MovementEmission {
        MovementEmission::None
    }
    fn get_default_gravity(&self) -> f64 {
        0.03
    }
    fn get_air_drag(&self) -> f32 {
        0.98
    }
    fn is_attackable(&self) -> bool {
        false
    }
    fn block_pos_below_offset(&self) -> f32 {
        0.999999
    }

    /// `doWaterSplashEffect()`: orbs make no splash.
    fn do_water_splash_effect(&mut self, _level: &mut ServerLevel) {}

    /// `hurtServer(level, source, damage)`: health 5, no extra immunity.
    fn hurt_server(
        &mut self,
        level: &mut ServerLevel,
        source: &crate::world::damagesource::DamageSource,
        damage: f32,
    ) -> bool {
        if self.is_invulnerable_to_base(level, source) {
            return false;
        }
        self.mark_hurt();
        self.health = (self.health as f32 - damage) as i32;
        if self.health <= 0 {
            self.discard();
        }
        true
    }

    fn tick(&mut self, level: &mut ServerLevel) {
        self.base_tick(level);
        let colliding = !level.no_collision(None, None, &self.base.bounding_box());
        if self.is_eye_in_fluid("water") {
            self.set_underwater_movement();
        } else if !colliding {
            self.apply_gravity();
        }
        if level
            .get_fluid_state(self.base.block_position())
            .is_tag("lava")
        {
            let r = &mut self.base.random;
            let x = f64::from((r.next_float() - r.next_float()) * 0.2f32);
            let z = f64::from((r.next_float() - r.next_float()) * 0.2f32);
            self.base.set_delta_movement_xyz(x, f64::from(0.2f32), z);
        }
        if self.base.tick_count % 20 == 1 {
            self.scan_for_merges(level);
        }
        self.follow_nearby_player(level);
        if self.following_player.is_none() && colliding {
            let next_colliding = !level.no_collision(
                None,
                None,
                &self
                    .base
                    .bounding_box()
                    .move_vec(self.base.delta_movement()),
            );
            if next_colliding {
                let bb = self.base.bounding_box();
                let p = self.base.position();
                super::item::move_towards_closest_space(
                    self,
                    level,
                    p.x,
                    (bb.min_y + bb.max_y) / 2.0,
                    p.z,
                );
                self.base.needs_sync = true;
            }
        }
        let fall_speed = self.base.delta_movement().y;
        let dm = self.base.delta_movement();
        self.r#move(level, MoverType::SelfMover, dm);
        self.apply_effects_from_blocks(level);
        let mut friction = self.get_air_drag();
        if self.base.on_ground() {
            let below = movement::get_block_pos_below_that_affects_my_movement(self, level);
            friction *= level.get_block_state(below).block().data().friction;
        }
        let m = self.base.delta_movement();
        self.base.set_delta_movement(m.scale(f64::from(friction)));
        if self.base.vertical_collision_below && fall_speed < -self.get_gravity() {
            let m = self.base.delta_movement();
            self.base
                .set_delta_movement_xyz(m.x, -fall_speed * 0.4, m.z);
        }
        self.age += 1;
        if self.age >= LIFETIME {
            self.discard();
        }
    }

    fn player_touch(&mut self, level: &mut ServerLevel, player: &mut ServerPlayer) {
        if player.player.take_xp_delay == 0 {
            player.player.take_xp_delay = 2;
            player.take(level, self, 1);
            let remaining = self.repair_player_items(level, player, self.get_value());
            if remaining > 0 {
                player.give_experience_points(level, remaining);
            }
            self.count -= 1;
            if self.count == 0 {
                self.discard();
            }
        }
    }

    fn add_additional_save_data(&self, out: &mut CompoundTag) {
        out.put_short("Health", self.health as i16);
        out.put_short("Age", self.age as i16);
        out.put_short("Value", self.get_value() as i16);
        out.put_int("Count", self.count);
    }

    fn read_additional_save_data(&mut self, input: &CompoundTag) {
        self.health = i32::from(input.get_short_or("Health", 5));
        self.age = i32::from(input.get_short_or("Age", 0));
        self.set_value(i32::from(input.get_short_or("Value", 0)));
        // `ExtraCodecs.POSITIVE_INT`: a non-positive or non-int count falls back to 1
        self.count = input.get_int("Count").filter(|&c| c > 0).unwrap_or(1);
    }
}

#[distributed_slice(ENTITY_FACTORIES)]
static EXPERIENCE_ORB_FACTORY: EntityRegistration = EntityRegistration {
    entity_type: EntityType::EXPERIENCE_ORB,
    create: |ty, id, _registries| Box::new(ExperienceOrb::new(ty, id)),
};
