//! `LivingEntity.die`, death loot and experience drops, kill credit.

use mc_core::ident::Identifier;
use mc_data::enums::Pose;
use mc_data::{Block, EntityType, GameEvent, Item};

use super::damage::broadcast_entity_event;
use crate::server::level::ServerLevel;
use crate::world::damagesource::{DamageEntity, DamageSource};
use crate::world::entity::Entity;
use crate::world::item::ItemStack;
use crate::world::item::enchantment::actors::Actors;

/// `LivingEntity.getKillCredit()`: `lastHurtByPlayer` (must resolve to a player), else `lastHurtByMob` (a living entity).
/// The entity that is running right now (the attacker) is taken out of the level: `source` supplies its snapshot.
pub fn resolve_kill_credit(
    level: &ServerLevel,
    victim: &dyn Entity,
    source: Option<&DamageSource>,
) -> Option<DamageEntity> {
    let living = victim.living()?;
    let (uuid, need_player) = match (living.last_hurt_by_player, living.last_hurt_by_mob) {
        (Some(uuid), _) => (uuid, true),
        (None, Some(uuid)) => (uuid, false),
        (None, None) => return None,
    };
    if let Some(e) = level.get_entity_by_uuid(uuid) {
        let ok = if need_player {
            e.is_player()
        } else {
            e.living().is_some()
        };
        return ok.then(|| DamageEntity::of(e));
    }
    let from_source = source
        .into_iter()
        .flat_map(|s| [s.causing_info(), s.direct_info()])
        .flatten()
        .find(|i| i.uuid == uuid)?;
    let ok = if need_player {
        from_source.is_player
    } else {
        from_source.is_living
    };
    ok.then(|| from_source.clone())
}

/// `killer.awardKillScore(victim, source)` (`Entity.awardKillScore` = `ENTITY_KILLED_PLAYER`, then `ServerPlayer`'s scoreboard,
/// stats and `PLAYER_KILLED_ENTITY`). The dying `victim` is out of the level: a killer the level hands out (in storage or
/// lent by its running code) is given it directly.
pub fn award_kill_score_to_killer(
    level: &mut ServerLevel,
    killer_id: crate::world::entity::EntityId,
    victim: &mut dyn Entity,
    snapshot: &DamageEntity,
    source: &DamageSource,
) {
    if killer_id == victim.id() {
        // the dying entity is its own killer (a player killed by its own TNT): `ServerPlayer.awardKillScore` does nothing
        // for `victim == this`, and for any other entity `Entity.awardKillScore` only reacts to a player victim
        return;
    }
    level.with_entity(killer_id, |level, killer| {
        if let Some(player) = victim
            .as_any_mut()
            .downcast_mut::<crate::world::entity::ServerPlayer>()
        {
            crate::advancements::triggers::CriteriaTriggers::ENTITY_KILLED_PLAYER
                .trigger(level, player, &*killer, source);
        }
        killer.award_kill_score(level, snapshot, Some(&*victim), source);
    });
}

/// `LivingEntity.die(source)`.
pub fn die(entity: &mut dyn Entity, level: &mut ServerLevel, source: &DamageSource) {
    if entity.living().is_none() || entity.is_removed() || entity.living().is_some_and(|l| l.dead) {
        return;
    }
    let victim = DamageEntity::of(entity);
    let killer = resolve_kill_credit(level, entity, Some(source));
    if let Some(killer) = &killer {
        award_kill_score_to_killer(level, killer.id, entity, &victim, source);
    }
    if entity.is_sleeping() {
        super::sleeping::stop_sleeping(entity, level);
    }
    super::blocking::stop_using_item(entity, level);
    if entity.base().get_custom_name().is_some() {
        let message = entity
            .living()
            .expect("living")
            .combat_tracker
            .get_death_message(level, entity);
        log::info!(
            "Named entity {}['{}'/{}] died: {}",
            entity.get_type().name(),
            crate::commands::host::entity_plain_text_name(entity),
            entity.id(),
            message.get_string()
        );
    }
    entity.living_mut().expect("living").dead = true; // handleKillingBlow()
    let proceed = match source.get_entity() {
        None => true,
        Some(source_entity) => {
            let (v, s) = (victim.clone(), source.clone());
            // the victim is lent: `killedEntity` may act on it (a zombie converting the villager it killed)
            level
                .lend(&mut *entity, |level| {
                    level.with_entity(source_entity, move |level, e| {
                        e.killed_entity(level, &v, &s)
                    })
                })
                .unwrap_or(true)
        }
    };
    if proceed {
        let position = entity.position();
        level.game_event_from_live_entity(entity, GameEvent::ENTITY_DIE, position);
        entity.drop_all_death_loot(level, source);
        create_wither_rose(entity, level, killer.as_ref());
    }
    broadcast_entity_event(level, entity.id(), 3);
    entity.set_pose(Pose::Dying);
}

/// `LivingEntity.createWitherRose(killer)`.
pub fn create_wither_rose(
    entity: &mut dyn Entity,
    level: &mut ServerLevel,
    killer: Option<&DamageEntity>,
) {
    use crate::world::level::block::state::BlockStateExt as _;
    use crate::world::level::level_access::{BlockGetter as _, LevelAccessor as _};
    if !killer.is_some_and(|k| k.entity_type == EntityType::WITHER) {
        return;
    }
    let mut placed = false;
    if level.mob_griefing() {
        let pos = entity.block_position();
        let state = Block::WITHER_ROSE.default_state();
        if level.get_block_state(pos).is_air() && state.can_survive(level, pos) {
            level.set_block(pos, state, 3, 512); // setBlockAndUpdate
            placed = true;
        }
    }
    if !placed {
        let p = entity.position();
        let item_entity = crate::world::entity::item::ItemEntity::with_stack(
            level,
            p.x,
            p.y,
            p.z,
            ItemStack::new(Item::WITHER_ROSE),
        );
        level.add_fresh_entity(Box::new(item_entity));
    }
}

/// `LivingEntity.dropAllDeathLoot(level, source)`.
pub fn drop_all_death_loot(
    entity: &mut dyn Entity,
    level: &mut ServerLevel,
    source: &DamageSource,
) {
    let player_killed = entity
        .living()
        .expect("living")
        .last_hurt_by_player_memory_time
        > 0;
    if entity.should_drop_loot(level) {
        drop_from_loot_table(entity, level, source, player_killed);
        entity.drop_custom_death_loot(level, source, player_killed);
    }
    entity.drop_equipment(level);
    drop_experience(entity, level, source);
}

/// `LivingEntity.getLootTable()` default: `EntityType.getDefaultLootTable()`.
pub fn default_loot_table(entity: &dyn Entity) -> Option<Identifier> {
    entity
        .get_type()
        .data()
        .loot_table
        .and_then(Identifier::try_parse)
}

/// `LivingEntity.dropFromLootTable(level, source, playerKilled)`.
fn drop_from_loot_table(
    entity: &mut dyn Entity,
    level: &mut ServerLevel,
    source: &DamageSource,
    player_killed: bool,
) {
    let Some(table_id) = entity.get_loot_table() else {
        return;
    };
    let items = loot_table_items(entity, level, source, player_killed, &table_id);
    for stack in items {
        crate::world::entity::item::spawn_at_location(
            level,
            entity,
            stack,
            mc_core::phys::Vec3::ZERO,
        );
    }
}

/// `LivingEntity.dropFromLootTable(level, source, playerKilled, lootTable, consumer)`: the items the table rolls for this
/// entity (the caller is the consumer: it spawns / counts them).
pub fn loot_table_items(
    entity: &mut dyn Entity,
    level: &mut ServerLevel,
    source: &DamageSource,
    player_killed: bool,
    table_id: &Identifier,
) -> Vec<ItemStack> {
    use crate::world::level::storage::loot::{LootContextParamSets, LootParams, loading};
    let table = loading::get_loot_table(&level.registries, table_id);
    let seed = entity.get_loot_table_seed();

    {
        let attacker: Option<&dyn Entity> = source.get_entity().and_then(|id| level.get_entity(id));
        let direct: Option<&dyn Entity> = match source.get_direct_entity() {
            Some(id) if Some(id) == source.get_entity() => attacker,
            Some(id) => level.get_entity(id),
            None => None,
        };
        let killer_uuid = entity.living().expect("living").last_hurt_by_player;
        let killer_player: Option<&dyn Entity> = match killer_uuid {
            Some(uuid) if player_killed => level
                .get_entity_by_uuid(uuid)
                .filter(|e| e.is_player())
                .or_else(|| attacker.filter(|a| a.uuid() == uuid && a.is_player())),
            _ => None,
        };
        let this: &dyn Entity = &*entity;
        let origin = this.position();
        let mut builder = LootParams::builder(level)
            .this_entity(this)
            .origin(origin)
            .damage_source(source)
            .attacking_entity(attacker)
            .direct_attacking_entity(direct);
        if let Some(p) = killer_player {
            let luck = p.living().map_or(0.0, |l| {
                l.get_attribute_value(mc_data::Attribute::LUCK) as f32
            });
            builder = builder.last_damage_player(Some(p)).luck(luck);
        }
        let params = builder.create(&LootContextParamSets::ENTITY);
        table.get_random_items_with_seed(&params, seed)
    }
}

/// `LivingEntity.getExperienceReward(level, killer)` (the killer is the entity of `source`).
pub fn get_experience_reward(
    entity: &mut dyn Entity,
    level: &mut ServerLevel,
    source: Option<&DamageSource>,
) -> i32 {
    let base = entity.get_base_experience_reward(level);
    let victim_id = entity.id();
    let mut actors = Actors::of(&mut *entity);
    crate::world::item::enchantment_helper::process_mob_experience(
        level,
        &mut actors,
        source.and_then(DamageSource::get_entity),
        victim_id,
        base,
    )
}

/// `LivingEntity.dropExperience(level, killer)`.
pub fn drop_experience(entity: &mut dyn Entity, level: &mut ServerLevel, source: &DamageSource) {
    let living = entity.living().expect("living");
    let eligible = !living.skip_drop_experience
        && (entity.is_always_experience_dropper()
            || living.last_hurt_by_player_memory_time > 0
                && entity.should_drop_experience()
                && level.mob_drops());
    if eligible {
        let base = entity.get_base_experience_reward(level);
        let victim_id = entity.id();
        let mut actors = Actors::of(&mut *entity);
        let amount = crate::world::item::enchantment_helper::process_mob_experience(
            level,
            &mut actors,
            source.get_entity(),
            victim_id,
            base,
        );
        let pos = entity.position();
        crate::world::entity::experience_orb::ExperienceOrb::award(level, &pos, amount);
    }
}
