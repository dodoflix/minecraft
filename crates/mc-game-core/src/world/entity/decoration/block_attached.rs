//! `net.minecraft.world.entity.decoration.BlockAttachedEntity`: what every entity that hangs on a block position shares
//! (item frames, paintings, leash knots, cushions). The concrete types implement [`BlockAttached`] and forward the
//! `Entity` methods named below to the free functions of this module (the Java superclass behavior).

use crate::server::level::ServerLevel;
use crate::world::damagesource::{DamageEntity, DamageSource};
use crate::world::entity::{Entity, EntityId, RemovalReason};
use mc_core::phys::Vec3;
use mc_core::pos::BlockPos;
use mc_data::GameEvent;
use mc_nbt::{CompoundTag, Tag};

/// `BlockAttachedEntity.CHECK_INTERVAL`.
const CHECK_INTERVAL: i32 = 100;

/// What `dropItem(level, causedBy)` reads of the causing entity (a damage source remembers its entities by value).
#[derive(Clone, Copy, Debug)]
pub struct Cause {
    pub id: EntityId,
    pub is_player: bool,
    pub instabuild: bool,
    pub may_build: bool,
    pub is_lightning: bool,
}

impl Cause {
    pub fn of(entity: &DamageEntity) -> Cause {
        Cause {
            id: entity.id,
            is_player: entity.is_player,
            instabuild: entity.instabuild,
            may_build: entity.may_build,
            is_lightning: entity.entity_type == mc_data::EntityType::LIGHTNING_BOLT,
        }
    }

    /// A lightning bolt (`LightningBolt.thunderHit`).
    pub fn lightning(id: EntityId) -> Cause {
        Cause {
            id,
            is_player: false,
            instabuild: false,
            may_build: false,
            is_lightning: true,
        }
    }
}

/// The fields `BlockAttachedEntity` adds to `Entity`.
#[derive(Clone, Copy, Debug, Default)]
pub struct Attached {
    pub pos: BlockPos,
    pub ticks_since_last_check: i32,
}

pub trait BlockAttached: Entity {
    fn attached(&self) -> &Attached;
    fn attached_mut(&mut self) -> &mut Attached;
    /// `recalculateBoundingBox()`.
    fn recalculate_bounding_box(&mut self);
    /// `survives()`.
    fn survives(&self, level: &ServerLevel) -> bool;
    /// `dropItem(level, causedBy)`.
    fn drop_item(&mut self, level: &mut ServerLevel, caused_by: Option<&Cause>);
    /// `onKilled()`.
    fn on_killed(&mut self, _level: &mut ServerLevel) {}
    /// `tickAtCheckInterval()`.
    fn tick_at_check_interval(&mut self, _level: &mut ServerLevel) {}

    /// `getPos()`.
    fn get_pos(&self) -> BlockPos {
        self.attached().pos
    }
}

/// `BlockAttachedEntity.tick()`.
pub fn tick<E: BlockAttached>(e: &mut E, level: &mut ServerLevel) {
    e.check_below_world(level);
    let a = e.attached_mut();
    let check = a.ticks_since_last_check >= CHECK_INTERVAL;
    a.ticks_since_last_check += 1;
    if check {
        e.attached_mut().ticks_since_last_check = 0;
        e.tick_at_check_interval(level);
        if !e.is_removed() && !e.survives(level) {
            level.remove_held_entity(e.as_dyn_mut(), RemovalReason::Discarded);
            e.drop_item(level, None);
        }
    }
}

/// `BlockAttachedEntity.skipAttackInteraction(source)`.
pub fn skip_attack_interaction<E: BlockAttached>(
    e: &mut E,
    level: &mut ServerLevel,
    source: &dyn Entity,
) -> bool {
    if !source.is_player() {
        return false;
    }
    if !level.may_interact_player_uuid(source.uuid(), e.attached().pos) {
        return true;
    }
    let damage = level.damage_sources().player_attack(source);
    e.hurt_server(level, &damage, 0.0)
}

/// `BlockAttachedEntity.kill(level)` (`kill(level, this)`).
pub fn kill_self<E: BlockAttached>(e: &mut E, level: &mut ServerLevel) {
    let id = e.id();
    kill(e, level, Some(id));
}

/// `BlockAttachedEntity.kill(level, attributedTo)`.
pub fn kill<E: BlockAttached>(e: &mut E, level: &mut ServerLevel, attributed_to: Option<EntityId>) {
    e.on_killed(level);
    level.remove_held_entity(e.as_dyn_mut(), RemovalReason::Killed);
    let position = e.base().position();
    match attributed_to {
        Some(source) => level.game_event_from_entity(Some(source), GameEvent::ENTITY_DIE, position),
        None => level.game_event_from_entity_ref(e.as_dyn(), GameEvent::ENTITY_DIE, position),
    }
}

/// `BlockAttachedEntity.hurtServer(level, source, damage)`.
pub fn hurt_server<E: BlockAttached>(
    e: &mut E,
    level: &mut ServerLevel,
    source: &DamageSource,
    _damage: f32,
) -> bool {
    if e.is_invulnerable_to_base(level, source) {
        return false;
    }
    if !level.mob_griefing() && super::caused_by_mob(source.causing_info()) {
        return false;
    }
    if !e.is_removed() {
        kill(e, level, source.get_entity());
        e.mark_hurt();
        e.drop_item(level, source.causing_info().map(Cause::of).as_ref());
    }
    true
}

/// `BlockAttachedEntity.ignoreExplosion(explosion)` given `super.ignoreExplosion` (the entity's own `Entity.ignoreExplosion`).
pub fn ignore_explosion(explosion: &dyn crate::world::level::explosion::Explosion) -> bool {
    if explosion
        .get_direct_source_entity_ref()
        .is_some_and(|d| d.is_in_water())
    {
        true
    } else {
        !explosion.should_affect_blocklike_entities()
    }
}

/// `BlockAttachedEntity.move(moverType, delta)`.
pub fn do_move<E: BlockAttached>(e: &mut E, level: &mut ServerLevel, delta: Vec3) {
    if !e.is_removed() && delta.length_sqr() > 0.0 {
        kill_self(e, level);
        e.drop_item(level, None);
    }
}

/// `BlockAttachedEntity.push(xa, ya, za)`.
pub fn do_push<E: BlockAttached>(e: &mut E, level: &mut ServerLevel, xa: f64, ya: f64, za: f64) {
    if !e.is_removed() && xa * xa + ya * ya + za * za > 0.0 {
        kill_self(e, level);
        e.drop_item(level, None);
    }
}

/// `BlockAttachedEntity.setPos(x, y, z)`.
pub fn set_pos<E: BlockAttached>(e: &mut E, x: f64, y: f64, z: f64) {
    e.attached_mut().pos = BlockPos::containing(x, y, z);
    e.recalculate_bounding_box();
    e.base_mut().needs_sync = true;
}

/// `BlockAttachedEntity.addAdditionalSaveData`.
pub fn add_additional_save_data<E: BlockAttached + ?Sized>(e: &E, out: &mut CompoundTag) {
    let pos = e.attached().pos;
    out.put_int_array("block_pos", vec![pos.x(), pos.y(), pos.z()]);
}

/// `BlockAttachedEntity.readAdditionalSaveData`.
pub fn read_additional_save_data<E: BlockAttached>(e: &mut E, input: &CompoundTag) {
    let stored = input.get("block_pos").and_then(read_block_pos);
    match stored {
        Some(pos) if pos.closer_than(&e.base().block_position(), 16.0) => {
            e.attached_mut().pos = pos
        }
        other => log::error!("Block-attached entity at invalid position: {other:?}"),
    }
}

/// `BlockPos.CODEC` (int array of three ints).
fn read_block_pos(tag: &Tag) -> Option<BlockPos> {
    match tag {
        Tag::IntArray(v) if v.len() == 3 => Some(BlockPos::new(v[0], v[1], v[2])),
        _ => None,
    }
}
