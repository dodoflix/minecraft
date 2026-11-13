//! `Player.attack(entity)` and everything it uses: attack range / strength / cooldown, enchanted damage, critical and
//! sweep attacks, knockback, item `hurtEnemy`/`postHurtEnemy`, stats and the attack sounds/particles.
//!
//! The attacking player is taken out of the level while it runs; the target is taken out for the duration of the attack
//! (`level.with_entity`), so every other lookup (sweep targets, `Actors`) sees neither of them.

use super::ServerPlayer;
use crate::server::level::ServerLevel;
use crate::server::level::level_events::encode_packet;
use crate::stats::stats::Stats;
use crate::world::damagesource::DamageSource;
use crate::world::entity::decoration::armor_stand::ArmorStand;
use crate::world::entity::equipment::EquipmentSlot;
use crate::world::entity::living_entity::combat::{self, DEG_TO_RAD};
use crate::world::entity::living_entity::damage::as_server_player;
use crate::world::entity::projectile::deflection::{Deflector, ProjectileDeflection};
use crate::world::entity::{Entity, EntityId};
use crate::world::item::enchantment::actors::Actors;
use crate::world::item::{
    InteractionHand, ItemStack, behaviour, enchantment_helper, piercing_weapon,
};
use mc_core::math::mth;
use mc_core::phys::{AABB, Vec3};
use mc_data::{Attribute, MobEffect, ParticleType, SoundEvent};
use mc_protocol::components::values::basic::MINIMUM_ATTACK_CHARGE;
use mc_protocol::components::values::combat::{ATTACK_RANGE, AttackRange, PIERCING_WEAPON};
use mc_protocol::packets::play::{
    ClientboundAnimatePacket, ClientboundSetEntityMotionPacket, ParticleOptions,
};

/// `AttackRange.defaultFor(livingEntity)`.
pub fn default_attack_range(entity_interaction_range: f64) -> AttackRange {
    let range = entity_interaction_range as f32;
    AttackRange {
        min_reach: 0.0,
        max_reach: range,
        min_creative_reach: 0.0,
        max_creative_reach: range,
        hitbox_margin: 0.0,
        mob_factor: 1.0,
    }
}

/// `LivingEntity.getAttackRangeWith(weaponItem)`.
pub fn get_attack_range_with(entity: &dyn Entity, weapon_item: &ItemStack) -> AttackRange {
    weapon_item.get(&ATTACK_RANGE).cloned().unwrap_or_else(|| {
        default_attack_range(
            entity
                .living()
                .expect("living")
                .get_attribute_value(Attribute::ENTITY_INTERACTION_RANGE),
        )
    })
}

/// `AttackRange.isInRange(attacker, boundingBox, extraBuffer)` for a player (`effectiveMin/MaxRange(Player)`).
pub fn is_in_range(
    range: &AttackRange,
    creative: bool,
    eye_position: Vec3,
    bounding_box: &AABB,
    extra_buffer: f64,
) -> bool {
    let distance = bounding_box.distance_to_sqr(eye_position).sqrt();
    let (min, max) = if creative {
        (range.min_creative_reach, range.max_creative_reach)
    } else {
        (range.min_reach, range.max_reach)
    };
    let min_reach = f64::from(min) - f64::from(range.hitbox_margin) - extra_buffer;
    let max_reach = f64::from(max) + f64::from(range.hitbox_margin) + extra_buffer;
    distance >= min_reach && distance <= max_reach
}

impl ServerPlayer {
    /// `Player.getCurrentItemAttackStrengthDelay()`.
    pub fn get_current_item_attack_strength_delay(&self) -> f32 {
        (1.0 / self
            .player
            .living
            .get_attribute_value(Attribute::ATTACK_SPEED)
            * 20.0) as f32
    }

    /// `Player.cannotAttackWithItem(itemStack, tolerance)`.
    pub fn cannot_attack_with_item(&self, item_stack: &ItemStack, tolerance: i32) -> bool {
        let required_strength = *item_stack.get_or_default(&MINIMUM_ATTACK_CHARGE, &0.0);
        let optimistic_strength = (self
            .player
            .living
            .attack_strength_ticker
            .wrapping_add(tolerance)) as f32
            / self.get_current_item_attack_strength_delay();
        required_strength > 0.0 && optimistic_strength < required_strength
    }

    /// `Player.getAttackStrengthScale(a)`.
    pub fn get_attack_strength_scale(&self, a: f32) -> f32 {
        mth::clamp_f32(
            (self.player.living.attack_strength_ticker as f32 + a)
                / self.get_current_item_attack_strength_delay(),
            0.0,
            1.0,
        )
    }

    /// `Player.resetAttackStrengthTicker()`.
    pub fn reset_attack_strength_ticker(&mut self) {
        self.player.living.attack_strength_ticker = 0;
        self.player.living.item_swap_ticker = 0;
    }

    /// `LivingEntity.getAttackRangeWith(weaponItem)`.
    pub fn get_attack_range_with(&self, weapon_item: &ItemStack) -> AttackRange {
        get_attack_range_with(self, weapon_item)
    }

    /// `Player.isWithinAttackRange(weaponItem, aabb, buffer)`.
    pub fn is_within_attack_range(
        &self,
        weapon_item: &ItemStack,
        aabb: &AABB,
        buffer: f64,
    ) -> bool {
        is_in_range(
            &self.get_attack_range_with(weapon_item),
            self.player.abilities.instabuild,
            self.base().get_eye_position(),
            aabb,
            buffer,
        )
    }

    /// `mainHandItem.has(DataComponents.PIERCING_WEAPON)`.
    pub fn main_hand_has_piercing_weapon(&self) -> bool {
        self.get_item_by_slot(EquipmentSlot::MainHand)
            .get(&PIERCING_WEAPON)
            .is_some()
    }

    /// `Player.isMobilityRestricted()`.
    fn is_mobility_restricted(&self) -> bool {
        self.has_effect(MobEffect::BLINDNESS)
    }

    /// `Player.attack(entity)`: the entity is looked up in the level (a missing entity is ignored).
    pub fn attack(&mut self, level: &mut ServerLevel, target: EntityId) {
        level.with_entity(target, |level, entity| self.attack_entity(level, entity));
    }

    /// `Player.attack(entity)` with the target already taken out of the level.
    pub fn attack_entity(&mut self, level: &mut ServerLevel, entity: &mut dyn Entity) {
        if self.cannot_attack(level, entity) {
            return;
        }
        let mut base_damage = if self.player.living.get_living_entity_flag(4) {
            self.player.living.auto_spin_attack_dmg
        } else {
            self.player
                .living
                .get_attribute_value(Attribute::ATTACK_DAMAGE) as f32
        };
        let attacking_item_stack = Entity::get_weapon_item(self).unwrap_or_else(ItemStack::empty);
        let damage_source = combat::item_stack_damage_source(level, &attacking_item_stack, &*self);
        let attack_strength_scale = self.get_attack_strength_scale(0.5);
        let magic_boost = attack_strength_scale
            * (self.get_enchanted_damage(level, entity, base_damage, &damage_source) - base_damage);
        base_damage *= self.base_damage_scale_factor();
        Entity::on_attack(self);
        if !self.deflect_projectile(level, entity) {
            if base_damage > 0.0 || magic_boost > 0.0 {
                let full_strength_attack = attack_strength_scale > 0.9;
                let knockback_attack = if Entity::is_sprinting(self) && full_strength_attack {
                    self.play_server_side_sound(level, SoundEvent::ENTITY_PLAYER_ATTACK_KNOCKBACK);
                    true
                } else {
                    false
                };
                base_damage += behaviour(attacking_item_stack.item()).get_attack_damage_bonus_for(
                    level,
                    &attacking_item_stack,
                    &mut *entity,
                    base_damage,
                    &damage_source,
                    &mut *self,
                );
                let critical_attack =
                    full_strength_attack && self.can_critical_attack(level, entity);
                if critical_attack {
                    base_damage *= 1.5;
                }
                let total_damage = base_damage + magic_boost;
                let sweep_attack = self.is_sweep_attack(
                    level,
                    full_strength_attack,
                    critical_attack,
                    knockback_attack,
                );
                let old_living_entity_health = entity.living().map_or(0.0, |l| l.get_health());
                let old_movement = entity.base().delta_movement();
                let was_hurt = level.lend(&mut *self, |level| {
                    entity.hurt_server(level, &damage_source, total_damage)
                });
                if was_hurt {
                    let knockback =
                        combat::get_knockback(&mut *self, level, &mut *entity, &damage_source)
                            + if knockback_attack { 0.5 } else { 0.0 };
                    Entity::cause_extra_knockback(
                        self,
                        &mut *entity,
                        knockback,
                        old_movement,
                        &damage_source,
                        total_damage,
                        true,
                    );
                    if sweep_attack {
                        self.do_sweep_attack(
                            level,
                            entity,
                            base_damage,
                            &damage_source,
                            attack_strength_scale,
                        );
                    }
                    self.attack_visual_effects(
                        level,
                        entity,
                        critical_attack,
                        sweep_attack,
                        full_strength_attack,
                        false,
                        magic_boost,
                    );
                    Entity::set_last_hurt_mob(self, &*entity);
                    self.item_attack_interaction(
                        level,
                        entity,
                        attacking_item_stack,
                        &damage_source,
                        true,
                        None,
                    );
                    self.damage_stats_and_hearts(level, entity, old_living_entity_health);
                    self.cause_food_exhaustion(0.1);
                } else {
                    self.play_server_side_sound(level, SoundEvent::ENTITY_PLAYER_ATTACK_NODAMAGE);
                }
            }
            combat::post_piercing_attack(self, level);
        }
    }

    /// `Player.stabAttack(slot, target, baseDamage, dealsDamage, dealsKnockback, dismounts)`; `target` is taken out of the level.
    pub fn stab_attack(
        &mut self,
        level: &mut ServerLevel,
        slot: EquipmentSlot,
        target: &mut dyn Entity,
        mut base_damage: f32,
        deals_damage: bool,
        deals_knockback: bool,
        dismounts: bool,
    ) -> bool {
        if self.cannot_attack(level, target) {
            return false;
        }
        let weapon_item = self.get_item_by_slot(slot);
        let damage_source = combat::item_stack_damage_source(level, &weapon_item, &*self);
        let mut magic_boost =
            self.get_enchanted_damage(level, target, base_damage, &damage_source) - base_damage;
        if !self.is_using_item() || self.get_used_item_hand().as_equipment_slot() != slot {
            magic_boost *= self.get_attack_strength_scale(0.5);
            base_damage *= self.base_damage_scale_factor();
        }
        if deals_knockback && self.deflect_projectile(level, target) {
            return true;
        }
        let total_damage = if deals_damage {
            base_damage + magic_boost
        } else {
            0.0
        };
        let old_living_entity_health = target.living().map_or(0.0, |l| l.get_health());
        let old_movement = target.base().delta_movement();
        let was_hurt = deals_damage
            && level.lend(&mut *self, |level| {
                target.hurt_server(level, &damage_source, total_damage)
            });
        if deals_knockback {
            Entity::cause_extra_knockback(
                self,
                &mut *target,
                0.4,
                old_movement,
                &damage_source,
                total_damage,
                false,
            );
            let knockback = combat::get_knockback(&mut *self, level, &mut *target, &damage_source);
            Entity::cause_extra_knockback(
                self,
                &mut *target,
                knockback,
                old_movement,
                &damage_source,
                total_damage,
                true,
            );
        }
        let dismounted = dismounts && piercing_weapon::dismount(level, target);
        if !was_hurt && !deals_knockback && !dismounted {
            return false;
        }
        self.attack_visual_effects(level, target, false, false, deals_damage, true, magic_boost);
        Entity::set_last_hurt_mob(self, &*target);
        let hand = if slot == EquipmentSlot::OffHand {
            InteractionHand::OffHand
        } else {
            InteractionHand::MainHand
        };
        self.item_attack_interaction(
            level,
            target,
            weapon_item,
            &damage_source,
            was_hurt,
            Some(hand),
        );
        self.damage_stats_and_hearts(level, target, old_living_entity_health);
        self.cause_food_exhaustion(0.1);
        true
    }

    /// `Player.playServerSideSound(sound)`.
    fn play_server_side_sound(&self, level: &mut ServerLevel, sound: SoundEvent) {
        let p = self.base().position();
        level.play_sound_at(
            None,
            p.x,
            p.y,
            p.z,
            sound,
            self.get_sound_source(),
            1.0,
            1.0,
        );
    }

    /// `Player.cannotAttack(entity)`.
    fn cannot_attack(&self, level: &mut ServerLevel, entity: &mut dyn Entity) -> bool {
        !entity.is_attackable()
            || level.expose(self, |level| entity.skip_attack_interaction(level, self))
    }

    /// `ServerPlayer.getEnchantedDamage(entity, dmg, damageSource)`.
    pub fn get_enchanted_damage(
        &mut self,
        level: &mut ServerLevel,
        entity: &mut dyn Entity,
        dmg: f32,
        damage_source: &DamageSource,
    ) -> f32 {
        let weapon = Entity::get_weapon_item(self).unwrap_or_else(ItemStack::empty);
        let victim = entity.id();
        enchantment_helper::modify_damage(
            level,
            &mut Actors::of(self).and(entity),
            &weapon,
            victim,
            damage_source,
            dmg,
        )
    }

    /// `Player.baseDamageScaleFactor()`.
    fn base_damage_scale_factor(&self) -> f32 {
        let attack_strength_scale = self.get_attack_strength_scale(0.5);
        0.2 + attack_strength_scale * attack_strength_scale * 0.8
    }

    /// `Player.deflectProjectile(entity)`: a redirectable projectile is aimed along the player's view.
    fn deflect_projectile(&mut self, level: &mut ServerLevel, entity: &mut dyn Entity) -> bool {
        if !crate::world::entity::projectile::is_pickable(entity.get_type()) {
            return false;
        }
        let deflector = Deflector {
            look_angle: self.get_look_angle(),
            known_speed: Entity::get_known_speed(self, level),
        };
        let owner = self.uuid();
        let Some(projectile) = entity.as_projectile_mut() else {
            return false;
        };
        if crate::world::entity::projectile::deflect_by(
            projectile,
            level,
            ProjectileDeflection::AimDeflect,
            Some(deflector),
            Some(owner),
            true,
            Vec3::new(1.0, 1.0, 1.0),
        ) {
            self.play_server_side_sound(level, SoundEvent::ENTITY_PLAYER_ATTACK_NODAMAGE);
            true
        } else {
            false
        }
    }

    /// `Player.canCriticalAttack(entity)`.
    fn can_critical_attack(&mut self, level: &ServerLevel, entity: &dyn Entity) -> bool {
        self.base().fall_distance > 0.0
            && !self.base().on_ground()
            && !crate::world::entity::living_entity::travel::climbable(self, level)
            && !Entity::is_in_water(self)
            && !self.is_mobility_restricted()
            && !self.base().is_passenger()
            && entity.living().is_some()
            && !Entity::is_sprinting(self)
    }

    /// `Player.isSweepAttack(fullStrengthAttack, criticalAttack, knockbackAttack)`.
    fn is_sweep_attack(
        &self,
        level: &ServerLevel,
        full_strength_attack: bool,
        critical_attack: bool,
        knockback_attack: bool,
    ) -> bool {
        if full_strength_attack && !critical_attack && !knockback_attack && self.base().on_ground()
        {
            let approximate_speed_sq =
                Entity::get_known_movement(self, level).horizontal_distance_sqr();
            let max_speed_for_sweep_attack = f64::from(Entity::get_speed(self)) * 2.5;
            if approximate_speed_sq < mth::square_f64(max_speed_for_sweep_attack) {
                let main_hand = self.get_item_in_hand(InteractionHand::MainHand);
                return !main_hand.is_empty()
                    && mc_data::tags::get::<mc_data::Item>("swords")
                        .is_some_and(|t| t.contains(main_hand.item()));
            }
        }
        false
    }

    /// `Player.attackVisualEffects(...)`.
    fn attack_visual_effects(
        &self,
        level: &mut ServerLevel,
        entity: &dyn Entity,
        critical_attack: bool,
        sweep_attack: bool,
        full_strength_attack: bool,
        stab_attack: bool,
        magic_boost: f32,
    ) {
        if critical_attack {
            self.play_server_side_sound(level, SoundEvent::ENTITY_PLAYER_ATTACK_CRIT);
            self.send_animate(level, entity, 1);
        }
        if !critical_attack && !sweep_attack && !stab_attack {
            self.play_server_side_sound(
                level,
                if full_strength_attack {
                    SoundEvent::ENTITY_PLAYER_ATTACK_STRONG
                } else {
                    SoundEvent::ENTITY_PLAYER_ATTACK_WEAK
                },
            );
        }
        if magic_boost > 0.0 {
            self.send_animate(level, entity, 2);
        }
    }

    /// `ServerPlayer.crit(entity)` (`action` 1) / `magicCrit(entity)` (2): to the tracking players and the player itself.
    fn send_animate(&self, level: &ServerLevel, entity: &dyn Entity, action: u8) {
        let payload = encode_packet(&ClientboundAnimatePacket {
            id: entity.id(),
            action,
        });
        level
            .entity_tracker
            .send_to_tracking_players_and_self(self.base_id(), &payload);
    }

    /// `Player.damageStatsAndHearts(entity, oldLivingEntityHealth)`.
    fn damage_stats_and_hearts(
        &mut self,
        level: &mut ServerLevel,
        entity: &dyn Entity,
        old_living_entity_health: f32,
    ) {
        if let Some(living) = entity.living() {
            let actual_damage = old_living_entity_health - living.get_health();
            self.award_stat(
                level,
                Stats::DAMAGE_DEALT,
                mc_core::math::java::round_f32(actual_damage * 10.0),
            );
            if actual_damage > 2.0 {
                let count = (f64::from(actual_damage) * 0.5) as i32;
                let b = entity.base();
                level.send_particles(
                    ParticleOptions::Simple(ParticleType::DAMAGE_INDICATOR),
                    b.x(),
                    b.get_y_progress(0.5),
                    b.z(),
                    count,
                    0.1,
                    0.0,
                    0.1,
                    0.2,
                );
            }
        }
    }

    /// `Player.itemAttackInteraction(entity, attackingItemStack, damageSource, applyToTarget)`. `attacking` is the weapon as it was
    /// when the attack began (a copy of the stack in the hand, or of the riptide item); `weapon_hand` is the hand holding it
    /// when the caller knows (a stab), otherwise it is derived like vanilla's identity test against the main hand.
    fn item_attack_interaction(
        &mut self,
        level: &mut ServerLevel,
        entity: &mut dyn Entity,
        attacking: ItemStack,
        damage_source: &DamageSource,
        apply_to_target: bool,
        weapon_hand: Option<InteractionHand>,
    ) {
        let (from_slot, hand) = match weapon_hand {
            Some(hand) => (true, hand),
            None => {
                // `attackingItemStack == this.getMainHandItem()` (object identity): only false for a riptide item that is not the held stack
                let main_hand = self.get_item_by_slot(EquipmentSlot::MainHand);
                let from_main_hand = !(self.player.living.get_living_entity_flag(4)
                    && self
                        .player
                        .living
                        .auto_spin_attack_item_stack
                        .as_ref()
                        .is_some_and(|s| {
                            !(ItemStack::is_same_item_same_components(s, &main_hand)
                                && s.count() == main_hand.count())
                        }));
                let hand = if from_main_hand {
                    InteractionHand::MainHand
                } else {
                    InteractionHand::OffHand
                };
                (from_main_hand, hand)
            }
        };
        let mut stack = attacking;
        let target_living = entity.living().is_some();
        let mut item_hurt_enemy = false;
        if target_living {
            item_hurt_enemy =
                crate::world::item::hurt_enemy(level, &mut stack, &mut *entity, &mut *self);
            if item_hurt_enemy {
                self.award_stat_once(level, crate::stats::Stat::Used(stack.item()));
            }
            self.write_weapon_back(from_slot, hand, &stack);
        }
        if apply_to_target {
            let victim = entity.id();
            enchantment_helper::do_post_attack_effects_with_item_source(
                level,
                &mut Actors::of(self).and(&mut *entity),
                victim,
                damage_source,
                Some(&stack),
            );
        }
        if !stack.is_empty() && target_living {
            if item_hurt_enemy {
                if from_slot {
                    // an enchantment effect may have changed the held stack (`EnchantedItemInUse` write-back)
                    stack = self.get_item_by_slot(hand.as_equipment_slot());
                }
                crate::world::item::post_hurt_enemy(&mut stack, level, &mut *entity, &mut *self);
                self.write_weapon_back(from_slot, hand, &stack);
            }
            if stack.is_empty() {
                self.set_item_in_hand(hand, ItemStack::empty());
            }
        }
    }

    /// Stores the weapon stack back where it came from (the hand stack is the same object in vanilla).
    fn write_weapon_back(&mut self, from_slot: bool, hand: InteractionHand, stack: &ItemStack) {
        if from_slot {
            self.player
                .inventory
                .equipment_set(hand.as_equipment_slot(), stack.clone());
        }
    }

    /// `Player.doSweepAttack(entity, baseDamage, damageSource, attackStrengthScale)`.
    fn do_sweep_attack(
        &mut self,
        level: &mut ServerLevel,
        entity: &mut dyn Entity,
        base_damage: f32,
        damage_source: &DamageSource,
        attack_strength_scale: f32,
    ) {
        self.play_server_side_sound(level, SoundEvent::ENTITY_PLAYER_ATTACK_SWEEP);
        let sweep_damage = 1.0
            + self
                .player
                .living
                .get_attribute_value(Attribute::SWEEPING_DAMAGE_RATIO) as f32
                * base_damage;
        let area = entity.get_bounding_box().inflate(1.0, 0.25, 1.0);
        let (me, target) = (self.id(), entity.id());
        let nearby_ids = level.get_entities(None, &area, |e| e.living().is_some());
        let y_rot = self.base().y_rot();
        for nearby_id in nearby_ids {
            if nearby_id == me || nearby_id == target {
                continue;
            }
            let (sin, cos) = (
                f64::from(mth::sin(f64::from(y_rot * DEG_TO_RAD))),
                f64::from(-mth::cos(f64::from(y_rot * DEG_TO_RAD))),
            );
            level.with_entity(nearby_id, |level, nearby| {
                let allied =
                    crate::world::effect::mob_effect_util::is_allied_to(level, &*self, &*nearby);
                let marker_stand = nearby
                    .as_any()
                    .downcast_ref::<ArmorStand>()
                    .is_some_and(|a| a.is_marker());
                if !allied
                    && !marker_stand
                    && self.base().distance_to_sqr_vec(nearby.position()) < 9.0
                {
                    let enchanted_damage =
                        self.get_enchanted_damage(level, &mut *nearby, sweep_damage, damage_source)
                            * attack_strength_scale;
                    if level.lend(&mut *self, |level| {
                        nearby.hurt_server(level, damage_source, enchanted_damage)
                    }) {
                        nearby.knockback(
                            0.4f32 as f64,
                            sin,
                            cos,
                            damage_source,
                            enchanted_damage,
                            false,
                        );
                        let id = nearby.id();
                        enchantment_helper::do_post_attack_effects(
                            level,
                            &mut Actors::of(&mut *self).and(&mut *nearby),
                            id,
                            damage_source,
                        );
                    }
                }
            });
        }
        let dx = -f64::from(mth::sin(f64::from(y_rot * DEG_TO_RAD)));
        let dz = f64::from(mth::cos(f64::from(y_rot * DEG_TO_RAD)));
        let b = self.base();
        level.send_particles(
            ParticleOptions::Simple(ParticleType::SWEEP_ATTACK),
            b.x() + dx,
            b.get_y_progress(0.5),
            b.z() + dz,
            0,
            dx,
            0.0,
            dz,
            0.0,
        );
    }
}

/// `Player.causeExtraKnockback(entity, knockbackAmount, oldMovement, damageSource, damage, comesFromEffect)`.
pub fn cause_extra_knockback(
    player: &mut ServerPlayer,
    entity: &mut dyn Entity,
    knockback_amount: f32,
    old_movement: Vec3,
    damage_source: &DamageSource,
    damage: f32,
    comes_from_effect: bool,
) {
    if knockback_amount > 0.0 {
        let y_rot = player.base().y_rot();
        let sin = mth::sin(f64::from(y_rot * DEG_TO_RAD));
        let cos = mth::cos(f64::from(y_rot * DEG_TO_RAD));
        if entity.living().is_some() {
            entity.knockback(
                f64::from(knockback_amount),
                f64::from(sin),
                f64::from(-cos),
                damage_source,
                damage,
                comes_from_effect,
            );
        } else {
            entity.push_xyz(
                f64::from(-sin * knockback_amount),
                0.1,
                f64::from(cos * knockback_amount),
            );
        }
        let m = player.base().delta_movement();
        player
            .base_mut()
            .set_delta_movement(m.multiply(Vec3::new(0.6, 1.0, 0.6)));
        Entity::set_sprinting(player, false);
    }
    if entity.base().sync_velocity
        && let Some(victim) = as_server_player(entity)
    {
        let packet = ClientboundSetEntityMotionPacket {
            id: victim.base_id(),
            movement: mc_protocol::codec::LpVec3(victim.base().delta_movement()),
        };
        victim.send(&packet);
        victim.base_mut().sync_velocity = false;
        victim.base_mut().set_delta_movement(old_movement);
    }
}
