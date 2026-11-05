//! `net.minecraft.world.entity.animal.squid.GlowSquid`.

use mc_core::random::RandomSource as _;
use crate::world::level::level_access::{BlockGetter as _, LevelAccessor as _};
use crate::world::level::material::FluidStateExt as _;
use super::squid::{self, SquidData, SquidLike, squid_entity};
use crate::define_accessor;
use crate::register_spawn_placement;
use crate::server::level::ServerLevel;
use crate::world::damagesource::DamageSource;
use crate::world::entity::ai::navigation::StandardPathNavigation;
use crate::world::entity::factory::{ENTITY_FACTORIES, EntityRegistration};
use crate::world::entity::mob::{self, MobData, MobKind};
use crate::world::entity::{Entity, EntityId};
use crate::world::level::LevelAccessor;
use crate::world::level::server_level_accessor::ServerLevelAccessor;
use linkme::distributed_slice;
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_data::block::Block;
use mc_data::enums::EntitySpawnReason;
use mc_data::{EntityType, SoundEvent};
use mc_nbt::CompoundTag;
use mc_protocol::packets::play::ParticleOptions;

define_accessor!(pub DATA_DARK_TICKS_REMAINING: Int = GlowSquid);

pub struct GlowSquid {
    pub mob: MobData,
    pub squid: SquidData,
}

impl GlowSquid {
    /// `new GlowSquid(type, level)`.
    pub fn new(ty: EntityType, id: EntityId) -> Self {
        let mut mob = MobData::new(ty, id, MobKind::AgeableMob, |b| {
            b.define(DATA_DARK_TICKS_REMAINING, 0);
        });
        let squid = SquidData::new(&mut mob);
        let mut s = GlowSquid { mob, squid };
        let navigation = StandardPathNavigation::ground(&s);
        squid::init(&mut s.mob, navigation);
        s
    }

    /// `setDarkTicks(ticks)`.
    fn set_dark_ticks(&mut self, ticks: i32) {
        self.mob.living.entity.entity_data.set(DATA_DARK_TICKS_REMAINING, ticks);
    }

    /// `getDarkTicksRemaining()`.
    pub fn get_dark_ticks_remaining(&self) -> i32 {
        self.mob.living.entity.entity_data.get(DATA_DARK_TICKS_REMAINING)
    }
}

impl SquidLike for GlowSquid {
    fn squid(&self) -> &SquidData {
        &self.squid
    }
    fn squid_mut(&mut self) -> &mut SquidData {
        &mut self.squid
    }
    fn get_squirt_sound(&self) -> SoundEvent {
        SoundEvent::ENTITY_GLOW_SQUID_SQUIRT
    }
    fn get_ink_particle(&self) -> ParticleOptions {
        ParticleOptions::Simple(mc_data::ParticleType::GLOW_SQUID_INK)
    }
}

impl Entity for GlowSquid {
    crate::mob_entity_accessors!(mob);
    crate::world::entity::animal::fish::water_animal::water_creature_entity!(no_leash);
    // `Squid`'s overrides, except `aiStep` / `hurtServer` which `GlowSquid` extends below
    squid_entity!(skip_ai_step_hurt);

    fn get_encode_id(&self) -> Option<&'static str> {
        Some("minecraft:glow_squid")
    }

    fn get_breed_offspring(&mut self, level: &mut ServerLevel, _partner: EntityId) -> Option<Box<dyn Entity>> {
        squid::get_breed_offspring(EntityType::GLOW_SQUID, level)
    }

    fn get_ambient_sound(&self) -> Option<SoundEvent> {
        Some(SoundEvent::ENTITY_GLOW_SQUID_AMBIENT)
    }

    fn get_hurt_sound(&self, _source: &DamageSource) -> Option<SoundEvent> {
        Some(SoundEvent::ENTITY_GLOW_SQUID_HURT)
    }

    fn get_death_sound(&self) -> Option<SoundEvent> {
        Some(SoundEvent::ENTITY_GLOW_SQUID_DEATH)
    }

    fn add_additional_save_data(&self, out: &mut CompoundTag) {
        mob::nbt::add_additional_save_data(self, out);
        out.put_int("DarkTicksRemaining", self.get_dark_ticks_remaining());
    }

    fn read_additional_save_data(&mut self, input: &CompoundTag) {
        mob::nbt::read_additional_save_data(self, input);
        self.set_dark_ticks(input.get_int_or("DarkTicksRemaining", 0));
    }

    fn ai_step(&mut self, level: &mut ServerLevel) {
        squid::ai_step(self, level);
        let dark_ticks = self.get_dark_ticks_remaining();
        if dark_ticks > 0 {
            self.set_dark_ticks(dark_ticks - 1);
        }
        // `level.addParticle(GLOW, getRandomX(0.6), getRandomY(), getRandomZ(0.6), ..)`: client-side particle; the
        // argument evaluation still draws the three doubles
        let b = self.base_mut();
        let _ = b.get_random_x(0.6);
        let _ = b.get_random_y();
        let _ = b.get_random_z(0.6);
    }

    fn hurt_server(&mut self, level: &mut ServerLevel, source: &DamageSource, damage: f32) -> bool {
        let hurt = squid::hurt_server(self, level, source, damage);
        if hurt {
            self.set_dark_ticks(100);
        }
        hurt
    }
}

/// `GlowSquid.checkGlowSquidSpawnRules`.
fn check_glow_squid_spawn_rules(_ty: EntityType, level: &dyn ServerLevelAccessor, _reason: EntitySpawnReason, pos: BlockPos, _random: &mut dyn RandomSource) -> bool {
    pos.y() <= level.get_sea_level() - 33 && level.get_raw_brightness(pos, 0) == 0 && level.get_block_state(pos).block() == Block::WATER
}

register_spawn_placement!(EntityType::GLOW_SQUID, check_glow_squid_spawn_rules);

#[distributed_slice(ENTITY_FACTORIES)]
static GLOW_SQUID: EntityRegistration = EntityRegistration { entity_type: EntityType::GLOW_SQUID, create: |ty, id, _registries| Box::new(GlowSquid::new(ty, id)) };

#[allow(dead_code)]
fn _unused(_: &dyn LevelAccessor) {}
