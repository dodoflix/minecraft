//! `Level.explode(..)` overloads and `ServerLevel.explode(..)`: runs a [`ServerExplosion`] and sends
//! `ClientboundExplodePacket` to the players within 64 blocks.

use super::ServerLevel;
use crate::world::damagesource::DamageSource;
use crate::world::entity::{Entity, EntityId};
use crate::world::level::explosion::{
    BlockInteraction, ExplosionDamageCalculator, ExplosionInteraction, ServerExplosion,
};
use mc_core::phys::Vec3;
use mc_data::{ParticleType, SoundEvent};
use mc_protocol::packets::play::{
    ClientboundExplodePacket, ExplosionParticleInfo, ParticleOptions, SoundEventHolder, Weighted,
    WeightedList,
};

/// `Level.DEFAULT_EXPLOSION_BLOCK_PARTICLES`.
pub fn default_explosion_block_particles() -> WeightedList<ExplosionParticleInfo> {
    WeightedList(vec![
        Weighted {
            value: ExplosionParticleInfo {
                particle: ParticleOptions::Simple(ParticleType::POOF),
                scaling: 0.5,
                speed: 1.0,
            },
            weight: 1,
        },
        Weighted {
            value: ExplosionParticleInfo {
                particle: ParticleOptions::Simple(ParticleType::SMOKE),
                scaling: 1.0,
                speed: 1.0,
            },
            weight: 1,
        },
    ])
}

/// The arguments of `ServerLevel.explode(..)`; [`Explode::new`] = the short `Level.explode(source, x, y, z, r, interaction)`
/// overload (default damage source/calculator/particles/sound), the fields are the long overload's parameters.
pub struct Explode<'a> {
    /// The exploding entity: lent to the level for the blast, so victims, listeners and loot read (and, for kill credit and
    /// triggers, re-enter) it like vanilla's object. `None` = no entity (a block).
    pub source: Option<&'a mut dyn Entity>,
    pub damage_source: Option<DamageSource>,
    pub calculator: Option<&'a dyn ExplosionDamageCalculator>,
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub radius: f32,
    pub fire: bool,
    pub interaction: ExplosionInteraction,
    pub small_explosion_particles: ParticleOptions,
    pub large_explosion_particles: ParticleOptions,
    pub block_particles: WeightedList<ExplosionParticleInfo>,
    pub explosion_sound: SoundEventHolder,
}

impl<'a> Explode<'a> {
    pub fn new(
        source: Option<&'a mut dyn Entity>,
        x: f64,
        y: f64,
        z: f64,
        radius: f32,
        interaction: ExplosionInteraction,
    ) -> Self {
        Self {
            source,
            damage_source: None,
            calculator: None,
            x,
            y,
            z,
            radius,
            fire: false,
            interaction,
            small_explosion_particles: ParticleOptions::Simple(ParticleType::EXPLOSION),
            large_explosion_particles: ParticleOptions::Simple(ParticleType::EXPLOSION_EMITTER),
            block_particles: default_explosion_block_particles(),
            explosion_sound: SoundEventHolder::Reference(SoundEvent::ENTITY_GENERIC_EXPLODE),
        }
    }

    pub fn with_fire(mut self, fire: bool) -> Self {
        self.fire = fire;
        self
    }

    pub fn with_damage_source(
        mut self,
        damage_source: Option<DamageSource>,
        calculator: Option<&'a dyn ExplosionDamageCalculator>,
    ) -> Self {
        self.damage_source = damage_source;
        self.calculator = calculator;
        self
    }
}

impl ServerLevel {
    /// `ServerLevel.explode(..)`. The source entity is lent to the level while the blast runs (it is usually out of its slot
    /// because its own code explodes). An entity that is hurt by an explosion started by its own code (a player using a bed
    /// or an anchor) must be lent (`ServerLevel::lend`) to be hurt, knocked back and sent the packet.
    pub fn explode(&mut self, mut params: Explode<'_>) {
        match params.source.take() {
            Some(source) => {
                let id = source.id();
                self.lend(source, |level| level.explode_blast(params, Some(id)));
            }
            None => self.explode_blast(params, None),
        }
    }

    fn explode_blast(&mut self, params: Explode<'_>, source: Option<EntityId>) {
        let block_interaction = match params.interaction {
            ExplosionInteraction::None => BlockInteraction::Keep,
            ExplosionInteraction::Block => self.get_destroy_type(self.block_explosion_drop_decay()),
            ExplosionInteraction::Mob => {
                if self.mob_griefing() {
                    self.get_destroy_type(self.mob_explosion_drop_decay())
                } else {
                    BlockInteraction::Keep
                }
            }
            ExplosionInteraction::Tnt => self.get_destroy_type(self.tnt_explosion_drop_decay()),
            ExplosionInteraction::Trigger => BlockInteraction::TriggerBlock,
        };
        let center = Vec3::new(params.x, params.y, params.z);
        let mut explosion = ServerExplosion::new(
            self,
            source,
            params.damage_source,
            params.calculator,
            center,
            params.radius,
            params.fire,
            block_interaction,
        );
        let block_count = explosion.explode(self);
        let explosion_particle = if explosion.is_small() {
            params.small_explosion_particles
        } else {
            params.large_explosion_particles
        };
        let play_sound = source
            .and_then(|id| self.get_entity_or_lent(id))
            .is_none_or(|s| !s.base().is_silent());
        let packet = |player_id, position: Vec3| {
            (position.distance_to_sqr(center) < 4096.0).then(|| ClientboundExplodePacket {
                center,
                radius: params.radius,
                block_count: block_count as i32,
                player_knockback: explosion.get_hit_player(player_id),
                explosion_particle: explosion_particle.clone(),
                explosion_sound: params.explosion_sound.clone(),
                block_particles: params.block_particles.clone(),
                play_sound,
            })
        };
        for record in self.player_records() {
            if let (Some(connection), Some(p)) =
                (&record.connection, packet(record.id, record.position))
            {
                connection.send(&p);
            }
        }
    }

    /// `getDestroyType(gameRule)`.
    fn get_destroy_type(&self, drop_decay: bool) -> BlockInteraction {
        if drop_decay {
            BlockInteraction::DestroyWithDecay
        } else {
            BlockInteraction::Destroy
        }
    }
}
