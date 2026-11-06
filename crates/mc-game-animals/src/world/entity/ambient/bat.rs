//! `Bat` (`AmbientCreature`).

use linkme::distributed_slice;
use mc_core::math::mth;
use mc_core::phys::Vec3;
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_data::enums::EntitySpawnReason;
use mc_data::{EntityType, SoundEvent};
use mc_game_core::define_accessor;
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::damagesource::DamageSource;
use mc_game_core::world::entity::ai::navigation::StandardPathNavigation;
use mc_game_core::world::entity::ai::targeting::TargetingConditions;
use mc_game_core::world::entity::factory::{ENTITY_FACTORIES, EntityRegistration};
use mc_game_core::world::entity::living_entity::util::lm;
use mc_game_core::world::entity::mob::{self, MobData, MobKind};
use mc_game_core::world::entity::{Entity, EntityId, MovementEmission};
use mc_game_core::world::level::block::BlockStateExt as _;
use mc_game_core::world::level::server_level_accessor::ServerLevelAccessor;
use mc_game_core::world::level::{BlockGetter as _, LevelAccessor as _, LevelReader as _};
use mc_nbt::CompoundTag;
use mc_world::heightmap::HeightmapType;
use std::sync::LazyLock;

define_accessor!(pub DATA_ID_FLAGS: Byte = Bat);

pub const FLAP_LENGTH_SECONDS: f32 = 0.5;
pub const TICKS_PER_FLAP: f32 = 10.0;
const FLAG_RESTING: i8 = 1;

/// `BAT_RESTING_TARGETING`.
static BAT_RESTING_TARGETING: LazyLock<TargetingConditions> =
    LazyLock::new(|| TargetingConditions::for_non_combat().range(4.0));

/// `Bat`.
pub struct Bat {
    pub mob: MobData,
    target_position: Option<BlockPos>,
}

/// `Math.signum(double)`: zeros and NaN are returned unchanged.
fn signum(d: f64) -> f64 {
    if d == 0.0 || d.is_nan() {
        d
    } else {
        d.signum()
    }
}

impl Bat {
    /// `new Bat(type, level)`.
    pub fn new(ty: EntityType, id: EntityId) -> Self {
        let mob = MobData::new(ty, id, MobKind::Mob, |b| {
            b.define(DATA_ID_FLAGS, 0);
        });
        let mut bat = Bat {
            mob,
            target_position: None,
        };
        let navigation = StandardPathNavigation::ground(&bat);
        bat.mob.set_navigation(Box::new(navigation));
        bat.set_resting(true);
        bat
    }

    /// `isResting()`.
    pub fn is_resting(&self) -> bool {
        self.mob.living.entity.entity_data.get(DATA_ID_FLAGS) & FLAG_RESTING != 0
    }

    /// `setResting(value)`.
    pub fn set_resting(&mut self, value: bool) {
        let current = self.mob.living.entity.entity_data.get(DATA_ID_FLAGS);
        self.mob.living.entity.entity_data.set(
            DATA_ID_FLAGS,
            if value { current | 1 } else { current & -2 },
        );
    }
}

impl Entity for Bat {
    mc_game_core::mob_entity_accessors!(mob);

    fn get_encode_id(&self) -> Option<&'static str> {
        Some("minecraft:bat")
    }

    fn is_flapping(&self) -> bool {
        !self.is_resting() && (self.base().tick_count as f32) % TICKS_PER_FLAP == 0.0
    }

    fn get_sound_volume(&self) -> f32 {
        0.1
    }

    fn get_voice_pitch(&mut self) -> f32 {
        mc_game_core::world::entity::living_entity::damage::voice_pitch(self) * 0.95
    }

    /// `getAmbientSound()`: the resting roll is made in [`Entity::play_ambient_sound`] (it needs the random).
    fn get_ambient_sound(&self) -> Option<SoundEvent> {
        Some(SoundEvent::ENTITY_BAT_AMBIENT)
    }

    fn play_ambient_sound(&mut self, level: &mut ServerLevel) {
        if self.is_resting() && self.base_mut().random.next_int_bounded(4) != 0 {
            return;
        }
        mc_game_core::world::entity::living_entity::damage::make_sound(
            self,
            level,
            SoundEvent::ENTITY_BAT_AMBIENT,
        );
    }

    fn get_hurt_sound(&self, _source: &DamageSource) -> Option<SoundEvent> {
        Some(SoundEvent::ENTITY_BAT_HURT)
    }

    fn get_death_sound(&self) -> Option<SoundEvent> {
        Some(SoundEvent::ENTITY_BAT_DEATH)
    }

    fn is_pushable(&self, _level: &ServerLevel) -> bool {
        false
    }

    fn do_push(&mut self, _level: &mut ServerLevel, _other: &mut dyn Entity) {}

    fn push_entities(&mut self, _level: &mut ServerLevel) {}

    /// `AmbientCreature.canBeLeashed()`.
    fn can_be_leashed(&self) -> bool {
        false
    }

    fn tick(&mut self, level: &mut ServerLevel) {
        mob::tick::tick(self, level);
        if self.is_resting() {
            self.base_mut().set_delta_movement(Vec3::ZERO);
            let y = f64::from(mth::floor_f64(self.base().y())) + 1.0
                - f64::from(self.base().get_bb_height());
            let (x, z) = (self.base().x(), self.base().z());
            self.base_mut().set_pos_raw(x, y, z);
        } else {
            let movement = self.base().delta_movement();
            self.base_mut()
                .set_delta_movement(movement.multiply_xyz(1.0, 0.6, 1.0));
        }
    }

    fn custom_server_ai_step(&mut self, level: &mut ServerLevel) {
        let pos = self.block_position();
        let above = pos.above();
        if self.is_resting() {
            let is_silent = self.base().is_silent();
            if level.get_block_state(above).is_redstone_conductor() {
                if self.base_mut().random.next_int_bounded(200) == 0 {
                    let yaw = self.base_mut().random.next_int_bounded(360) as f32;
                    lm(self).y_head_rot = yaw;
                }
                let p = self.base().position();
                if mob::ext::get_nearest_player(level, &BAT_RESTING_TARGETING, self, p.x, p.y, p.z)
                    .is_some()
                {
                    self.set_resting(false);
                    if !is_silent {
                        level.level_event_impl(None, 1025, pos, 0);
                    }
                }
            } else {
                self.set_resting(false);
                if !is_silent {
                    level.level_event_impl(None, 1025, pos, 0);
                }
            }
        } else {
            if let Some(target) = self.target_position {
                if !level.is_empty_block(target) || target.y() <= level.get_min_y() {
                    self.target_position = None;
                }
            }
            let position = self.base().position();
            let needs_new_target = match self.target_position {
                None => true,
                Some(target) => {
                    self.base_mut().random.next_int_bounded(30) == 0
                        || target.closer_to_center_than(&position, 2.0)
                }
            };
            if needs_new_target {
                let random = &mut self.base_mut().random;
                let x = position.x + f64::from(random.next_int_bounded(7))
                    - f64::from(random.next_int_bounded(7));
                let y = position.y + f64::from(random.next_int_bounded(6)) - 2.0;
                let z = position.z + f64::from(random.next_int_bounded(7))
                    - f64::from(random.next_int_bounded(7));
                self.target_position = Some(BlockPos::containing(x, y, z));
            }
            let target = self.target_position.expect("target position was just set");
            let (px, py, pz) = (self.base().x(), self.base().y(), self.base().z());
            let dx = f64::from(target.x()) + 0.5 - px;
            let dy = f64::from(target.y()) + 0.1 - py;
            let dz = f64::from(target.z()) + 0.5 - pz;
            let movement = self.base().delta_movement();
            let new_movement = movement.add_xyz(
                (signum(dx) * 0.5 - movement.x) * f64::from(0.1f32),
                (signum(dy) * f64::from(0.7f32) - movement.y) * f64::from(0.1f32),
                (signum(dz) * 0.5 - movement.z) * f64::from(0.1f32),
            );
            self.base_mut().set_delta_movement(new_movement);
            let y_rot_d = (mth::atan2(new_movement.z, new_movement.x) * f64::from(180.0f32)
                / f64::from(std::f32::consts::PI)) as f32
                - 90.0;
            let rot_diff = mth::wrap_degrees_f32(y_rot_d - self.base().y_rot());
            lm(self).zza = 0.5;
            let y_rot = self.base().y_rot() + rot_diff;
            self.set_y_rot(y_rot);
            if self.base_mut().random.next_int_bounded(100) == 0
                && level.get_block_state(above).is_redstone_conductor()
            {
                self.set_resting(true);
            }
        }
    }

    fn get_movement_emission(&self) -> MovementEmission {
        MovementEmission::Events
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

    fn is_ignoring_block_triggers(&self) -> bool {
        true
    }

    fn hurt_server(&mut self, level: &mut ServerLevel, source: &DamageSource, amount: f32) -> bool {
        if self.is_invulnerable_to(level, source) {
            return false;
        }
        if self.is_resting() {
            self.set_resting(false);
        }
        mc_game_core::world::entity::living_entity::damage::hurt_server(self, level, source, amount)
    }

    fn add_additional_save_data(&self, out: &mut CompoundTag) {
        mob::nbt::add_additional_save_data(self, out);
        out.put_byte(
            "BatFlags",
            self.mob.living.entity.entity_data.get(DATA_ID_FLAGS),
        );
    }

    fn read_additional_save_data(&mut self, input: &CompoundTag) {
        mob::nbt::read_additional_save_data(self, input);
        self.mob
            .living
            .entity
            .entity_data
            .set(DATA_ID_FLAGS, input.get_byte_or("BatFlags", 0));
    }
}

/// `Bat.checkBatSpawnRules(type, level, spawnReason, pos, random)`.
pub fn check_bat_spawn_rules(
    ty: EntityType,
    level: &dyn ServerLevelAccessor,
    reason: EntitySpawnReason,
    pos: BlockPos,
    random: &mut dyn RandomSource,
) -> bool {
    if pos.y() >= level.get_height_at(HeightmapType::WorldSurface, pos.x(), pos.z()) {
        false
    } else if random.next_boolean() {
        false
    } else if level.get_max_local_raw_brightness(pos) > random.next_int_bounded(4) {
        false
    } else if !level
        .get_block_state(pos.below())
        .is_tag("bats_spawnable_on")
    {
        false
    } else {
        mc_game_core::world::entity::spawn_predicates::check_mob_spawn_rules(
            ty, level, reason, pos, random,
        )
    }
}

mc_game_core::register_spawn_placement!(EntityType::BAT, check_bat_spawn_rules);

#[distributed_slice(ENTITY_FACTORIES)]
static BAT: EntityRegistration = EntityRegistration {
    entity_type: EntityType::BAT,
    create: |ty, id, _registries| Box::new(Bat::new(ty, id)),
};
