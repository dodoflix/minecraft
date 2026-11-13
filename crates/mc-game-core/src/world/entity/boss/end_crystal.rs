//! `net.minecraft.world.entity.boss.enderdragon.EndCrystal`.

use crate::define_accessor;
use crate::server::level::ServerLevel;
use crate::server::level::level_explode::Explode;
use crate::world::damagesource::{DamageSource, DamageTypeTags};
use crate::world::entity::factory::{ENTITY_FACTORIES, EntityRegistration};
use crate::world::entity::living::read_block_pos;
use crate::world::entity::{Entity, EntityData, EntityId, MovementEmission, RemovalReason, entity};
use crate::world::item::ItemStack;
use crate::world::level::block::impls::base_fire_block;
use crate::world::level::explosion::ExplosionInteraction;
use crate::world::level::{BlockGetter, LevelAccessor};
use linkme::distributed_slice;
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_data::{EntityType, GameEvent, Item};
use mc_nbt::CompoundTag;
use std::any::Any;

define_accessor!(DATA_BEAM_TARGET: OptionalBlockPos = EndCrystal);
define_accessor!(DATA_SHOW_BOTTOM: Boolean = EndCrystal);

const DEFAULT_SHOW_BOTTOM: bool = true;
const EXPLOSION_POWER: f32 = 6.0;

pub struct EndCrystal {
    base: EntityData,
    /// `time` (only the client animation reads it).
    pub time: i32,
}

impl EndCrystal {
    /// `EndCrystal(type, level)`.
    pub fn new(ty: EntityType, id: EntityId) -> Self {
        let mut base = EntityData::new(ty, id, entity::TOTAL_AIR_SUPPLY, |b| {
            b.define(DATA_BEAM_TARGET, None);
            b.define(DATA_SHOW_BOTTOM, DEFAULT_SHOW_BOTTOM);
        });
        base.blocks_building = true;
        let time = base.random.next_int_bounded(100000);
        Self { base, time }
    }

    /// `EndCrystal(level, x, y, z)`.
    pub fn at(level: &ServerLevel, x: f64, y: f64, z: f64) -> Self {
        let mut crystal = Self::new(EntityType::END_CRYSTAL, level.next_entity_id());
        crystal.set_pos(x, y, z);
        crystal
    }

    pub fn set_beam_target(&mut self, target: Option<BlockPos>) {
        self.base.entity_data.set(DATA_BEAM_TARGET, target);
    }

    pub fn get_beam_target(&self) -> Option<BlockPos> {
        self.base.entity_data.get(DATA_BEAM_TARGET)
    }

    pub fn set_show_bottom(&mut self, show_bottom: bool) {
        self.base.entity_data.set(DATA_SHOW_BOTTOM, show_bottom);
    }

    pub fn shows_bottom(&self) -> bool {
        self.base.entity_data.get(DATA_SHOW_BOTTOM)
    }

    /// `onDestroyedBy(level, source)`.
    fn on_destroyed_by(&self, level: &ServerLevel, _source: &DamageSource) {
        if level.has_dragon_fight() {
            // Not implemented: `EnderDragonFight.onCrystalDestroyed(crystal, source)` (the dragon fight is not ported).
        }
    }
}

impl Entity for EndCrystal {
    fn base(&self) -> &EntityData {
        &self.base
    }
    fn base_mut(&mut self) -> &mut EntityData {
        &mut self.base
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
    fn as_dyn(&self) -> &dyn Entity {
        self
    }
    fn as_dyn_mut(&mut self) -> &mut dyn Entity {
        self
    }

    fn get_movement_emission(&self) -> MovementEmission {
        MovementEmission::None
    }

    /// `tick()`: no `baseTick`; with a dragon fight a crystal in air lights the block it stands in.
    fn tick(&mut self, level: &mut ServerLevel) {
        self.time += 1;
        self.apply_effects_from_blocks(level);
        self.handle_portal(level);
        if level.has_dragon_fight() {
            let pos = self.base.block_position();
            if level.get_block_state(pos).is_air() {
                let fire = base_fire_block::get_state(&*level, pos);
                level.set_block_and_update(pos, fire);
            }
        }
    }

    fn is_pickable(&self) -> bool {
        true
    }

    /// `hurtServer(level, source, damage)`: the crystal is removed before its explosion (so watchers get
    /// `remove_entities` first); a source that is not an explosion makes it explode, credited to the attacker.
    fn hurt_server(
        &mut self,
        level: &mut ServerLevel,
        source: &DamageSource,
        _amount: f32,
    ) -> bool {
        level.note_unlent_attacker(source, self);
        if self.is_invulnerable_to_base(level, source) {
            return false;
        }
        if source
            .causing_info()
            .is_some_and(|c| c.entity_type == EntityType::ENDER_DRAGON)
        {
            return false;
        }
        if !self.is_removed() {
            level.remove_held_entity(self.as_dyn_mut(), RemovalReason::Killed);
            if !source.is_tag(level, DamageTypeTags::IS_EXPLOSION) {
                // a lent attacker (a player's hit runs inside `ServerLevel::lend`) is found here and takes part in
                // the explosion where vanilla runs it; a causing entity that is out of the level and not lent (a
                // projectile running its hit) is read from the source
                let damage_source = match source.get_entity().and_then(|id| level.get_entity(id)) {
                    Some(cause) => Some(
                        level
                            .damage_sources()
                            .explosion(Some(self.as_dyn()), Some(cause)),
                    ),
                    None => level
                        .damage_sources()
                        .explosion_caused_by(self.as_dyn(), source),
                };
                explode(level, self.as_dyn(), damage_source);
            }
            self.on_destroyed_by(level, source);
        }
        true
    }

    /// `kill(level)`: the fight is told first, then `Entity.kill`.
    fn kill_in_level(&mut self, level: &mut ServerLevel) {
        let generic = level.damage_sources().generic();
        self.on_destroyed_by(level, &generic);
        level.remove_held_entity(self.as_dyn_mut(), RemovalReason::Killed);
        level.game_event_from_entity(Some(self.id()), GameEvent::ENTITY_DIE, self.position());
    }

    fn get_pick_result(&self) -> Option<ItemStack> {
        Some(ItemStack::new(Item::END_CRYSTAL))
    }

    fn add_additional_save_data(&self, out: &mut CompoundTag) {
        if let Some(target) = self.get_beam_target() {
            out.put_int_array("beam_target", vec![target.x(), target.y(), target.z()]);
        }
        out.put_boolean("ShowBottom", self.shows_bottom());
    }

    fn read_additional_save_data(&mut self, input: &CompoundTag) {
        self.set_beam_target(read_block_pos(input.get("beam_target")));
        self.set_show_bottom(input.get_boolean_or("ShowBottom", DEFAULT_SHOW_BOTTOM));
    }
}

/// The explosion of a destroyed crystal at its position (`Level.explode(crystal, damageSource, null, x, y, z, 6.0F, false, BLOCK)`).
fn explode(level: &mut ServerLevel, crystal: &dyn Entity, damage_source: Option<DamageSource>) {
    let p = crystal.position();
    let params = Explode::new(
        Some(crystal),
        p.x,
        p.y,
        p.z,
        EXPLOSION_POWER,
        ExplosionInteraction::Block,
    )
    .with_damage_source(damage_source, None);
    level.explode(params, &mut []);
}

#[distributed_slice(ENTITY_FACTORIES)]
static END_CRYSTAL_FACTORY: EntityRegistration = EntityRegistration {
    entity_type: EntityType::END_CRYSTAL,
    create: |ty, id, _registries| Box::new(EndCrystal::new(ty, id)),
};
