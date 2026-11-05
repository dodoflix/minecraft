//! `LivingEntity.die`, death loot and experience drops, kill credit.

use mc_core::ident::Identifier;
use mc_data::enums::Pose;
use mc_data::{Block, EntityType, GameEvent, Item};

use super::damage::broadcast_entity_event;
use crate::server::level::ServerLevel;
use crate::world::damagesource::{DamageEntity, DamageSource};
use crate::world::entity::Entity;
use crate::world::item::ItemStack;

/// `LivingEntity.getKillCredit()`: `lastHurtByPlayer` (must resolve to a player), else `lastHurtByMob` (a living entity).
/// The entity that is running right now (the attacker) is taken out of the level: `source` supplies its snapshot.
pub fn resolve_kill_credit(level: &ServerLevel, victim: &dyn Entity, source: Option<&DamageSource>) -> Option<DamageEntity> {
    let living = victim.living()?;
    let (uuid, need_player) = match (living.last_hurt_by_player, living.last_hurt_by_mob) {
        (Some(uuid), _) => (uuid, true),
        (None, Some(uuid)) => (uuid, false),
        (None, None) => return None,
    };
    if let Some(e) = level.get_entity_by_uuid(uuid) {
        let ok = if need_player { e.is_player() } else { e.living().is_some() };
        return ok.then(|| DamageEntity::of(e));
    }
    let from_source = source.into_iter().flat_map(|s| [s.causing_info(), s.direct_info()]).flatten().find(|i| i.uuid == uuid)?;
    let ok = if need_player { from_source.is_player } else { from_source.is_living };
    ok.then(|| from_source.clone())
}

/// `LivingEntity.die(source)`.
pub fn die(entity: &mut dyn Entity, level: &mut ServerLevel, source: &DamageSource) {
    if entity.living().is_none() || entity.is_removed() || entity.living().is_some_and(|l| l.dead) {
        return;
    }
    let victim = DamageEntity::of(entity);
    let killer = resolve_kill_credit(level, entity, Some(source));
    if let Some(killer) = &killer {
        let (v, s) = (victim.clone(), source.clone());
        level.with_entity_or_defer(killer.id, move |level, killer_entity| killer_entity.award_kill_score(level, &v, &s));
    }
    if entity.living().is_some_and(|l| l.is_sleeping()) {
        super::sleeping::stop_sleeping(entity, level);
    }
    super::blocking::stop_using_item(entity, level);
    if entity.base().get_custom_name().is_some() {
        let message = entity.living().expect("living").combat_tracker.get_death_message(level, entity);
        log::info!("Named entity {}['{}'/{}] died: {}", entity.get_type().name(), crate::commands::host::entity_plain_text_name(entity), entity.id(), message.get_string());
    }
    entity.living_mut().expect("living").dead = true; // handleKillingBlow()
    let proceed = match source.get_entity() {
        None => true,
        Some(source_entity) => {
            let (v, s) = (victim.clone(), source.clone());
            level.with_entity_or_defer(source_entity, move |level, e| e.killed_entity(level, &v, &s)).unwrap_or(true)
        }
    };
    if proceed {
        entity.game_event(level, GameEvent::ENTITY_DIE);
        entity.drop_all_death_loot(level, source);
        create_wither_rose(entity, level, killer.as_ref());
    }
    broadcast_entity_event(level, entity.id(), 3);
    entity.set_pose(Pose::Dying);
}

/// `LivingEntity.createWitherRose(killer)`.
pub fn create_wither_rose(entity: &mut dyn Entity, level: &mut ServerLevel, killer: Option<&DamageEntity>) {
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
        let item_entity = crate::world::entity::item::ItemEntity::with_stack(level, p.x, p.y, p.z, ItemStack::new(Item::WITHER_ROSE));
        level.add_fresh_entity(Box::new(item_entity));
    }
}

/// `LivingEntity.dropAllDeathLoot(level, source)`.
pub fn drop_all_death_loot(entity: &mut dyn Entity, level: &mut ServerLevel, source: &DamageSource) {
    let player_killed = entity.living().expect("living").last_hurt_by_player_memory_time > 0;
    if entity.should_drop_loot(level) {
        drop_from_loot_table(entity, level, source, player_killed);
        entity.drop_custom_death_loot(level, source, player_killed);
    }
    entity.drop_equipment(level);
    drop_experience(entity, level, source.get_entity());
}

/// `LivingEntity.getLootTable()` default: `EntityType.getDefaultLootTable()`.
pub fn default_loot_table(entity: &dyn Entity) -> Option<Identifier> {
    entity.get_type().data().loot_table.and_then(Identifier::try_parse)
}

/// `LivingEntity.dropFromLootTable(level, source, playerKilled)`.
fn drop_from_loot_table(entity: &mut dyn Entity, level: &mut ServerLevel, source: &DamageSource, player_killed: bool) {
    let Some(table) = entity.get_loot_table() else { return };
    let items = crate::world::level::storage::loot::entity_drops::death_loot(level, entity, source, player_killed, &table, entity.get_loot_table_seed());
    for stack in items {
        crate::world::entity::item::spawn_at_location(level, entity, stack, mc_core::phys::Vec3::ZERO);
    }
}

/// `LivingEntity.dropExperience(level, killer)`.
pub fn drop_experience(entity: &mut dyn Entity, level: &mut ServerLevel, killer: Option<crate::world::entity::EntityId>) {
    let living = entity.living().expect("living");
    let eligible = !living.skip_drop_experience
        && (entity.is_always_experience_dropper() || living.last_hurt_by_player_memory_time > 0 && entity.should_drop_experience() && level.mob_drops());
    if eligible {
        let base = entity.get_base_experience_reward(level);
        let killer_entity = killer.and_then(|id| level.get_entity(id));
        let amount = crate::world::item::enchantment::enchantment_helper::process_mob_experience(level, killer_entity, entity, base);
        let pos = entity.position();
        crate::world::entity::experience_orb::ExperienceOrb::award(level, &pos, amount);
    }
}
