//! `net.minecraft.world.entity.projectile.FishingHook`: the bobber of a fishing rod. It flies, hooks the first entity it
//! touches (live item entities included) or settles in water, where the bite timeline (`timeUntilLured`, `timeUntilHooked`,
//! `nibble`) runs until the owner reels it in.
//!
//! The timeline draws from the hook's own random, the dive of a bite from `syncronizedRandom`, which is reseeded every
//! tick from the hook's UUID and the game time.
//!
//! Not implemented: the hook of a player is not deregistered from `Player.fishing` when it is removed from outside its
//! own tick or retrieval (an unloaded chunk keeps the stale reference vanilla keeps and reels in); readers treat an id
//! without a live hook as no hook. The hooked entity is kept by id: one that left the level before the reel-in is not
//! pulled and counts as no item entity (vanilla keeps the object).

use super::{
    EntityHitResult, Projectile, ProjectileData, RAD_TO_DEG, hit_target_or_deflect_self,
    on_hit_block_base, update_rotation, util,
};
use crate::advancements::triggers::CriteriaTriggers;
use crate::define_accessor;
use crate::impl_projectile_entity;
use crate::impl_throwable_projectile;
use crate::server::level::ServerLevel;
use crate::stats::stats::Stats;
use crate::world::entity::entity::TOTAL_AIR_SUPPLY;
use crate::world::entity::equipment::EquipmentSlot;
use crate::world::entity::experience_orb::ExperienceOrb;
use crate::world::entity::factory::{ENTITY_FACTORIES, EntityRegistration};
use crate::world::entity::item::ItemEntity;
use crate::world::entity::scenario_dump::{SCENARIO_DUMP_EXTRA, ScenarioDumpExtra};
use crate::world::entity::{
    Entity, EntityData, EntityId, MovementEmission, MoverType, ServerPlayer,
};
use crate::world::item::ItemStack;
use crate::world::level::block::BlockStateExt;
use crate::world::level::material::FluidStateExt;
use crate::world::level::storage::loot::context::LootParams;
use crate::world::level::storage::loot::loading::get_loot_table;
use crate::world::level::storage::loot::parameters::LootContextParamSets;
use crate::world::level::{BlockGetter, LevelReader};
use linkme::distributed_slice;
use mc_core::ident::Identifier;
use mc_core::math::java::java_float_to_string;
use mc_core::math::mth;
use mc_core::phys::{BlockHitResult, Vec3};
use mc_core::pos::BlockPos;
use mc_core::random::{LegacyRandomSource, RandomSource};
use mc_data::block::Block;
use mc_data::{Attribute, EntityType, Item, ParticleType, SoundEvent};
use mc_nbt::CompoundTag;
use mc_protocol::packets::play::ParticleOptions;

define_accessor!(pub DATA_HOOKED_ENTITY: Int = FishingHook);
define_accessor!(pub DATA_BITING: Boolean = FishingHook);

/// `(float) (Math.PI / 180.0)`.
const DEG_TO_RAD: f32 = (std::f64::consts::PI / 180.0) as f32;
const MAX_OUT_OF_WATER_TIME: i32 = 10;

/// `FishingHook.FishHookState`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum FishHookState {
    Flying,
    HookedInEntity,
    Bobbing,
}

/// `FishingHook.OpenWaterType`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum OpenWaterType {
    AboveWater,
    InsideWater,
    Invalid,
}

pub struct FishingHook {
    base: EntityData,
    projectile: ProjectileData,
    syncronized_random: LegacyRandomSource,
    biting: bool,
    out_of_water_time: i32,
    life: i32,
    nibble: i32,
    time_until_lured: i32,
    time_until_hooked: i32,
    fish_angle: f32,
    open_water: bool,
    hooked_in: Option<EntityId>,
    current_state: FishHookState,
    luck: i32,
    lure_speed: i32,
}

impl FishingHook {
    /// `FishingHook(type, level, luck, lureSpeed)`.
    pub fn new(ty: EntityType, id: EntityId, luck: i32, lure_speed: i32) -> Self {
        let base = EntityData::new(ty, id, TOTAL_AIR_SUPPLY, |b| {
            b.define(DATA_HOOKED_ENTITY, 0);
            b.define(DATA_BITING, false);
        });
        Self {
            base,
            projectile: ProjectileData::default(),
            syncronized_random: mc_core::random::create_new(),
            biting: false,
            out_of_water_time: 0,
            life: 0,
            nibble: 0,
            time_until_lured: 0,
            time_until_hooked: 0,
            fish_angle: 0.0,
            open_water: true,
            hooked_in: None,
            current_state: FishHookState::Flying,
            luck: luck.max(0),
            lure_speed: lure_speed.max(0),
        }
    }

    /// `FishingHook(player, level, luck, lureSpeed)`: placed in front of the player's eyes and thrown along the view.
    pub fn with_owner(
        level: &ServerLevel,
        player: &dyn Entity,
        luck: i32,
        lure_speed: i32,
    ) -> Self {
        let mut hook = Self::new(
            EntityType::FISHING_BOBBER,
            level.next_entity_id(),
            luck,
            lure_speed,
        );
        hook.set_owner(Some(player));
        let (x_rot, y_rot) = (player.base().x_rot(), player.base().y_rot());
        let deg = DEG_TO_RAD;
        let pi = std::f32::consts::PI;
        let y_cos = mth::cos(f64::from(-y_rot * deg - pi));
        let y_sin = mth::sin(f64::from(-y_rot * deg - pi));
        let x_cos = -mth::cos(f64::from(-x_rot * deg));
        let x_sin = mth::sin(f64::from(-x_rot * deg));
        let x = player.base().x() - f64::from(y_sin) * 0.3;
        let y = player.base().get_eye_y();
        let z = player.base().z() - f64::from(y_cos) * 0.3;
        hook.snap_to(x, y, z, y_rot, x_rot);
        let movement = Vec3::new(
            f64::from(-y_sin),
            f64::from(mth::clamp_f32(-(x_sin / x_cos), -5.0, 5.0)),
            f64::from(-y_cos),
        );
        let dist = movement.length();
        let random = &mut hook.base.random;
        let (a, b, c) = (
            0.6 / dist + random.triangle(0.5, 0.0103365),
            0.6 / dist + random.triangle(0.5, 0.0103365),
            0.6 / dist + random.triangle(0.5, 0.0103365),
        );
        let movement = movement.multiply_xyz(a, b, c);
        hook.base.set_delta_movement(movement);
        hook.base
            .set_y_rot((mth::atan2(movement.x, movement.z) * RAD_TO_DEG) as f32);
        hook.base.set_x_rot(
            (mth::atan2(movement.y, movement.horizontal_distance()) * RAD_TO_DEG) as f32,
        );
        hook.base.y_rot_o = hook.base.y_rot();
        hook.base.x_rot_o = hook.base.x_rot();
        hook
    }

    /// `setHookedEntity(hookedIn)`.
    fn set_hooked_entity(&mut self, hooked_in: Option<EntityId>) {
        self.hooked_in = hooked_in;
        self.base
            .entity_data
            .set(DATA_HOOKED_ENTITY, hooked_in.map_or(0, |id| id + 1));
    }

    /// `getEntityData().set(DATA_BITING, biting)` and the `onSyncedDataUpdated` that follows a change: a bite pulls the
    /// bobber down.
    fn set_biting(&mut self, biting: bool) {
        if self.base.entity_data.set(DATA_BITING, biting) {
            self.biting = biting;
            if biting {
                let y = -0.4f32 * mth::next_float(&mut self.syncronized_random, 0.6, 1.0);
                let m = self.base.delta_movement();
                self.base.set_delta_movement_xyz(m.x, f64::from(y), m.z);
            }
        }
    }

    /// Whether the owner can interact with the level, holds a fishing rod in either hand and is within 32 blocks (the
    /// negation of `shouldStopFishing`'s discard).
    fn owner_allows_fishing(&self, owner: &dyn Entity) -> bool {
        owner.can_interact_with_level()
            && (owner
                .get_item_by_slot(EquipmentSlot::MainHand)
                .is(Item::FISHING_ROD)
                || owner
                    .get_item_by_slot(EquipmentSlot::OffHand)
                    .is(Item::FISHING_ROD))
            && self.base.distance_to_sqr_vec(owner.position()) <= 1024.0
    }

    /// `shouldStopFishing(owner)`: discards the hook unless the owner may keep fishing.
    fn should_stop_fishing(&mut self, owner: &dyn Entity) -> bool {
        if self.owner_allows_fishing(owner) {
            return false;
        }
        self.discard();
        true
    }

    /// `discard()` inside the hook's own tick: watchers get the removal before anything else the tick sends.
    fn stop(&mut self, level: &mut ServerLevel) {
        self.discard();
        level.untrack_discarded_entity(self);
    }

    /// `catchingFish(blockPos)`.
    fn catching_fish(&mut self, level: &mut ServerLevel, block_pos: BlockPos) {
        let mut fishing_speed = 1;
        let above = block_pos.above();
        if self.base.random.next_float() < 0.25 && level.is_raining_at(above) {
            fishing_speed += 1;
        }
        if self.base.random.next_float() < 0.5 && !level.can_see_sky(above) {
            fishing_speed -= 1;
        }
        let to_radians = DEG_TO_RAD;
        if self.nibble > 0 {
            self.nibble -= 1;
            if self.nibble <= 0 {
                self.time_until_lured = 0;
                self.time_until_hooked = 0;
                self.set_biting(false);
            }
        } else if self.time_until_hooked > 0 {
            self.time_until_hooked -= fishing_speed;
            if self.time_until_hooked > 0 {
                self.fish_angle += self.base.random.triangle(0.0, 9.188) as f32;
                let angle = self.fish_angle * to_radians;
                let angle_sin = mth::sin(f64::from(angle));
                let angle_cos = mth::cos(f64::from(angle));
                let fish_x =
                    self.base.x() + f64::from(angle_sin * self.time_until_hooked as f32 * 0.1f32);
                let fish_y = f64::from(mth::floor_f64(self.base.y()) as f32 + 1.0f32);
                let fish_z =
                    self.base.z() + f64::from(angle_cos * self.time_until_hooked as f32 * 0.1f32);
                let splash_state =
                    level.get_block_state(BlockPos::containing(fish_x, fish_y - 1.0, fish_z));
                if splash_state.block() == Block::WATER {
                    if self.base.random.next_float() < 0.15 {
                        level.send_particles(
                            ParticleOptions::Simple(ParticleType::BUBBLE),
                            fish_x,
                            fish_y - f64::from(0.1f32),
                            fish_z,
                            1,
                            f64::from(angle_sin),
                            0.1,
                            f64::from(angle_cos),
                            0.0,
                        );
                    }
                    let particle_x_movement = angle_sin * 0.04f32;
                    let particle_z_movement = angle_cos * 0.04f32;
                    level.send_particles(
                        ParticleOptions::Simple(ParticleType::FISHING),
                        fish_x,
                        fish_y,
                        fish_z,
                        0,
                        f64::from(particle_z_movement),
                        0.01,
                        f64::from(-particle_x_movement),
                        1.0,
                    );
                    level.send_particles(
                        ParticleOptions::Simple(ParticleType::FISHING),
                        fish_x,
                        fish_y,
                        fish_z,
                        0,
                        f64::from(-particle_z_movement),
                        0.01,
                        f64::from(particle_x_movement),
                        1.0,
                    );
                }
            } else {
                let pitch =
                    1.0 + (self.base.random.next_float() - self.base.random.next_float()) * 0.4;
                self.play_sound(level, SoundEvent::ENTITY_FISHING_BOBBER_SPLASH, 0.25, pitch);
                let y = self.base.y() + 0.5;
                let width = self.base.get_bb_width();
                let count = (1.0 + width * 20.0) as i32;
                level.send_particles(
                    ParticleOptions::Simple(ParticleType::BUBBLE),
                    self.base.x(),
                    y,
                    self.base.z(),
                    count,
                    f64::from(width),
                    0.0,
                    f64::from(width),
                    f64::from(0.2f32),
                );
                level.send_particles(
                    ParticleOptions::Simple(ParticleType::FISHING),
                    self.base.x(),
                    y,
                    self.base.z(),
                    count,
                    f64::from(width),
                    0.0,
                    f64::from(width),
                    f64::from(0.2f32),
                );
                self.nibble = mth::next_int(&mut self.base.random, 20, 40);
                self.set_biting(true);
            }
        } else if self.time_until_lured > 0 {
            self.time_until_lured -= fishing_speed;
            let mut tease_chance = 0.15f32;
            if self.time_until_lured < 20 {
                tease_chance += (20 - self.time_until_lured) as f32 * 0.05f32;
            } else if self.time_until_lured < 40 {
                tease_chance += (40 - self.time_until_lured) as f32 * 0.02f32;
            } else if self.time_until_lured < 60 {
                tease_chance += (60 - self.time_until_lured) as f32 * 0.01f32;
            }
            if self.base.random.next_float() < tease_chance {
                let angle = mth::next_float(&mut self.base.random, 0.0, 360.0) * to_radians;
                let dist = mth::next_float(&mut self.base.random, 25.0, 60.0);
                let fish_x = self.base.x() + f64::from(mth::sin(f64::from(angle)) * dist) * 0.1;
                let fish_y = f64::from(mth::floor_f64(self.base.y()) as f32 + 1.0f32);
                let fish_z = self.base.z() + f64::from(mth::cos(f64::from(angle)) * dist) * 0.1;
                let splash_state =
                    level.get_block_state(BlockPos::containing(fish_x, fish_y - 1.0, fish_z));
                if splash_state.block() == Block::WATER {
                    let count = 2 + self.base.random.next_int_bounded(2);
                    level.send_particles(
                        ParticleOptions::Simple(ParticleType::SPLASH),
                        fish_x,
                        fish_y,
                        fish_z,
                        count,
                        f64::from(0.1f32),
                        0.0,
                        f64::from(0.1f32),
                        0.0,
                    );
                }
            }
            if self.time_until_lured <= 0 {
                self.fish_angle = mth::next_float(&mut self.base.random, 0.0, 360.0);
                self.time_until_hooked = mth::next_int(&mut self.base.random, 20, 80);
            }
        } else {
            self.time_until_lured = mth::next_int(&mut self.base.random, 100, 600);
            self.time_until_lured -= self.lure_speed;
        }
    }

    /// `calculateOpenWater(blockPos)`: four layers of 5x5 blocks, air above and source water without a collision shape below.
    fn calculate_open_water(level: &ServerLevel, block_pos: BlockPos) -> bool {
        let mut previous_layer = OpenWaterType::Invalid;
        for y in -1..=2 {
            let layer = Self::open_water_type_for_area(
                level,
                block_pos.offset(-2, y, -2),
                block_pos.offset(2, y, 2),
            );
            match layer {
                OpenWaterType::AboveWater => {
                    if previous_layer == OpenWaterType::Invalid {
                        return false;
                    }
                }
                OpenWaterType::InsideWater => {
                    if previous_layer == OpenWaterType::AboveWater {
                        return false;
                    }
                }
                OpenWaterType::Invalid => return false,
            }
            previous_layer = layer;
        }
        true
    }

    /// `getOpenWaterTypeForArea(from, to)`.
    fn open_water_type_for_area(
        level: &ServerLevel,
        from: BlockPos,
        to: BlockPos,
    ) -> OpenWaterType {
        BlockPos::between_closed(&from, &to)
            .map(|pos| Self::open_water_type_for_block(level, pos))
            .reduce(|a, b| if a == b { a } else { OpenWaterType::Invalid })
            .unwrap_or(OpenWaterType::Invalid)
    }

    /// `getOpenWaterTypeForBlock(pos)`.
    fn open_water_type_for_block(level: &ServerLevel, pos: BlockPos) -> OpenWaterType {
        let state = level.get_block_state(pos);
        if !state.is_air() && state.block() != Block::LILY_PAD {
            let fluid = state.get_fluid_state();
            if fluid.is_tag("water")
                && fluid.is_source()
                && state.get_collision_shape(level, pos).is_empty()
            {
                OpenWaterType::InsideWater
            } else {
                OpenWaterType::Invalid
            }
        } else {
            OpenWaterType::AboveWater
        }
    }

    /// `pullEntity(entity)`: the owner is the player reeling the hook in (not in the level while its code runs).
    fn pull_entity(&self, level: &mut ServerLevel, owner: Vec3, entity: EntityId) {
        let delta = Vec3::new(
            owner.x - self.base.x(),
            owner.y - self.base.y(),
            owner.z - self.base.z(),
        )
        .scale(0.1);
        level.with_entity(entity, |_, e| {
            if e.can_simulate_movement() {
                let m = e.base().delta_movement();
                e.base_mut().set_delta_movement(m.add(delta));
            }
        });
    }

    /// `retrieve(rod)` for the player using the rod (the owner, out of the level while it acts). Returns the durability
    /// damage the rod takes.
    pub fn retrieve(
        &mut self,
        level: &mut ServerLevel,
        owner: &mut ServerPlayer,
        rod: &ItemStack,
    ) -> i32 {
        if self.should_stop_fishing(&*owner) {
            return 0;
        }
        let mut dmg = 0;
        if let Some(hooked) = self.hooked_in {
            self.pull_entity(level, owner.position(), hooked);
            let matches = {
                let hooked_in = level.get_entity(hooked).unwrap_or(self.as_dyn());
                CriteriaTriggers::FISHING_ROD_HOOKED.matches(&*level, &*owner, rod, hooked_in, &[])
            };
            matches.award(level, owner);
            level.broadcast_entity_event(self.id(), 31);
            dmg = if level
                .get_entity(hooked)
                .is_some_and(|e| e.as_any().is::<ItemEntity>())
            {
                3
            } else {
                5
            };
        } else if self.nibble > 0 {
            let luck = self.luck as f32
                + owner
                    .living()
                    .map_or(0.0, |l| l.get_attribute_value(Attribute::LUCK) as f32);
            let items = {
                let params = LootParams::builder(level)
                    .origin(self.position())
                    .tool(rod)
                    .this_entity(self.as_dyn())
                    .luck(luck)
                    .create(&LootContextParamSets::FISHING);
                let table = get_loot_table(
                    &level.registries,
                    &Identifier::parse("minecraft:gameplay/fishing").expect("valid identifier"),
                );
                table.get_random_items(&params)
            };
            let matches = CriteriaTriggers::FISHING_ROD_HOOKED.matches(
                &*level,
                &*owner,
                rod,
                self.as_dyn(),
                &items,
            );
            matches.award(level, owner);
            let fishes = mc_data::tags::get::<Item>("fishes");
            for stack in items {
                let is_fish = fishes.is_some_and(|t| t.contains(stack.item()));
                let mut entity = ItemEntity::with_stack(
                    level,
                    self.base.x(),
                    self.base.y(),
                    self.base.z(),
                    stack,
                );
                let xa = owner.base().x() - self.base.x();
                let ya = owner.base().y() - self.base.y();
                let za = owner.base().z() - self.base.z();
                entity.base_mut().set_delta_movement_xyz(
                    xa * 0.1,
                    ya * 0.1 + (xa * xa + ya * ya + za * za).sqrt().sqrt() * 0.08,
                    za * 0.1,
                );
                level.add_fresh_entity(Box::new(entity));
                let value = self.base.random.next_int_bounded(6) + 1;
                let orb = ExperienceOrb::at(
                    level,
                    owner.base().x(),
                    owner.base().y() + 0.5,
                    owner.base().z() + 0.5,
                    value,
                );
                level.add_fresh_entity(Box::new(orb));
                if is_fish {
                    owner.award_stat(level, Stats::FISH_CAUGHT, 1);
                }
            }
            dmg = 1;
        }
        if self.base.on_ground() {
            dmg = 2;
        }
        self.discard();
        dmg
    }
}

impl Projectile for FishingHook {
    impl_throwable_projectile!();

    fn should_bounce_on_world_border(&self) -> bool {
        true
    }

    /// `canHitEntity(entity)`: live item entities too.
    fn can_hit_entity(&self, level: &ServerLevel, entity: &dyn Entity) -> bool {
        super::can_hit_entity_base(self, level, entity)
            || (entity.is_alive() && entity.as_any().is::<ItemEntity>())
    }

    /// `getAddEntityPacket`: a hook without an owner names itself.
    fn add_entity_packet_data(&self) -> i32 {
        self.projectile.owner_id.unwrap_or(self.id())
    }

    fn on_hit_entity(&mut self, _level: &mut ServerLevel, hit: &EntityHitResult) {
        self.set_hooked_entity(Some(hit.entity));
    }

    fn on_hit_block(&mut self, level: &mut ServerLevel, hit: &BlockHitResult) {
        on_hit_block_base(self, level, hit);
        // `HitResult.distanceTo(entity)` is the squared distance
        let distance = hit.location.distance_to_sqr(self.base.position());
        let m = self.base.delta_movement();
        self.base.set_delta_movement(m.normalize().scale(distance));
    }
}

impl Entity for FishingHook {
    fn base(&self) -> &EntityData {
        &self.base
    }
    fn base_mut(&mut self) -> &mut EntityData {
        &mut self.base
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
    fn as_dyn(&self) -> &dyn Entity {
        self
    }
    fn as_dyn_mut(&mut self) -> &mut dyn Entity {
        self
    }
    impl_projectile_entity!();

    fn get_default_gravity(&self) -> f64 {
        f64::from(0.03f32)
    }

    fn get_movement_emission(&self) -> MovementEmission {
        MovementEmission::None
    }

    fn can_use_portal(&self, _ignore_passenger: bool) -> bool {
        false
    }

    fn fishing_hook_open_water(&self) -> Option<bool> {
        Some(self.open_water)
    }

    fn tick(&mut self, level: &mut ServerLevel) {
        let lsb = self.base.uuid.as_u128() as u64 as i64;
        self.syncronized_random.set_seed(lsb ^ level.game_time());
        super::tick(self, level);
        // `getPlayerOwner()`, then `shouldStopFishing(owner)` on what the level holds of it
        let owner = self
            .projectile
            .owner
            .and_then(|uuid| level.get_entity_by_uuid(uuid))
            .filter(|o| o.is_player());
        let Some(owner) = owner else {
            self.stop(level);
            return;
        };
        if !self.owner_allows_fishing(owner) {
            self.stop(level);
            return;
        }
        if self.base.on_ground() {
            self.life += 1;
            if self.life >= 1200 {
                self.stop(level);
                return;
            }
        } else {
            self.life = 0;
        }

        let mut liquid_height = 0.0f32;
        let block_pos = self.block_position();
        let fluid_state = level.get_fluid_state(block_pos);
        if fluid_state.is_tag("water") {
            liquid_height = fluid_state.get_height(&*level, block_pos);
        }
        let is_in_water = liquid_height > 0.0;
        if self.current_state == FishHookState::Flying {
            if self.hooked_in.is_some() {
                self.base.set_delta_movement(Vec3::ZERO);
                self.current_state = FishHookState::HookedInEntity;
                return;
            }
            if is_in_water {
                let m = self.base.delta_movement();
                self.base.set_delta_movement(m.multiply_xyz(0.3, 0.2, 0.3));
                self.current_state = FishHookState::Bobbing;
                return;
            }
            // `checkCollision()`
            let hit = util::get_hit_result_on_move_vector(level, self, &|e| {
                self.can_hit_entity(level, e)
            });
            hit_target_or_deflect_self(self, level, &hit);
        } else {
            if self.current_state == FishHookState::HookedInEntity {
                if let Some(hooked) = self.hooked_in {
                    let target = level
                        .get_entity(hooked)
                        .filter(|h| !h.is_removed() && h.can_interact_with_level())
                        .map(|h| {
                            Vec3::new(h.base().x(), h.base().get_y_progress(0.8), h.base().z())
                        });
                    match target {
                        Some(p) => self.set_pos(p.x, p.y, p.z),
                        None => {
                            self.set_hooked_entity(None);
                            self.current_state = FishHookState::Flying;
                        }
                    }
                }
                return;
            }
            if self.current_state == FishHookState::Bobbing {
                let movement = self.base.delta_movement();
                let mut force = self.base.y() + movement.y
                    - f64::from(block_pos.y())
                    - f64::from(liquid_height);
                if force.abs() < 0.01 {
                    force += force.signum() * 0.1;
                }
                let r = f64::from(self.base.random.next_float());
                self.base.set_delta_movement_xyz(
                    movement.x * 0.9,
                    movement.y - force * r * 0.2,
                    movement.z * 0.9,
                );
                if self.nibble <= 0 && self.time_until_hooked <= 0 {
                    self.open_water = true;
                } else {
                    self.open_water = self.open_water
                        && self.out_of_water_time < MAX_OUT_OF_WATER_TIME
                        && Self::calculate_open_water(level, block_pos);
                }
                if is_in_water {
                    self.out_of_water_time = (self.out_of_water_time - 1).max(0);
                    if self.biting {
                        let a = f64::from(self.syncronized_random.next_float());
                        let b = f64::from(self.syncronized_random.next_float());
                        let m = self.base.delta_movement();
                        self.base
                            .set_delta_movement(m.add_xyz(0.0, -0.1 * a * b, 0.0));
                    }
                    self.catching_fish(level, block_pos);
                } else {
                    self.out_of_water_time =
                        (self.out_of_water_time + 1).min(MAX_OUT_OF_WATER_TIME);
                }
            }
        }
        if !fluid_state.is_tag("water") && !self.base.on_ground() && self.hooked_in.is_none() {
            let gravity = self.get_default_gravity();
            let m = self.base.delta_movement();
            self.base.set_delta_movement(m.add_xyz(0.0, -gravity, 0.0));
        }
        let movement = self.base.delta_movement();
        self.r#move(level, MoverType::SelfMover, movement);
        self.apply_effects_from_blocks(level);
        update_rotation(self);
        if self.current_state == FishHookState::Flying
            && (self.base.on_ground() || self.base.horizontal_collision)
        {
            self.base.set_delta_movement(Vec3::ZERO);
        }
        let m = self.base.delta_movement();
        self.base.set_delta_movement(m.scale(0.92));
        self.reapply_position();
    }

    // `addAdditionalSaveData` / `readAdditionalSaveData` are empty: the entity type is not saved.
    fn add_additional_save_data(&self, _out: &mut CompoundTag) {}

    fn read_additional_save_data(&mut self, _input: &CompoundTag) {}
}

#[distributed_slice(ENTITY_FACTORIES)]
static FISHING_BOBBER_FACTORY: EntityRegistration = EntityRegistration {
    entity_type: EntityType::FISHING_BOBBER,
    create: |ty, id, _| Box::new(FishingHook::new(ty, id, 0, 0)),
};

#[distributed_slice(SCENARIO_DUMP_EXTRA)]
static FISHING_HOOK_DUMP: ScenarioDumpExtra = ScenarioDumpExtra {
    order: 30,
    applies: |e| e.as_any().is::<FishingHook>(),
    fields: |e, _level, ctx| {
        let h = e
            .as_any()
            .downcast_ref::<FishingHook>()
            .expect("fishing hook");
        let state = match h.current_state {
            FishHookState::Flying => "flying",
            FishHookState::HookedInEntity => "hooked_in_entity",
            FishHookState::Bobbing => "bobbing",
        };
        vec![
            ("state".into(), state.into()),
            ("life".into(), h.life.to_string()),
            ("nibble".into(), h.nibble.to_string()),
            ("lured".into(), h.time_until_lured.to_string()),
            ("hooked".into(), h.time_until_hooked.to_string()),
            ("angle".into(), java_float_to_string(h.fish_angle)),
            ("biting".into(), u8::from(h.biting).to_string()),
            ("openwater".into(), u8::from(h.open_water).to_string()),
            ("out".into(), h.out_of_water_time.to_string()),
            ("hookedin".into(), ctx.name_opt(h.hooked_in)),
            ("luck".into(), h.luck.to_string()),
            ("lure".into(), h.lure_speed.to_string()),
        ]
    },
};
