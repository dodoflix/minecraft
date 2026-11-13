//! `net.minecraft.world.damagesource.DamageSources`: the factory behind `level.damage_sources()`.
//!
//! Sources that name entities take `&dyn Entity` (the snapshot a [`DamageSource`] keeps is built from it): pass the
//! entity you are running as `self.as_dyn()` or one looked up from the level.

use mc_core::ident::Identifier;
use mc_core::phys::Vec3;

use super::damage_source::DamageSource;
use super::damage_types::DamageTypes;
use crate::registry::typed::damage_type::DamageType;
use crate::server::level::ServerLevel;
use crate::world::entity::Entity;
use std::sync::Arc;

pub struct DamageSources<'a> {
    level: &'a ServerLevel,
}

impl ServerLevel {
    /// `damageSources()`.
    pub fn damage_sources(&self) -> DamageSources<'_> {
        DamageSources { level: self }
    }
}

impl<'a> DamageSources<'a> {
    /// `source(key, direct, causing)`: `damageTypes.getOrThrow(key)`.
    pub fn source(
        &self,
        key: &str,
        direct: Option<&dyn Entity>,
        causing: Option<&dyn Entity>,
    ) -> DamageSource {
        let (id, data) = self.resolve(key);
        DamageSource::new(id, data, direct, causing, None)
    }

    fn resolve(&self, key: &str) -> (Identifier, Arc<DamageType>) {
        let id = Identifier::parse(key).expect("valid damage type key");
        let data = self
            .level
            .registries
            .get::<DamageType>(crate::registry::keys::DAMAGE_TYPE, &id)
            .unwrap_or_else(|| panic!("Missing key in ResourceKey[minecraft:root / minecraft:damage_type]: ResourceKey[minecraft:damage_type / {id}]"))
            .clone();
        (id, Arc::new(data))
    }

    pub fn in_fire(&self) -> DamageSource {
        self.source(DamageTypes::IN_FIRE, None, None)
    }

    pub fn campfire(&self) -> DamageSource {
        self.source(DamageTypes::CAMPFIRE, None, None)
    }

    pub fn lightning_bolt(&self) -> DamageSource {
        self.source(DamageTypes::LIGHTNING_BOLT, None, None)
    }

    pub fn on_fire(&self) -> DamageSource {
        self.source(DamageTypes::ON_FIRE, None, None)
    }

    pub fn lava(&self) -> DamageSource {
        self.source(DamageTypes::LAVA, None, None)
    }

    pub fn hot_floor(&self) -> DamageSource {
        self.source(DamageTypes::HOT_FLOOR, None, None)
    }

    pub fn in_wall(&self) -> DamageSource {
        self.source(DamageTypes::IN_WALL, None, None)
    }

    pub fn cramming(&self) -> DamageSource {
        self.source(DamageTypes::CRAMMING, None, None)
    }

    pub fn drown(&self) -> DamageSource {
        self.source(DamageTypes::DROWN, None, None)
    }

    pub fn starve(&self) -> DamageSource {
        self.source(DamageTypes::STARVE, None, None)
    }

    pub fn cactus(&self) -> DamageSource {
        self.source(DamageTypes::CACTUS, None, None)
    }

    pub fn fall(&self) -> DamageSource {
        self.source(DamageTypes::FALL, None, None)
    }

    pub fn ender_pearl(&self) -> DamageSource {
        self.source(DamageTypes::ENDER_PEARL, None, None)
    }

    pub fn fly_into_wall(&self) -> DamageSource {
        self.source(DamageTypes::FLY_INTO_WALL, None, None)
    }

    pub fn generic(&self) -> DamageSource {
        self.source(DamageTypes::GENERIC, None, None)
    }

    pub fn magic(&self) -> DamageSource {
        self.source(DamageTypes::MAGIC, None, None)
    }

    pub fn wither(&self) -> DamageSource {
        self.source(DamageTypes::WITHER, None, None)
    }

    pub fn dragon_breath(&self) -> DamageSource {
        self.source(DamageTypes::DRAGON_BREATH, None, None)
    }

    pub fn dry_out(&self) -> DamageSource {
        self.source(DamageTypes::DRY_OUT, None, None)
    }

    pub fn sweet_berry_bush(&self) -> DamageSource {
        self.source(DamageTypes::SWEET_BERRY_BUSH, None, None)
    }

    pub fn freeze(&self) -> DamageSource {
        self.source(DamageTypes::FREEZE, None, None)
    }

    pub fn stalagmite(&self) -> DamageSource {
        self.source(DamageTypes::STALAGMITE, None, None)
    }

    pub fn generic_kill(&self) -> DamageSource {
        self.source(DamageTypes::GENERIC_KILL, None, None)
    }

    /// `fellOutOfWorld()` (`DamageTypes.FELL_OUT_OF_WORLD` = `out_of_world`).
    pub fn fell_out_of_world(&self) -> DamageSource {
        self.source(DamageTypes::FELL_OUT_OF_WORLD, None, None)
    }

    /// `outOfBorder()`.
    pub fn out_of_border(&self) -> DamageSource {
        self.source(DamageTypes::OUTSIDE_BORDER, None, None)
    }

    pub fn falling_block(&self, entity: &dyn Entity) -> DamageSource {
        self.source(DamageTypes::FALLING_BLOCK, Some(entity), Some(entity))
    }

    pub fn anvil(&self, entity: &dyn Entity) -> DamageSource {
        self.source(DamageTypes::FALLING_ANVIL, Some(entity), Some(entity))
    }

    pub fn falling_stalactite(&self, entity: &dyn Entity) -> DamageSource {
        self.source(DamageTypes::FALLING_STALACTITE, Some(entity), Some(entity))
    }

    pub fn sting(&self, entity: &dyn Entity) -> DamageSource {
        self.source(DamageTypes::STING, Some(entity), Some(entity))
    }

    pub fn mob_attack(&self, entity: &dyn Entity) -> DamageSource {
        self.source(DamageTypes::MOB_ATTACK, Some(entity), Some(entity))
    }

    pub fn no_aggro_mob_attack(&self, entity: &dyn Entity) -> DamageSource {
        self.source(DamageTypes::MOB_ATTACK_NO_AGGRO, Some(entity), Some(entity))
    }

    pub fn player_attack(&self, entity: &dyn Entity) -> DamageSource {
        self.source(DamageTypes::PLAYER_ATTACK, Some(entity), Some(entity))
    }

    pub fn thorns(&self, entity: &dyn Entity) -> DamageSource {
        self.source(DamageTypes::THORNS, Some(entity), Some(entity))
    }

    pub fn sonic_boom(&self, entity: &dyn Entity) -> DamageSource {
        self.source(DamageTypes::SONIC_BOOM, Some(entity), Some(entity))
    }

    pub fn mace(&self, entity: &dyn Entity) -> DamageSource {
        self.source(DamageTypes::MACE_SMASH, Some(entity), Some(entity))
    }

    pub fn arrow(&self, entity: &dyn Entity, owner: Option<&dyn Entity>) -> DamageSource {
        self.source(DamageTypes::ARROW, Some(entity), owner)
    }

    pub fn trident(&self, entity: &dyn Entity, owner: Option<&dyn Entity>) -> DamageSource {
        self.source(DamageTypes::TRIDENT, Some(entity), owner)
    }

    pub fn mob_projectile(&self, entity: &dyn Entity, owner: Option<&dyn Entity>) -> DamageSource {
        self.source(DamageTypes::MOB_PROJECTILE, Some(entity), owner)
    }

    pub fn spit(&self, entity: &dyn Entity, owner: Option<&dyn Entity>) -> DamageSource {
        self.source(DamageTypes::SPIT, Some(entity), owner)
    }

    pub fn wind_charge(&self, entity: &dyn Entity, owner: Option<&dyn Entity>) -> DamageSource {
        self.source(DamageTypes::WIND_CHARGE, Some(entity), owner)
    }

    pub fn fireworks(&self, entity: &dyn Entity, owner: Option<&dyn Entity>) -> DamageSource {
        self.source(DamageTypes::FIREWORKS, Some(entity), owner)
    }

    pub fn thrown(&self, entity: &dyn Entity, owner: Option<&dyn Entity>) -> DamageSource {
        self.source(DamageTypes::THROWN, Some(entity), owner)
    }

    pub fn indirect_magic(&self, entity: &dyn Entity, owner: Option<&dyn Entity>) -> DamageSource {
        self.source(DamageTypes::INDIRECT_MAGIC, Some(entity), owner)
    }

    /// `fireball(fireball, owner)`: `UNATTRIBUTED_FIREBALL` without an owner.
    pub fn fireball(&self, fireball: &dyn Entity, owner: Option<&dyn Entity>) -> DamageSource {
        match owner {
            None => self.source(
                DamageTypes::UNATTRIBUTED_FIREBALL,
                Some(fireball),
                Some(fireball),
            ),
            Some(_) => self.source(DamageTypes::FIREBALL, Some(fireball), owner),
        }
    }

    pub fn wither_skull(&self, skull: &dyn Entity, owner: &dyn Entity) -> DamageSource {
        self.source(DamageTypes::WITHER_SKULL, Some(skull), Some(owner))
    }

    /// `explosion(entity, cause)`: `PLAYER_EXPLOSION` when both are present (`explosion(Explosion)` passes
    /// `getDirectSourceEntity()` / `getIndirectSourceEntity()`).
    pub fn explosion(
        &self,
        entity: Option<&dyn Entity>,
        cause: Option<&dyn Entity>,
    ) -> DamageSource {
        let key = if cause.is_some() && entity.is_some() {
            DamageTypes::PLAYER_EXPLOSION
        } else {
            DamageTypes::EXPLOSION
        };
        self.source(key, entity, cause)
    }

    /// `badRespawnPointExplosion(boomPos)`.
    pub fn bad_respawn_point_explosion(&self, boom_pos: Vec3) -> DamageSource {
        let (id, data) = self.resolve(DamageTypes::BAD_RESPAWN_POINT);
        DamageSource::new(id, data, None, None, Some(boom_pos))
    }
}
