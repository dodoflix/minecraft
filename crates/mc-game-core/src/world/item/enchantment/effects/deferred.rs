//! Entity effects whose world-side services belong to other slices (explosions, particles, entity spawning, functions).

use mc_core::phys::Vec3;

use super::{EnchantmentEntityEffect, ExplodeEffect, SummonEntityEffect};
use crate::commands::arguments::coordinates::block_pos_argument::is_in_spawnable_bounds;
use crate::server::level::ServerLevel;
use crate::server::level::level_explode::Explode;
use crate::world::damagesource::DamageSource;
use crate::world::entity::EntityId;
use crate::world::entity::lightning_bolt::LightningBolt;
use crate::world::entity::spawn_config::create_at;
use crate::world::item::enchantment::actors::Actors;
use crate::world::item::enchantment::enchanted_item_in_use::EnchantedItemInUse;
use crate::world::item::enchantment::value_codecs::ExplosionInteraction as EffectInteraction;
use crate::world::level::LevelAccessor;
use crate::world::level::explosion::{
    ExplosionInteraction, ImmuneBlocks, SimpleExplosionDamageCalculator,
};
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource as _;
use mc_data::enums::EntitySpawnReason;
use mc_data::{Block, EntityType};
use mc_protocol::components::{Holder, RegistrySet};
use mc_protocol::packets::play::{
    DirectSoundEvent, ExplosionParticleInfo as PacketParticleInfo, SoundEventHolder, Weighted,
    WeightedList,
};

pub fn apply(
    effect: &EnchantmentEntityEffect,
    level: &mut ServerLevel,
    actors: &mut Actors<'_>,
    enchantment_level: i32,
    item: &mut EnchantedItemInUse,
    entity: EntityId,
    _position: Vec3,
) {
    match effect {
        // `Player.causeFoodExhaustion(amount)`
        EnchantmentEntityEffect::ApplyExhaustion { amount } => {
            let amount = amount.calculate(enchantment_level);
            actors.with(level, entity, |_, e| {
                if let Some(p) = e.player_mut() {
                    if !p.abilities.invulnerable {
                        p.food_data.add_exhaustion(amount);
                    }
                }
            });
        }
        // `Enchantment run_function effect failed for non-existent function`: no data pack functions are loaded in this server
        EnchantmentEntityEffect::RunFunction { function } => {
            log::error!(
                "Enchantment run_function effect failed for non-existent function {function}"
            );
        }
        EnchantmentEntityEffect::SpawnParticles(e) => {
            let particles = actors.with(level, entity, |lvl, t| {
                let movement = t.get_known_movement(lvl);
                let (w, h) = (t.base().get_bb_width(), t.base().get_bb_height());
                let pos = _position;
                let random = &mut t.base_mut().random;
                let x = e
                    .horizontal_position
                    .get_coordinate(pos.x, pos.x, w, random);
                let y = e.vertical_position.get_coordinate(
                    pos.y,
                    pos.y + f64::from(h / 2.0f32),
                    h,
                    random,
                );
                let z = e
                    .horizontal_position
                    .get_coordinate(pos.z, pos.z, w, random);
                let vx = e.horizontal_velocity.get_velocity(movement.x, random);
                let vy = e.vertical_velocity.get_velocity(movement.y, random);
                let vz = e.horizontal_velocity.get_velocity(movement.z, random);
                let speed = e.speed.sample(random);
                (x, y, z, vx, vy, vz, speed)
            });
            if let Some((x, y, z, vx, vy, vz, speed)) = particles {
                level.send_particles(e.particle.clone(), x, y, z, 0, vx, vy, vz, f64::from(speed));
            }
        }
        EnchantmentEntityEffect::Explode(e) => {
            explode(e, level, actors, enchantment_level, entity, _position)
        }
        EnchantmentEntityEffect::SummonEntity(e) => {
            summon_entity(e, level, actors, item, entity, _position)
        }
        _ => unreachable!("handled in apply"),
    }
}

/// `ExplodeEffect.apply`.
fn explode(
    e: &ExplodeEffect,
    level: &mut ServerLevel,
    actors: &mut Actors<'_>,
    enchantment_level: i32,
    entity: EntityId,
    position: Vec3,
) {
    let pos = position.add(e.offset);
    let damage_source = e.damage_type.as_ref().and_then(|holder| {
        let (id, data) = super::apply::damage_type_of(level, holder)?;
        Some(if e.attribute_to_user {
            let source = actors.get(level, entity);
            DamageSource::new(id, data, source, source, None)
        } else {
            DamageSource::new(id, data, None, None, Some(pos))
        })
    });
    let immune_blocks = e.immune_blocks.as_ref().map(|set| match set {
        RegistrySet::Tag(tag) => ImmuneBlocks::List(
            mc_data::tags::get::<Block>(&tag.to_string())
                .map(|t| t.iter().collect())
                .unwrap_or_default(),
        ),
        RegistrySet::Entries(blocks) => ImmuneBlocks::List(blocks.clone()),
    });
    let calculator = SimpleExplosionDamageCalculator {
        explodes_blocks: e.block_interaction != EffectInteraction::None,
        damages_entities: e.damage_type.is_some(),
        knockback_multiplier: e
            .knockback_multiplier
            .as_ref()
            .map(|v| v.calculate(enchantment_level)),
        immune_blocks,
    };
    let interaction = match e.block_interaction {
        EffectInteraction::None => ExplosionInteraction::None,
        EffectInteraction::Block => ExplosionInteraction::Block,
        EffectInteraction::Mob => ExplosionInteraction::Mob,
        EffectInteraction::Tnt => ExplosionInteraction::Tnt,
        EffectInteraction::Trigger => ExplosionInteraction::Trigger,
    };
    let sound = match &e.sound {
        Holder::Reference(id) => {
            mc_data::SoundEvent::by_name(&id.to_string()).map(SoundEventHolder::Reference)
        }
        Holder::Direct(d) => Some(SoundEventHolder::Direct(DirectSoundEvent {
            location: d.location.clone(),
            fixed_range: d.fixed_range,
        })),
    };
    let Some(sound) = sound else { return };
    let block_particles = WeightedList(
        e.block_particles
            .iter()
            .map(|w| Weighted {
                value: PacketParticleInfo {
                    particle: w.value.particle.clone(),
                    scaling: w.value.scaling,
                    speed: w.value.speed,
                },
                weight: w.weight,
            })
            .collect(),
    );
    let radius = e.radius.calculate(enchantment_level).max(0.0);
    let run = |level: &mut ServerLevel, source: Option<&dyn crate::world::entity::Entity>| {
        let mut params = Explode::new(source, pos.x, pos.y, pos.z, radius, interaction)
            .with_fire(e.create_fire)
            .with_damage_source(damage_source.clone(), Some(&calculator));
        params.small_explosion_particles = e.small_particle.clone();
        params.large_explosion_particles = e.large_particle.clone();
        params.block_particles = block_particles.clone();
        params.explosion_sound = sound.clone();
        level.explode(params, &mut []);
    };
    if e.attribute_to_user {
        // the user may be the entity that is running (outside the level): `explode` takes it as the explosion source
        if actors
            .with(level, entity, |level, user| run(level, Some(&*user)))
            .is_none()
        {
            run(level, None);
        }
    } else {
        run(level, None);
    }
}

/// `SummonEntityEffect.apply`.
fn summon_entity(
    e: &SummonEntityEffect,
    level: &mut ServerLevel,
    actors: &mut Actors<'_>,
    item: &EnchantedItemInUse,
    entity: EntityId,
    position: Vec3,
) {
    let block_pos = BlockPos::containing_position(&position);
    if !is_in_spawnable_bounds(block_pos) {
        return;
    }
    let candidates: Vec<EntityType> = match &e.entity_types {
        RegistrySet::Tag(tag) => mc_data::tags::get::<EntityType>(&tag.to_string())
            .map(|t| t.iter().collect())
            .unwrap_or_default(),
        RegistrySet::Entries(types) => types.clone(),
    };
    if candidates.is_empty() {
        return;
    }
    // `HolderSet.getRandomElement(random)`: `Util.getRandom(list, random)`
    let ty =
        candidates[LevelAccessor::random(level).next_int_bounded(candidates.len() as i32) as usize];
    let Some(spawned) = create_at(
        level,
        ty,
        None,
        block_pos,
        EntitySpawnReason::Triggered,
        false,
        false,
    ) else {
        return;
    };
    let spawned_id = spawned.id();
    level.add_fresh_entity_with_passengers(vec![spawned]);
    let owner_is_player = item
        .owner
        .is_some_and(|o| actors.get(level, o).is_some_and(|o| o.is_player()));
    let team = if e.join_team {
        actors.get(level, entity).and_then(|t| {
            level
                .scoreboard()
                .get_players_team(&t.get_scoreboard_name())
                .map(|t| t.get_name().to_string())
        })
    } else {
        None
    };
    level.with_entity(spawned_id, |level, spawned| {
        if let Some(bolt) = spawned.as_any_mut().downcast_mut::<LightningBolt>()
            && owner_is_player
        {
            bolt.set_cause(item.owner);
        }
        if let Some(team) = &team {
            level
                .scoreboard_mut()
                .add_player_to_team(&spawned.get_scoreboard_name(), team);
        }
        let (y_rot, x_rot) = (spawned.base().y_rot(), spawned.base().x_rot());
        spawned.snap_to(position.x, position.y, position.z, y_rot, x_rot);
    });
}
