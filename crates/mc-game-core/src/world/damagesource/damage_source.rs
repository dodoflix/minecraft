//! `net.minecraft.world.damagesource.DamageSource`.
//!
//! Vanilla holds `Entity` references; here entities are looked up by id and the entity that is currently running
//! (an attacking player or mob) is taken out of the level, so a source keeps the ids *and* a [`DamageEntity`]
//! snapshot of what vanilla reads from the references (position, name, main hand item, kind).

use std::rc::Rc;
use std::sync::Arc;

use mc_core::ident::Identifier;
use mc_core::phys::Vec3;
use mc_data::EntityType;
use mc_text::Component;
use uuid::Uuid;

use crate::registry::typed::damage_type::{DamageScaling, DamageType};
use crate::server::level::ServerLevel;
use crate::world::entity::equipment::EquipmentSlot;
use crate::world::entity::{Entity, EntityId};
use crate::world::item::{ItemStack, ItemStackExt as _};

/// A copy of an entity taken when a [`DamageSource`] is built (`Entity::snapshot_for_damage`): the attacker that is
/// running is taken out of the level, but loot/xp evaluation at the victim's death still reads its equipment,
/// attributes and effects.
#[derive(Clone)]
pub struct EntitySnapshot(pub Rc<std::cell::RefCell<Box<dyn Entity>>>);

impl std::fmt::Debug for EntitySnapshot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "EntitySnapshot(#{})", self.0.borrow().id())
    }
}

/// What a [`DamageSource`] needs to know about its direct/causing entity, captured when the source is built.
#[derive(Clone, Debug)]
pub struct DamageEntity {
    pub id: EntityId,
    pub uuid: Uuid,
    pub entity_type: EntityType,
    pub position: Vec3,
    /// `getEyePosition()` and `getLookAngle()` when captured (`SulfurCube.knockback` reads the attacker's aim).
    pub eye_position: Vec3,
    pub look_angle: Vec3,
    /// `getName()` (not team-decorated: the scoreboard is applied when the message is built).
    pub name: Component,
    pub scoreboard_name: String,
    /// `instanceof LivingEntity`.
    pub is_living: bool,
    /// `instanceof Player`.
    pub is_player: bool,
    /// `instanceof Player && abilities.instabuild`.
    pub instabuild: bool,
    /// `instanceof Player && abilities.mayBuild`.
    pub may_build: bool,
    /// `getMainHandItem()` for living entities, empty otherwise.
    pub main_hand_item: ItemStack,
    /// `getWeaponItem()`.
    pub weapon_item: Option<ItemStack>,
    /// `getSecondsToDisableBlocking()` (living entities).
    pub seconds_to_disable_blocking: f32,
    /// `AbstractArrow.getPierceLevel()` (0 for everything else).
    pub pierce_level: i32,
    /// `Projectile.calculateHorizontalHurtKnockbackDirection` (`None` for non-projectiles).
    pub projectile_hurt_knockback: Option<HurtKnockbackDirection>,
    /// A tamed wolf's `getOwnerReference()` (`Some(None)`: tamed without owner), `None` for everything else.
    pub tamed_wolf_owner: Option<Option<Uuid>>,
    /// `this instanceof AbstractThrownPotion`: whether its potion `is(HURTS_WATER_SENSITIVE_ENTITIES)` (`EnderMan.hurtServer`).
    pub thrown_potion_hurts_water_sensitive: Option<bool>,
    /// Full copy of the entity at the time the source was built (`None` for classes without `snapshot_for_damage`).
    pub snapshot: Option<EntitySnapshot>,
}

/// `Projectile.calculateHorizontalHurtKnockbackDirection(hurtEntity, source)`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum HurtKnockbackDirection {
    /// `Projectile`: the projectile's horizontal delta movement.
    DeltaMovement { x: f64, z: f64 },
    /// `FireworkRocketEntity` / `AbstractThrownPotion`: from the projectile towards the hurt entity.
    AwayFromProjectile,
}

impl DamageEntity {
    /// The entity as a damage source participant: description + full snapshot.
    pub fn capture(entity: &dyn Entity) -> DamageEntity {
        let mut info = DamageEntity::of(entity);
        info.snapshot = entity
            .snapshot_for_damage()
            .map(|b| EntitySnapshot(Rc::new(std::cell::RefCell::new(b))));
        info
    }

    /// The description only (no snapshot).
    pub fn of(entity: &dyn Entity) -> DamageEntity {
        let is_living = entity.living().is_some();
        DamageEntity {
            id: entity.id(),
            uuid: entity.uuid(),
            entity_type: entity.get_type(),
            position: entity.position(),
            eye_position: entity.base().get_eye_position(),
            look_angle: entity.get_look_angle(),
            name: crate::commands::host::entity_name(entity),
            scoreboard_name: entity.get_scoreboard_name(),
            is_living,
            is_player: entity.is_player(),
            instabuild: entity.is_player()
                && entity.player().is_some_and(|p| p.abilities.instabuild),
            may_build: entity.is_player() && entity.player().is_some_and(|p| p.abilities.may_build),
            main_hand_item: if is_living {
                entity.get_item_by_slot(EquipmentSlot::MainHand)
            } else {
                ItemStack::empty()
            },
            weapon_item: entity.get_weapon_item(),
            seconds_to_disable_blocking: entity.get_seconds_to_disable_blocking(),
            pierce_level: entity.get_pierce_level(),
            projectile_hurt_knockback: entity.hurt_knockback_direction(),
            tamed_wolf_owner: entity.tamed_wolf_owner(),
            thrown_potion_hurts_water_sensitive: entity.thrown_potion_hurts_water_sensitive(),
            snapshot: None,
        }
    }
}

#[derive(Clone, Debug)]
pub struct DamageSource {
    /// The `damage_type` registry key of the type (`type.unwrapKey()`).
    pub damage_type: Identifier,
    /// The resolved `DamageType` (`type()`).
    pub data: Arc<DamageType>,
    pub direct_entity: Option<EntityId>,
    pub causing_entity: Option<EntityId>,
    /// `damageSourcePosition` (explicit position; see [`DamageSource::get_source_position`]).
    pub source_position: Option<Vec3>,
    direct: Option<Rc<DamageEntity>>,
    causing: Option<Rc<DamageEntity>>,
}

impl DamageSource {
    /// `new DamageSource(type, directEntity, causingEntity, position)`.
    pub fn new(
        damage_type: Identifier,
        data: Arc<DamageType>,
        direct: Option<&dyn Entity>,
        causing: Option<&dyn Entity>,
        source_position: Option<Vec3>,
    ) -> DamageSource {
        let same = matches!((direct, causing), (Some(d), Some(c)) if d.id() == c.id());
        let direct_snapshot = direct.map(|e| Rc::new(DamageEntity::capture(e)));
        let causing_snapshot = if same {
            direct_snapshot.clone()
        } else {
            causing.map(|e| Rc::new(DamageEntity::capture(e)))
        };
        DamageSource {
            damage_type,
            data,
            direct_entity: direct.map(|e| e.id()),
            causing_entity: causing.map(|e| e.id()),
            source_position,
            direct: direct_snapshot,
            causing: causing_snapshot,
        }
    }

    /// `toString()`.
    pub fn to_debug_string(&self) -> String {
        format!("DamageSource ({})", self.data.msg_id)
    }

    /// `type()`.
    pub fn type_(&self) -> &DamageType {
        &self.data
    }

    /// `getMsgId()`.
    pub fn get_msg_id(&self) -> &str {
        &self.data.msg_id
    }

    /// `getFoodExhaustion()`.
    pub fn get_food_exhaustion(&self) -> f32 {
        self.data.exhaustion
    }

    /// `isDirect()`: `causingEntity == directEntity`.
    pub fn is_direct(&self) -> bool {
        self.causing_entity == self.direct_entity
    }

    /// `getDirectEntity()` (id).
    pub fn get_direct_entity(&self) -> Option<EntityId> {
        self.direct_entity
    }

    /// `getEntity()` (id of the causing entity).
    pub fn get_entity(&self) -> Option<EntityId> {
        self.causing_entity
    }

    /// The snapshot of the causing entity when it is not in the level (it is the running, taken-out entity): lend it
    /// to enchantment/loot code (`Actors::push`) and read it as the attacker.
    pub fn causing_snapshot(
        &self,
        level: &ServerLevel,
    ) -> Option<std::cell::RefMut<'_, Box<dyn Entity>>> {
        let info = self.causing.as_deref()?;
        if level.get_entity(info.id).is_some() {
            return None;
        }
        info.snapshot
            .as_ref()
            .and_then(|s| s.0.try_borrow_mut().ok())
    }

    /// Like [`DamageSource::causing_snapshot`] for the direct entity (`None` when it is the causing one, already lent).
    pub fn direct_snapshot(
        &self,
        level: &ServerLevel,
    ) -> Option<std::cell::RefMut<'_, Box<dyn Entity>>> {
        let info = self.direct.as_deref()?;
        if level.get_entity(info.id).is_some() || self.causing_entity == self.direct_entity {
            return None;
        }
        info.snapshot
            .as_ref()
            .and_then(|s| s.0.try_borrow_mut().ok())
    }

    /// Snapshot of the direct entity.
    pub fn direct_info(&self) -> Option<&DamageEntity> {
        self.direct.as_deref()
    }

    /// Snapshot of the causing entity.
    pub fn causing_info(&self) -> Option<&DamageEntity> {
        self.causing.as_deref()
    }

    /// `getEntity() instanceof LivingEntity`.
    pub fn causing_is_living(&self) -> bool {
        self.causing.as_ref().is_some_and(|e| e.is_living)
    }

    /// `getEntity() instanceof Player`.
    pub fn causing_is_player(&self) -> bool {
        self.causing.as_ref().is_some_and(|e| e.is_player)
    }

    /// `getDirectEntity() instanceof Player`.
    pub fn direct_is_player(&self) -> bool {
        self.direct.as_ref().is_some_and(|e| e.is_player)
    }

    /// `getWeaponItem()`: the direct entity's weapon.
    pub fn get_weapon_item(&self) -> Option<&ItemStack> {
        self.direct.as_ref().and_then(|e| e.weapon_item.as_ref())
    }

    /// `scalesWithDifficulty()`.
    pub fn scales_with_difficulty(&self) -> bool {
        match self.data.scaling {
            DamageScaling::Never => false,
            DamageScaling::WhenCausedByLivingNonPlayer => {
                self.causing_is_living() && !self.causing_is_player()
            }
            DamageScaling::Always => true,
        }
    }

    /// `isCreativePlayer()`: `getEntity() instanceof Player && abilities.instabuild`.
    pub fn is_creative_player(&self) -> bool {
        self.causing
            .as_ref()
            .is_some_and(|e| e.is_player && e.instabuild)
    }

    /// `getSourcePosition()`: the explicit position, else the direct entity's position.
    pub fn get_source_position(&self) -> Option<Vec3> {
        self.source_position
            .or_else(|| self.direct.as_ref().map(|e| e.position))
    }

    /// `sourcePositionRaw()`.
    pub fn source_position_raw(&self) -> Option<Vec3> {
        self.source_position
    }

    /// `is(ResourceKey<DamageType>)`: `key` is a full identifier (`DamageTypes::FALL`) or a `minecraft:` path.
    pub fn is(&self, key: &str) -> bool {
        match key.split_once(':') {
            Some((ns, path)) => {
                self.damage_type.namespace() == ns && self.damage_type.path() == path
            }
            None => self.damage_type.namespace() == "minecraft" && self.damage_type.path() == key,
        }
    }

    /// `is(TagKey<DamageType>)` (`DamageTypeTags::IS_FIRE`, ...): membership in the level's `damage_type` tags.
    pub fn is_tag(&self, level: &ServerLevel, tag: &str) -> bool {
        let Some(tag) = Identifier::try_parse(tag) else {
            return false;
        };
        level
            .registries
            .is_in_tag(crate::registry::keys::DAMAGE_TYPE, &self.damage_type, &tag)
    }

    /// `getLocalizedDeathMessage(victim)`.
    pub fn get_localized_death_message(
        &self,
        level: &ServerLevel,
        victim: &dyn Entity,
    ) -> Component {
        use crate::world::damagesource::display_name::entity_display_name;
        let death_msg = format!("death.attack.{}", self.data.msg_id);
        let victim_name = entity_display_name(level, victim);
        if self.causing.is_none() && self.direct.is_none() {
            let source = crate::world::entity::living_entity::death::resolve_kill_credit(
                level, victim, None,
            );
            let player_msg = format!("{death_msg}.player");
            match source {
                Some(source) => Component::translatable(
                    player_msg,
                    vec![victim_name.into(), source.display_name(level).into()],
                ),
                None => Component::translatable(death_msg, vec![victim_name.into()]),
            }
        } else {
            let name_source = self
                .causing
                .as_ref()
                .or(self.direct.as_ref())
                .expect("one entity present");
            let name = name_source.display_name(level);
            let held = match &self.causing {
                Some(c) if c.is_living => c.main_hand_item.clone(),
                _ => ItemStack::empty(),
            };
            if !held.is_empty() && held.has(mc_data::DataComponentType::CUSTOM_NAME) {
                Component::translatable(
                    format!("{death_msg}.item"),
                    vec![
                        victim_name.into(),
                        name.into(),
                        held.get_display_name().into(),
                    ],
                )
            } else {
                Component::translatable(death_msg, vec![victim_name.into(), name.into()])
            }
        }
    }

    /// `HolderSet<DamageType>.contains(source.typeHolder())` for the `bypassed_by` / `type` sets of data components.
    pub fn is_in_holder_set(
        &self,
        level: &ServerLevel,
        set: &mc_protocol::components::holder::HolderSet<mc_protocol::components::holder::NoDirect>,
    ) -> bool {
        use mc_protocol::components::holder::{Holder, HolderSet};
        match set {
            HolderSet::Named(tag) => self.is_tag(level, &tag.to_string()),
            HolderSet::Direct(holders) => holders
                .iter()
                .any(|h| matches!(h, Holder::Reference(name) if *name == self.damage_type)),
        }
    }
}
