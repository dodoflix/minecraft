//! Item blocking (`blocks_attacks`, e.g. shields): `LivingEntity.applyItemBlocking/getItemBlockingWith/blockUsingItem`
//! and `BlocksAttacks.resolveBlockedDamage/hurtBlockingItem/onBlocked/disable`.

use mc_core::math::mth;
use mc_core::phys::Vec3;
use mc_core::random::RandomSource;
use mc_protocol::components::values::combat::{BLOCKS_ATTACKS, BlocksAttacks, WEAPON};

use super::damage::as_server_player;
use crate::server::level::ServerLevel;
use crate::world::damagesource::{DamageEntity, DamageSource, DamageTypeTags};
use crate::world::entity::Entity;
use crate::world::entity::equipment::EquipmentSlot;
use crate::world::item::{ItemStack, behaviour};

/// `BlocksAttacks.blockDelayTicks()`.
fn block_delay_ticks(blocks_attacks: &BlocksAttacks) -> i32 {
    mc_core::math::java::round_f32(blocks_attacks.block_delay_seconds * 20.0)
}

/// `LivingEntity.getItemBlockingWith()`: the stack in use if it blocks attacks and its block delay has elapsed.
pub fn get_item_blocking_with(entity: &dyn Entity) -> Option<ItemStack> {
    let living = entity.living()?;
    if !living.get_living_entity_flag(super::super::living::LIVING_ENTITY_FLAG_IS_USING) {
        return None;
    }
    let blocks_attacks = living.use_item.get(&BLOCKS_ATTACKS)?;
    let elapsed_ticks = behaviour(living.use_item.item()).get_use_duration(&living.use_item)
        - living.use_item_remaining;
    (elapsed_ticks >= block_delay_ticks(blocks_attacks)).then(|| living.use_item.clone())
}

/// `LivingEntity.getUsedItemHand()`.
fn used_item_slot(entity: &dyn Entity) -> EquipmentSlot {
    if entity.living().is_some_and(|l| {
        l.get_living_entity_flag(super::super::living::LIVING_ENTITY_FLAG_OFF_HAND)
    }) {
        EquipmentSlot::OffHand
    } else {
        EquipmentSlot::MainHand
    }
}

/// `LivingEntity.getSecondsToDisableBlocking()`: the `weapon` component of the main hand item when it is the active item.
pub fn seconds_to_disable_blocking(entity: &dyn Entity) -> f32 {
    let Some(living) = entity.living() else {
        return 0.0;
    };
    let weapon_item = entity.get_item_by_slot(EquipmentSlot::MainHand);
    // `weaponItem == this.getActiveItem()`: while an item is in use that is the use item, else the main hand item.
    let using = living.get_living_entity_flag(super::super::living::LIVING_ENTITY_FLAG_IS_USING);
    let active_is_weapon = !using || used_item_slot(entity) == EquipmentSlot::MainHand;
    match weapon_item.get(&WEAPON) {
        Some(weapon) if active_is_weapon && !entity.is_spectator() => {
            weapon.disable_blocking_for_seconds
        }
        _ => 0.0,
    }
}

/// `BlocksAttacks.DamageReduction.resolve` summed and clamped (`resolveBlockedDamage`).
fn resolve_blocked_damage(
    blocks_attacks: &BlocksAttacks,
    level: &ServerLevel,
    source: &DamageSource,
    dealt_damage: f32,
    angle: f64,
) -> f32 {
    let mut blocked_damage = 0.0f32;
    for reduction in &blocks_attacks.damage_reductions {
        let resolved = if angle
            > f64::from((std::f64::consts::PI / 180.0) as f32 * reduction.horizontal_blocking_angle)
            || reduction
                .r#type
                .as_ref()
                .is_some_and(|t| !source.is_in_holder_set(level, t))
        {
            0.0
        } else {
            mth::clamp_f32(
                reduction.base + reduction.factor * dealt_damage,
                0.0,
                dealt_damage,
            )
        };
        blocked_damage += resolved;
    }
    mth::clamp_f32(blocked_damage, 0.0, dealt_damage)
}

/// `LivingEntity.applyItemBlocking(level, source, damage)`: the damage the blocking item absorbed.
pub fn apply_item_blocking(
    entity: &mut dyn Entity,
    level: &mut ServerLevel,
    source: &DamageSource,
    damage: f32,
) -> f32 {
    if damage <= 0.0 {
        return 0.0;
    }
    let Some(blocking_with) = get_item_blocking_with(entity) else {
        return 0.0;
    };
    let Some(blocks_attacks) = blocking_with.get(&BLOCKS_ATTACKS).cloned() else {
        return 0.0;
    };
    if blocks_attacks
        .bypassed_by
        .as_ref()
        .is_some_and(|t| source.is_in_holder_set(level, t))
    {
        return 0.0;
    }
    if source.direct_info().is_some_and(|d| d.pierce_level > 0) {
        return 0.0;
    }
    let angle = match source.get_source_position() {
        Some(source_position) => {
            let view_vector = crate::world::item::calculate_view_vector(
                0.0,
                entity.living().expect("living").y_head_rot,
            );
            let vector_to = source_position.subtract(entity.position());
            let vector_to = Vec3::new(vector_to.x, 0.0, vector_to.z).normalize();
            vector_to.dot(view_vector).acos()
        }
        None => f64::from(std::f32::consts::PI),
    };
    let damage_blocked = resolve_blocked_damage(&blocks_attacks, level, source, damage, angle);
    hurt_blocking_item(entity, level, &blocks_attacks, damage_blocked);
    if damage_blocked > 0.0
        && !source.is_tag(level, DamageTypeTags::IS_PROJECTILE)
        && let Some(direct) = source.direct_info().filter(|d| d.is_living).cloned()
    {
        entity.block_using_item(level, &direct, source, damage, damage_blocked >= damage);
    }
    damage_blocked
}

/// `BlocksAttacks.hurtBlockingItem(level, item, user, hand, damage)`.
fn hurt_blocking_item(
    entity: &mut dyn Entity,
    level: &mut ServerLevel,
    blocks_attacks: &BlocksAttacks,
    damage: f32,
) {
    if !entity.is_player() {
        return;
    }
    let slot = used_item_slot(entity);
    let item = entity.living().expect("living").use_item.clone();
    if let Some(player) = as_server_player(entity) {
        player.award_stat(level, crate::stats::Stat::Used(item.item()), 1);
    }
    let f = &blocks_attacks.item_damage;
    let item_damage = if damage < f.threshold {
        0
    } else {
        mth::floor_f32(f.base + f.factor * damage)
    };
    if item_damage > 0 {
        entity.hurt_and_break_equipment(level, slot, item_damage);
        // the use item aliases the hand stack in vanilla: keep the copy in step
        let in_hand = entity.get_item_by_slot(slot);
        entity.living_mut().expect("living").use_item = in_hand;
    }
}

/// `BlocksAttacks.onBlocked(level, user)`.
pub fn on_blocked(level: &mut ServerLevel, user: &dyn Entity, blocks_attacks: &BlocksAttacks) {
    if let Some(sound) = blocks_attacks
        .block_sound
        .as_ref()
        .and_then(crate::world::item::holder_sound)
    {
        let p = user.position();
        let pitch = 0.8 + level.tick_state.random.next_float() * 0.4;
        level.play_sound_at(
            None,
            p.x,
            p.y,
            p.z,
            sound,
            user.get_sound_source(),
            1.0,
            pitch,
        );
    }
}

/// `BlocksAttacks.disable(level, user, baseSeconds, blockingWith)`.
pub fn disable(
    user: &mut dyn Entity,
    level: &mut ServerLevel,
    blocks_attacks: &BlocksAttacks,
    base_seconds: f32,
    blocking_with: &ItemStack,
) {
    let seconds = base_seconds * blocks_attacks.disable_cooldown_scale;
    let cooldown_ticks = if seconds > 0.0 {
        mc_core::math::java::round_f32(seconds * 20.0)
    } else {
        0
    };
    if cooldown_ticks > 0 {
        if let Some(player) = as_server_player(user) {
            player
                .player
                .cooldowns
                .add_cooldown(blocking_with, cooldown_ticks);
            player.flush_item_cooldown_packets();
        }
        stop_using_item(user, level);
        if let Some(sound) = blocks_attacks
            .disable_sound
            .as_ref()
            .and_then(crate::world::item::holder_sound)
        {
            let p = user.position();
            let pitch = 0.8 + level.tick_state.random.next_float() * 0.4;
            level.play_sound_at(
                None,
                p.x,
                p.y,
                p.z,
                sound,
                user.get_sound_source(),
                0.8,
                pitch,
            );
        }
    }
}

/// `LivingEntity.stopUsingItem()`.
pub fn stop_using_item(entity: &mut dyn Entity, level: &mut ServerLevel) {
    if let Some(player) = as_server_player(entity) {
        player.stop_using_item(level);
    } else if entity.living().is_some() {
        super::using_item::stop_using_item(entity, level);
    }
}

/// `LivingEntity.blockUsingItem(level, attacker, source, damage, fullyBlocked)` = `attacker.blockedByItem(this, ...)`.
pub fn block_using_item(
    entity: &mut dyn Entity,
    _level: &mut ServerLevel,
    attacker: &DamageEntity,
    source: &DamageSource,
    damage: f32,
    fully_blocked: bool,
) {
    if !fully_blocked {
        let p = entity.position();
        entity.knockback(
            0.5,
            p.x - attacker.position.x,
            p.z - attacker.position.z,
            source,
            damage,
            false,
        );
    }
}

/// `Player.blockUsingItem`: after the base behaviour, the attacker's weapon may disable the shield.
pub fn disable_after_block(
    user: &mut dyn Entity,
    level: &mut ServerLevel,
    attacker: &DamageEntity,
) {
    let blocking_with = get_item_blocking_with(user);
    let blocks_attacks = blocking_with
        .as_ref()
        .and_then(|s| s.get(&BLOCKS_ATTACKS).cloned());
    let seconds_to_disable_blocking = attacker.seconds_to_disable_blocking;
    if seconds_to_disable_blocking > 0.0
        && let (Some(blocks_attacks), Some(blocking_with)) = (blocks_attacks, blocking_with)
    {
        disable(
            user,
            level,
            &blocks_attacks,
            seconds_to_disable_blocking,
            &blocking_with,
        );
    }
}
