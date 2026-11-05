//! `net.minecraft.world.damagesource.DamageSource`.
//!
//! Vanilla holds `Entity` references; here entities are looked up by id and the entity that is currently running
//! (an attacking player or mob) is taken out of the level, so a source keeps the ids *and* a [`DamageEntity`]
//! snapshot of what vanilla reads from the references (position, name, main hand item, kind).

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
use crate::world::item::ItemStack;

/// What a [`DamageSource`] needs to know about its direct/causing entity, captured when the source is built.
#[derive(Clone, Debug)]
pub struct DamageEntity {
    pub id: EntityId,
    pub uuid: Uuid,
    pub entity_type: EntityType,
    pub position: Vec3,
    /// `getName()` (not team-decorated: the scoreboard is applied when the message is built).
    pub name: Component,
    pub scoreboard_name: String,
    /// `instanceof LivingEntity`.
    pub is_living: bool,
    /// `instanceof Player`.
    pub is_player: bool,
    /// `instanceof Player && abilities.instabuild`.
    pub instabuild: bool,
    /// `getMainHandItem()` for living entities, empty otherwise.
    pub main_hand_item: ItemStack,
    /// `getWeaponItem()`.
    pub weapon_item: Option<ItemStack>,
}

impl DamageEntity {
    pub fn of(entity: &dyn Entity) -> DamageEntity {
        let is_living = entity.living().is_some();
        DamageEntity {
            id: entity.id(),
            uuid: entity.uuid(),
            entity_type: entity.get_type(),
            position: entity.position(),
            name: crate::commands::host::entity_name(entity),
            scoreboard_name: entity.get_scoreboard_name(),
            is_living,
            is_player: entity.is_player(),
            instabuild: entity.is_player() && entity.player().is_some_and(|p| p.abilities.instabuild),
            main_hand_item: if is_living { entity.get_item_by_slot(EquipmentSlot::MainHand) } else { ItemStack::empty() },
            weapon_item: entity.get_weapon_item(),
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
    direct: Option<Arc<DamageEntity>>,
    causing: Option<Arc<DamageEntity>>,
}

impl DamageSource {
    /// `new DamageSource(type, directEntity, causingEntity, position)`.
    pub fn new(damage_type: Identifier, data: Arc<DamageType>, direct: Option<&dyn Entity>, causing: Option<&dyn Entity>, source_position: Option<Vec3>) -> DamageSource {
        let same = matches!((direct, causing), (Some(d), Some(c)) if d.id() == c.id());
        let direct_snapshot = direct.map(|e| Arc::new(DamageEntity::of(e)));
        let causing_snapshot = if same { direct_snapshot.clone() } else { causing.map(|e| Arc::new(DamageEntity::of(e))) };
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
            DamageScaling::WhenCausedByLivingNonPlayer => self.causing_is_living() && !self.causing_is_player(),
            DamageScaling::Always => true,
        }
    }

    /// `isCreativePlayer()`: `getEntity() instanceof Player && abilities.instabuild`.
    pub fn is_creative_player(&self) -> bool {
        self.causing.as_ref().is_some_and(|e| e.is_player && e.instabuild)
    }

    /// `getSourcePosition()`: the explicit position, else the direct entity's position.
    pub fn get_source_position(&self) -> Option<Vec3> {
        self.source_position.or_else(|| self.direct.as_ref().map(|e| e.position))
    }

    /// `sourcePositionRaw()`.
    pub fn source_position_raw(&self) -> Option<Vec3> {
        self.source_position
    }

    /// `is(ResourceKey<DamageType>)`: `key` is a full identifier (`DamageTypes::FALL`) or a `minecraft:` path.
    pub fn is(&self, key: &str) -> bool {
        match key.split_once(':') {
            Some((ns, path)) => self.damage_type.namespace() == ns && self.damage_type.path() == path,
            None => self.damage_type.namespace() == "minecraft" && self.damage_type.path() == key,
        }
    }

    /// `is(TagKey<DamageType>)` (`DamageTypeTags::IS_FIRE`, ...): membership in the level's `damage_type` tags.
    pub fn is_tag(&self, level: &ServerLevel, tag: &str) -> bool {
        let Some(tag) = Identifier::try_parse(tag) else { return false };
        level.registries.is_in_tag(crate::registry::keys::DAMAGE_TYPE, &self.damage_type, &tag)
    }
}
