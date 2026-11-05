//! `Player` / `ServerPlayer` damage overrides: `isInvulnerableTo`, `hurtServer`, `actuallyHurt`, `die`, kill score,
//! combat packets, hurt animation, fall damage, death drops. The `impl Entity for ServerPlayer` delegates here; the
//! generic `LivingEntity` behaviour they call as `super` is in `living_entity::{damage, death, blocking}`.

use mc_core::math::mth;
use mc_data::{GameEvent, SoundEvent};
use mc_protocol::packets::play::{ClientboundHurtAnimationPacket, ClientboundPlayerCombatEndPacket, ClientboundPlayerCombatEnterPacket, ClientboundPlayerCombatKillPacket};
use mc_text::Component;
use mc_world::level_storage::Difficulty;

use super::ServerPlayer;
use crate::registry::typed::damage_type::DamageEffects;
use crate::server::level::ServerLevel;
use crate::stats::Stats;
use crate::world::damagesource::{DamageEntity, DamageSource, DamageTypeTags, DamageTypes};
use crate::world::entity::Entity;
use crate::world::entity::equipment::EquipmentSlot;
use crate::world::entity::living_entity::{blocking, damage, death};
use crate::world::scores::team::Visibility;

/// `DamageEffects.sound()`.
pub fn damage_effects_sound(effects: DamageEffects) -> SoundEvent {
    match effects {
        DamageEffects::Hurt | DamageEffects::Thorns => SoundEvent::ENTITY_PLAYER_HURT,
        DamageEffects::Drowning => SoundEvent::ENTITY_PLAYER_HURT_DROWN,
        DamageEffects::Burning => SoundEvent::ENTITY_PLAYER_HURT_ON_FIRE,
        DamageEffects::Poking => SoundEvent::ENTITY_PLAYER_HURT_SWEET_BERRY_BUSH,
        DamageEffects::Freezing => SoundEvent::ENTITY_PLAYER_HURT_FREEZE,
    }
}

impl ServerPlayer {
    /// `Entity.getTeam()`'s name -> `doTeamsAllowDamage(other)` (`team == null || !team.isAlliedTo(otherTeam) || allowFriendlyFire`).
    fn do_teams_allow_damage(&self, level: &ServerLevel, other_scoreboard_name: &str) -> bool {
        let scoreboard = level.scoreboard();
        let Some(team) = scoreboard.get_players_team(&self.player.game_profile.name) else { return true };
        let other_team = scoreboard.get_players_team(other_scoreboard_name);
        !team.is_allied_to(other_team) || team.is_allow_friendly_fire()
    }

    /// `ServerPlayer.canHarmPlayer(target)`: `isPvpAllowed() && Player.canHarmPlayer(target)` for `this` hurting `target`;
    /// here `this` is the victim and `attacker` the player that hurts it (`!this.canHarmPlayer(player)` in `hurtServer`).
    fn can_be_harmed_by_player(&self, level: &ServerLevel, attacker: &DamageEntity) -> bool {
        level.pvp() && self.do_teams_allow_damage(level, &attacker.scoreboard_name)
    }
}

/// `Player.isInvulnerableTo` + `ServerPlayer.isInvulnerableTo`.
pub fn is_invulnerable_to(player: &ServerPlayer, level: &ServerLevel, source: &DamageSource) -> bool {
    // Player: LivingEntity (base + enchantment immunity), then the game rules
    let player_invulnerable = if damage::is_invulnerable_to(player, level, source) {
        true
    } else if source.is_tag(level, DamageTypeTags::IS_DROWNING) {
        !level.drowning_damage()
    } else if source.is_tag(level, DamageTypeTags::IS_FALL) {
        !level.fall_damage()
    } else if source.is_tag(level, DamageTypeTags::IS_FIRE) {
        !level.fire_damage()
    } else if source.is_tag(level, DamageTypeTags::IS_FREEZING) {
        !level.freeze_damage()
    } else {
        false
    };
    player_invulnerable
        || player.is_changing_dimension() && !source.is(DamageTypes::ENDER_PEARL)
        || !player.listener.as_ref().is_some_and(|l| l.has_client_loaded())
}

/// `ServerPlayer.hurtServer(level, source, damage)`.
pub fn hurt_server(player: &mut ServerPlayer, level: &mut ServerLevel, source: &DamageSource, damage: f32) -> bool {
    if player.is_invulnerable_to(level, source) {
        return false;
    }
    // `entity instanceof Player player && !this.canHarmPlayer(player)`; the `AbstractArrow` branch tests the
    // *causing* entity (an arrow never is: its source names the owner), so it cannot apply.
    if let Some(attacker) = source.causing_info().filter(|c| c.is_player)
        && !player.can_be_harmed_by_player(level, attacker)
    {
        return false;
    }
    player_hurt_server(player, level, source, damage)
}

/// `Player.hurtServer(level, source, damage)`.
fn player_hurt_server(player: &mut ServerPlayer, level: &mut ServerLevel, source: &DamageSource, mut damage: f32) -> bool {
    if player.is_invulnerable_to(level, source) {
        return false;
    }
    if player.player.abilities.invulnerable && !source.is_tag(level, DamageTypeTags::BYPASSES_INVULNERABILITY) {
        return false;
    }
    player.player.living.no_action_time = 0;
    if player.is_dead_or_dying() {
        return false;
    }
    // `removeEntitiesOnShoulder()`: see the shoulder-entity gap (EcPlayer).
    if source.scales_with_difficulty() {
        let difficulty = level.difficulty();
        if difficulty == Difficulty::Peaceful {
            damage = 0.0;
        }
        if difficulty == Difficulty::Easy {
            damage = f32::min(damage / 2.0 + 1.0, damage);
        }
        if difficulty == Difficulty::Hard {
            damage = damage * 3.0 / 2.0;
        }
    }
    if damage == 0.0 { false } else { damage::hurt_server(player, level, source, damage) }
}

/// `Player.actuallyHurt(level, source, dmg)`.
pub fn actually_hurt(player: &mut ServerPlayer, level: &mut ServerLevel, source: &DamageSource, dmg: f32) {
    if player.is_invulnerable_to(level, source) {
        return;
    }
    let mut dmg = damage::get_damage_after_armor_absorb(player, level, source, dmg);
    dmg = damage::get_damage_after_magic_absorb(player, level, source, dmg);
    let var8 = (dmg - player.get_absorption_amount()).max(0.0);
    let absorption = player.get_absorption_amount();
    player.set_absorption_amount(absorption - (dmg - var8));
    let absorbed_damage = dmg - var8;
    if absorbed_damage > 0.0 && absorbed_damage < 3.4028235E37 {
        player.award_stat(level, Stats::DAMAGE_ABSORBED, mc_core::math::java::round_f32(absorbed_damage * 10.0));
    }
    if var8 != 0.0 {
        player.cause_food_exhaustion(source.get_food_exhaustion());
        crate::world::entity::living_entity::combat::record_damage(player, level, source, var8);
        let health = player.get_health();
        player.player.living.set_health(health - var8);
        if var8 < 3.4028235E37 {
            player.award_stat(level, Stats::DAMAGE_TAKEN, mc_core::math::java::round_f32(var8 * 10.0));
        }
        player.game_event(level, GameEvent::ENTITY_DAMAGE);
    }
}

/// `Player.hurtArmor` / `hurtHelmet`.
pub fn hurt_armor(player: &mut ServerPlayer, level: &mut ServerLevel, source: &DamageSource, dmg: f32) {
    damage::do_hurt_equipment(player, level, source, dmg, &[EquipmentSlot::Feet, EquipmentSlot::Legs, EquipmentSlot::Chest, EquipmentSlot::Head]);
}

pub fn hurt_helmet(player: &mut ServerPlayer, level: &mut ServerLevel, source: &DamageSource, dmg: f32) {
    damage::do_hurt_equipment(player, level, source, dmg, &[EquipmentSlot::Head]);
}

/// `Player.getHurtSound(source)`: `source.type().effects().sound()`.
pub fn get_hurt_sound(source: &DamageSource) -> SoundEvent {
    damage_effects_sound(source.type_().effects)
}

/// `ServerPlayer.indicateDamage(xd, zd)`.
pub fn indicate_damage(player: &mut ServerPlayer, xd: f64, zd: f64) {
    player.player.hurt_dir = (mth::atan2(zd, xd) * 180.0 / f64::from(std::f32::consts::PI) as f64) as f32 - player.base().y_rot();
    let packet = ClientboundHurtAnimationPacket { id: player.id(), yaw: player.player.hurt_dir };
    if let Some(connection) = &player.connection {
        connection.send(&packet);
    }
}

/// `Player.blockUsingItem`: the base behaviour, then the attacker's weapon may disable the shield.
pub fn block_using_item(player: &mut ServerPlayer, level: &mut ServerLevel, attacker: &DamageEntity, source: &DamageSource, damage: f32, fully_blocked: bool) {
    blocking::block_using_item(player, level, attacker, source, damage, fully_blocked);
    blocking::disable_after_block(player, level, attacker);
}

/// `Player.causeFallDamage(fallDistance, damageModifier, source)`.
pub fn cause_fall_damage(player: &mut ServerPlayer, level: &mut ServerLevel, fall_distance: f64, damage_modifier: f32, source: &DamageSource) -> bool {
    if player.player.abilities.mayfly {
        return false;
    }
    if fall_distance >= 2.0 {
        player.award_stat(level, Stats::FALL_ONE_CM, mc_core::math::java::round_f64(fall_distance * 100.0) as i32);
    }
    damage::cause_fall_damage(player, level, fall_distance, damage_modifier, source)
}

/// `ServerPlayer.onEnterCombat()` / `onLeaveCombat()`.
pub fn on_enter_combat(player: &ServerPlayer) {
    if let Some(connection) = &player.connection {
        connection.send(&ClientboundPlayerCombatEnterPacket);
    }
}

pub fn on_leave_combat(player: &ServerPlayer) {
    let duration = player.player.living.combat_tracker.get_combat_duration(player);
    if let Some(connection) = &player.connection {
        connection.send(&ClientboundPlayerCombatEndPacket { duration });
    }
}

/// `Player.dropEquipment(level)`: unless `keepInventory`, vanishing-cursed items are destroyed and the inventory dropped.
pub fn drop_equipment(player: &mut ServerPlayer, level: &mut ServerLevel) {
    if !level.keep_inventory() {
        destroy_vanishing_cursed_items(player);
        // `Inventory.dropAll()`: `createItemStackToDrop(stack, true, false)` + `addFreshEntity` per stack, in drop order
        for stack in player.player.inventory.drop_all() {
            if let Some(entity) = player.create_item_stack_to_drop(level, stack, true, false) {
                level.add_fresh_entity(Box::new(entity));
            }
        }
    }
}

/// `Player.destroyVanishingCursedItems()`.
fn destroy_vanishing_cursed_items(player: &mut ServerPlayer) {
    use crate::world::container::Container;
    let size = player.player.inventory.get_container_size();
    for i in 0..size {
        let stack = player.player.inventory.get_item(i);
        if !stack.is_empty() && crate::world::item::enchantment::enchantment_helper::has_prevent_equipment_drop(&stack) {
            player.player.inventory.remove_item_no_update(i);
        }
    }
}

/// `Player.getBaseExperienceReward(level)`.
pub fn get_base_experience_reward(player: &ServerPlayer, level: &ServerLevel) -> i32 {
    if !level.keep_inventory() && !player.is_spectator() { i32::min(player.player.experience_level * 7, 100) } else { 0 }
}

/// `ServerPlayer.awardKillScore(victim, killingBlow)`.
pub fn award_kill_score(player: &mut ServerPlayer, level: &mut ServerLevel, victim: &DamageEntity, _killing_blow: &DamageSource) {
    if victim.id == player.id() {
        return;
    }
    // `super.awardKillScore`: `CriteriaTriggers.ENTITY_KILLED_PLAYER` is fired by the advancement slice.
    {
        use crate::world::scores::ObjectiveCriteria;
        let mut scoreboard = level.scoreboard_mut();
        scoreboard.for_all_objectives(&ObjectiveCriteria::KILL_COUNT_ALL, &*player, |score| {
            let _ = score.increment();
        });
        if victim.is_player {
            scoreboard.for_all_objectives(&ObjectiveCriteria::KILL_COUNT_PLAYERS, &*player, |score| {
                let _ = score.increment();
            });
        }
    }
    if victim.is_player {
        player.award_stat(level, Stats::PLAYER_KILLS, 1);
    } else {
        player.award_stat(level, Stats::MOB_KILLS, 1);
    }
    let own_name = player.player.game_profile.name.clone();
    handle_team_kill(level, &own_name, &victim.scoreboard_name, true);
    handle_team_kill(level, &victim.scoreboard_name, &own_name, false);
    // `CriteriaTriggers.PLAYER_KILLED_ENTITY.trigger(this, victim, killingBlow)`: advancement slice.
}

/// `ServerPlayer.handleTeamKill(source, target, criteriaByTeam)`: `source`/`target` are scoreboard names;
/// `team_kill` selects `ObjectiveCriteria.TEAM_KILL` (else `KILLED_BY_TEAM`) keyed by the target's team color.
fn handle_team_kill(level: &ServerLevel, source: &str, target: &str, team_kill: bool) {
    use crate::world::scores::ObjectiveCriteria;
    use crate::world::scores::score::NameOnlyHolder;
    let mut scoreboard = level.scoreboard_mut();
    let color = scoreboard.get_players_team(target).and_then(|t| t.get_color());
    if let Some(color) = color {
        let criteria = if team_kill { ObjectiveCriteria::team_kill(color) } else { ObjectiveCriteria::killed_by_team(color) };
        scoreboard.for_all_objectives(&criteria, &NameOnlyHolder(source.to_owned()), |score| {
            let _ = score.increment();
        });
    }
}

/// `Player.killedEntity(level, victim, source)`: counts the kill, the victim's death proceeds.
pub fn killed_entity(player: &mut ServerPlayer, level: &mut ServerLevel, victim: &DamageEntity) -> bool {
    player.award_stat(level, Stats::killed(victim.entity_type), 1);
    true
}

/// `ServerPlayer.die(source)`.
pub fn die(player: &mut ServerPlayer, level: &mut ServerLevel, source: &DamageSource) {
    player.game_event(level, GameEvent::ENTITY_DIE);
    let show_death_message = level.show_death_messages();
    if show_death_message {
        let tracker = std::mem::take(&mut player.player.living.combat_tracker);
        let death_message = tracker.get_death_message(level, player);
        player.player.living.combat_tracker = tracker;
        if let Some(connection) = &player.connection {
            connection.send(&ClientboundPlayerCombatKillPacket { player_id: player.id(), message: death_message.clone() });
        }
        broadcast_death_message(player, level, &death_message);
    } else if let Some(connection) = &player.connection {
        connection.send(&ClientboundPlayerCombatKillPacket { player_id: player.id(), message: Component::empty() });
    }

    // `removeEntitiesOnShoulder()`: see the shoulder-entity gap (EcPlayer).
    if level.forgive_dead_players() {
        // `tellNeutralMobsThatIDied()`: neutral mobs (`NeutralMob.playerDied`) belong to the mob slice.
    }
    if !player.is_spectator() {
        player.drop_all_death_loot(level, source);
    }
    {
        use crate::world::scores::ObjectiveCriteria;
        level.scoreboard_mut().for_all_objectives(&ObjectiveCriteria::DEATH_COUNT, &*player, |score| {
            let _ = score.increment();
        });
    }
    let killer = death::resolve_kill_credit(level, player, Some(source));
    if let Some(killer) = &killer {
        player.award_stat(level, Stats::killed_by(killer.entity_type), 1);
        let (victim, s) = (DamageEntity::of(player), source.clone());
        level.with_entity_or_defer(killer.id, move |level, e| e.award_kill_score(level, &victim, &s));
        death::create_wither_rose(player, level, Some(killer));
    }
    damage::broadcast_entity_event(level, player.id(), 3);
    player.award_stat(level, Stats::DEATHS, 1);
    player.reset_stat(level, Stats::TIME_SINCE_DEATH);
    player.reset_stat(level, Stats::TIME_SINCE_REST);
    player.clear_fire();
    player.base_mut().set_ticks_frozen(0);
    player.set_shared_flag_on_fire(false);
    crate::world::entity::living_entity::combat::recheck_status(player, level);
    player.player.last_death_location = Some(mc_protocol::codec::GlobalPos { dimension: player.dimension.clone(), pos: player.base().block_position() });
    if let Some(listener) = player.listener.as_mut() {
        listener.mark_client_unloaded_after_death();
    }
}

/// The team visibility rules of `ServerPlayer.die`: `PlayerList.broadcastSystemMessage` / `broadcastSystemToTeam` /
/// `broadcastSystemToAllExceptTeam`.
fn broadcast_death_message(player: &ServerPlayer, level: &ServerLevel, message: &Component) {
    let name = &player.player.game_profile.name;
    let handles = level.all_player_handles();
    let scoreboard = level.scoreboard();
    let team = scoreboard.get_players_team(name);
    match team.map(|t| t.get_death_message_visibility()) {
        None | Some(Visibility::Always) => {
            log::info!("System chat: {}", message.get_string());
            for h in &handles {
                h.send_system_message(message, false);
            }
        }
        Some(Visibility::HideForOtherTeams) => {
            for member in team.expect("team").get_players() {
                if let Some(h) = handles.iter().find(|h| h.name() == member.as_str())
                    && h.id != player.id()
                {
                    h.send_system_message(message, false);
                }
            }
        }
        Some(Visibility::HideForOwnTeam) => {
            for h in &handles {
                if scoreboard.get_players_team(h.name()).map(|t| t.get_name()) != team.map(|t| t.get_name()) {
                    h.send_system_message(message, false);
                }
            }
        }
        Some(Visibility::Never) => {}
    }
}

/// `Entity.snapshot_for_damage()` of a player: a detached `ServerPlayer` carrying what loot/enchantment/predicate
/// evaluation reads of an attacker (position, flags, attributes, effects, equipment, inventory, abilities, game mode).
pub fn snapshot_for_damage(player: &ServerPlayer) -> Box<dyn Entity> {
    use crate::world::entity::entity::DATA_SHARED_FLAGS_ID;
    let mut copy = ServerPlayer::new(player.id(), player.dimension.clone(), player.player.game_profile.clone(), Default::default());
    let (src, dst) = (&player.player, &mut copy.player);
    dst.inventory.equipment_mut().set_all(src.inventory.equipment());
    dst.inventory.replace_with(&src.inventory);
    dst.abilities = src.abilities.clone();
    dst.food_data = src.food_data.clone();
    dst.experience_level = src.experience_level;
    dst.total_experience = src.total_experience;
    dst.experience_progress = src.experience_progress;
    dst.living.attributes = src.living.attributes.clone();
    dst.living.active_effects = src.living.active_effects.clone();
    dst.living.y_head_rot = src.living.y_head_rot;
    dst.living.y_body_rot = src.living.y_body_rot;
    dst.living.use_item = src.living.use_item.clone();
    dst.living.use_item_remaining = src.living.use_item_remaining;
    dst.living.absorption_amount = src.living.absorption_amount;
    dst.internal_set_absorption_amount(src.get_absorption_amount());
    let living_flags = src.living.entity.entity_data.get(crate::world::entity::living::DATA_LIVING_ENTITY_FLAGS);
    dst.living.entity.entity_data.set(crate::world::entity::living::DATA_LIVING_ENTITY_FLAGS, living_flags);
    dst.living.set_health(src.living.get_health());
    let (se, de) = (&src.living.entity, &mut dst.living.entity);
    de.uuid = se.uuid;
    de.entity_data.set(DATA_SHARED_FLAGS_ID, se.entity_data.get(DATA_SHARED_FLAGS_ID));
    de.fall_distance = se.fall_distance;
    de.remaining_fire_ticks = se.remaining_fire_ticks;
    de.tick_count = se.tick_count;
    de.set_custom_name(se.get_custom_name());
    de.set_ticks_frozen(se.get_ticks_frozen());
    de.set_on_ground_raw(se.on_ground());
    de.set_delta_movement(se.delta_movement());
    de.set_y_rot(se.y_rot());
    de.set_x_rot(se.x_rot());
    copy.game_mode = player.game_mode.clone();
    copy.set_pos_vec(player.position());
    Box::new(copy)
}
