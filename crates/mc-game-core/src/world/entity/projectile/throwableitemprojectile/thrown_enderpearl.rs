//! `net.minecraft.world.entity.projectile.throwableitemprojectile.ThrownEnderpearl`.
//!
//! The owner is looked up in any level of the server (`getEntityInAnyDimension`, then the player list for a killed
//! player). An owner in another level than the pearl is teleported into the pearl's level when the server runs the
//! action the pearl queued (right after the pearl's tick, see `ServerLevel::queue_server_action`); the pearl itself is
//! gone by then. The owner's `enderPearls` registry is kept by the owner's player object, so a pearl removes itself from
//! it through the level (`Entity::ender_pearl_owner_to_deregister`).

use super::super::throwable::{self, DATA_ITEM_STACK};
use super::super::{EntityHitResult, HitResult, Projectile, ProjectileData, on_hit_base};
use crate::server::level::{AnyDimensionEntity, RemoteEntity, ServerLevel};
use crate::server::players::dimension_change::{
    PostTeleportTransition, TeleportTransition, teleport_entity, teleport_player,
    teleport_same_dimension,
};
use crate::server::players::ender_pearl_ticket::{
    PearlOwner, deliver, register_and_update_ender_pearl_ticket, register_ender_pearl,
};
use crate::server::players::teleport_transition::place_portal_ticket;
use crate::server::world_dimensions::{end_key, overworld_key};
use crate::world::entity::entity::TOTAL_AIR_SUPPLY;
use crate::world::entity::factory::{
    self, ENTITY_FACTORIES, EntityRegistration, EntitySpawnRequest,
};
use crate::world::entity::portal::can_teleport_base;
use crate::world::entity::{Entity, EntityData, EntityId, RemovalReason, ServerPlayer};
use crate::world::item::ItemStack;
use crate::{impl_throwable_entity, impl_throwable_projectile};
use linkme::distributed_slice;
use mc_core::ident::Identifier;
use mc_core::math::mth;
use mc_core::phys::Vec3;
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_data::block::{Block, BlockState};
use mc_data::enums::EntitySpawnReason;
use mc_data::{EntityType, Item, SoundEvent};
use mc_nbt::CompoundTag;
use mc_protocol::packets::play::{Relatives, SoundSource};
use mc_world::level_storage::Difficulty;
use uuid::Uuid;

pub struct ThrownEnderpearl {
    base: EntityData,
    projectile: ProjectileData,
    /// `ticketTimer`: ticks until the chunk ticket is refreshed (not saved).
    ticket_timer: i64,
}

/// Who `isAllowedToTeleportOwner` lets the pearl teleport.
enum OwnerTarget {
    /// An owner of the pearl's level.
    Here(EntityId),
    /// An owner of another level that `canUsePortal(true)`.
    Elsewhere(Box<RemoteEntity>),
}

impl ThrownEnderpearl {
    /// `ThrownEnderpearl(type, level)`.
    pub fn new(ty: EntityType, id: EntityId) -> Self {
        let base = EntityData::new(ty, id, TOTAL_AIR_SUPPLY, |b| {
            b.define(DATA_ITEM_STACK, ItemStack::new(Item::ENDER_PEARL));
        });
        Self {
            base,
            projectile: ProjectileData::default(),
            ticket_timer: 0,
        }
    }

    /// `ThrownEnderpearl(level, mob, itemStack)`.
    pub fn with_owner(level: &ServerLevel, owner: &dyn Entity, item: &ItemStack) -> Self {
        let mut s = Self::new(EntityType::ENDER_PEARL, level.next_entity_id());
        throwable::position_at_owner(&mut s, owner);
        throwable::set_item(&mut s.base, item);
        s.set_owner(Some(owner));
        s
    }

    /// `owner.getUUID()`.
    pub fn owner_uuid(&self) -> Option<Uuid> {
        self.projectile.owner
    }

    /// `playSound(level, position)`.
    fn play_sound(level: &mut ServerLevel, position: Vec3) {
        level.play_sound_at(
            None,
            position.x,
            position.y,
            position.z,
            SoundEvent::ENTITY_PLAYER_TELEPORT,
            SoundSource::Players,
            1.0,
            1.0,
        );
    }

    /// `getOwner()` and `isAllowedToTeleportOwner(owner, level)`.
    fn owner_target(&self, level: &ServerLevel) -> Option<OwnerTarget> {
        match level.get_entity_in_any_dimension(self.projectile.owner?)? {
            AnyDimensionEntity::Here(owner) => {
                (owner.is_alive() && !owner.is_sleeping()).then(|| OwnerTarget::Here(owner.id()))
            }
            AnyDimensionEntity::Elsewhere(owner) => owner
                .can_use_portal
                .then_some(OwnerTarget::Elsewhere(owner)),
        }
    }

    /// The body of `onHit` for an owner of another level: `owner.teleport(..)` into the pearl's level, which the server
    /// runs when the pearl's tick is over (`level` is the pearl's level, out of the server).
    fn teleport_remote_owner(&mut self, level: &mut ServerLevel, owner: RemoteEntity) {
        let teleport_pos = self.base.old_position();
        let on_portal_cooldown = self.is_on_portal_cooldown();
        // drawn for an owner that accepts messages; the pearl is discarded right after, nothing else reads its stream
        let endermite_roll = self.base.random.next_float();
        level.queue_server_action(move |server, level| {
            let destination = level.dimension.clone();
            let transition = |y_rot, x_rot| {
                TeleportTransition::new(
                    destination.clone(),
                    teleport_pos,
                    Vec3::ZERO,
                    y_rot,
                    x_rot,
                    PostTeleportTransition::DO_NOTHING,
                )
            };
            if owner.is_player {
                let live = server.with_level(&owner.dimension, |_, owner_level| {
                    owner_level.with_player(owner.id, |_, p| {
                        (
                            p.listener
                                .as_ref()
                                .is_none_or(|l| l.is_accepting_messages()),
                            p.position(),
                            p.base().y_rot(),
                            p.base().x_rot(),
                        )
                    })
                });
                let Some(Some((accepting, position, y_rot, x_rot))) = live else {
                    return;
                };
                if !accepting {
                    return;
                }
                if endermite_roll < 0.05
                    && level.is_spawning_monsters()
                    && level.difficulty() != Difficulty::Peaceful
                    && let Some(mut endermite) = factory::create(
                        EntityType::ENDERMITE,
                        level,
                        EntitySpawnRequest {
                            reason: EntitySpawnReason::Triggered,
                            ignore_checks: false,
                        },
                    )
                {
                    endermite.snap_to(position.x, position.y, position.z, y_rot, x_rot);
                    level.add_fresh_entity(endermite);
                }
                if on_portal_cooldown {
                    server.with_level(&owner.dimension, |_, owner_level| {
                        owner_level.with_player(owner.id, |l, p| p.set_portal_cooldown(l))
                    });
                }
                let t = transition(0.0, 0.0).with_relatives(Relatives::ROTATION);
                let moved = server.with_lent_level(level, |server| {
                    teleport_player(server, &owner.dimension, owner.id, &t)
                });
                if moved {
                    level.with_player(owner.id, |level, p| {
                        p.reset_fall_distance(level);
                        p.reset_current_impulse_context();
                        let source = level.damage_sources().ender_pearl();
                        p.hurt_server(level, &source, 5.0);
                    });
                }
            } else {
                let t = transition(owner.y_rot, owner.x_rot);
                let moved = server.with_lent_level(level, |server| {
                    teleport_entity(server, &owner.dimension, owner.id, &t)
                });
                if let Some(new_id) = moved {
                    level.with_entity(new_id, |level, e| {
                        e.reset_fall_distance(level);
                        if e.living().is_some() {
                            e.reset_current_impulse_context();
                        }
                    });
                }
            }
            Self::play_sound(level, teleport_pos);
        });
    }
}

impl Projectile for ThrownEnderpearl {
    impl_throwable_projectile!();

    fn on_hit_entity(&mut self, level: &mut ServerLevel, hit: &EntityHitResult) {
        let source = level
            .damage_sources()
            .thrown(self.as_dyn(), self.get_owner(level));
        level.with_entity(hit.entity, |level, e| {
            level.lend(self.as_dyn_mut(), |level| {
                e.hurt_server(level, &source, 0.0)
            })
        });
    }

    fn on_hit(&mut self, level: &mut ServerLevel, hit: &HitResult) {
        on_hit_base(self, level, hit);
        // The 32 portal particles are a client call, but their arguments draw from the pearl's random.
        for _ in 0..32 {
            self.base.random.next_double();
            self.base.random.next_gaussian();
            self.base.random.next_gaussian();
        }
        if self.is_removed() {
            return;
        }
        let owner = match self.owner_target(level) {
            Some(OwnerTarget::Here(owner)) => owner,
            Some(OwnerTarget::Elsewhere(owner)) => {
                self.teleport_remote_owner(level, *owner);
                self.discard_in_level(level);
                return;
            }
            None => {
                self.discard_in_level(level);
                return;
            }
        };
        let teleport_pos = self.base.old_position();
        let on_portal_cooldown = self.is_on_portal_cooldown();
        let random = &mut self.base.random;
        let played = level.with_entity(owner, |level, owner| {
            let dimension = level.dimension.clone();
            let transition = |y_rot, x_rot| {
                TeleportTransition::new(
                    dimension.clone(),
                    teleport_pos,
                    Vec3::ZERO,
                    y_rot,
                    x_rot,
                    PostTeleportTransition::DO_NOTHING,
                )
            };
            if owner.is_player() {
                let accepting = owner
                    .as_any()
                    .downcast_ref::<ServerPlayer>()
                    .and_then(|p| p.listener.as_ref())
                    .is_none_or(|l| l.is_accepting_messages());
                if !accepting {
                    return false;
                }
                if random.next_float() < 0.05
                    && level.is_spawning_monsters()
                    && level.difficulty() != Difficulty::Peaceful
                    && let Some(mut endermite) = factory::create(
                        EntityType::ENDERMITE,
                        level,
                        EntitySpawnRequest {
                            reason: EntitySpawnReason::Triggered,
                            ignore_checks: false,
                        },
                    )
                {
                    let p = owner.base();
                    endermite.snap_to(p.x(), p.y(), p.z(), p.y_rot(), p.x_rot());
                    level.add_fresh_entity(endermite);
                }
                if on_portal_cooldown {
                    owner.set_portal_cooldown(level);
                }
                let t = transition(0.0, 0.0).with_relatives(Relatives::ROTATION);
                // `ServerPlayer.teleport`: `removeVehicle()` before a same-dimension move
                level.remove_vehicle(owner);
                teleport_same_dimension(level, owner, &t);
                owner.reset_fall_distance(level);
                owner.reset_current_impulse_context();
                let source = level.damage_sources().ender_pearl();
                owner.hurt_server(level, &source, 5.0);
            } else {
                let t = transition(owner.base().y_rot(), owner.base().x_rot());
                level.stop_riding(owner);
                teleport_same_dimension(level, owner, &t);
                owner.reset_fall_distance(level);
                if owner.living().is_some() {
                    owner.reset_current_impulse_context();
                }
            }
            true
        });
        // the owner is back in the level: its connection hears the sound
        if played == Some(true) {
            Self::play_sound(level, teleport_pos);
        }
        self.discard_in_level(level);
    }
}

impl Entity for ThrownEnderpearl {
    // the pearl keeps `Entity`'s bubble column handling (a stasis chamber), not `Projectile`'s
    impl_throwable_entity!(@shared 0.03, arrow);

    fn tick(&mut self, level: &mut ServerLevel) {
        let start_x = mth::floor_f64(self.base.x()) >> 4;
        let start_z = mth::floor_f64(self.base.z()) >> 4;
        let owner = self
            .projectile
            .owner
            .and_then(|uuid| PearlOwner::find(level, uuid));
        if owner.is_some_and(|o| !o.alive && !o.won_game) && level.ender_pearls_vanish_on_death() {
            self.discard_in_level(level);
        } else {
            throwable::tick(self, level);
        }
        if self.is_alive() {
            let pos = self.base.position();
            self.ticket_timer -= 1;
            if (self.ticket_timer <= 0
                || start_x != mth::floor_f64(pos.x) >> 4
                || start_z != mth::floor_f64(pos.z) >> 4)
                && let Some(owner) = owner
            {
                self.ticket_timer = register_and_update_ender_pearl_ticket(
                    level,
                    &owner,
                    self.id(),
                    self.base.chunk_position(),
                );
            }
        }
    }

    fn can_teleport(&self, level: &ServerLevel, from: &Identifier, to: &Identifier) -> bool {
        let owner_has_seen_credits = *from != end_key()
            || *to != overworld_key()
            || match self
                .projectile
                .owner
                .and_then(|uuid| level.get_entity_in_any_dimension(uuid))
            {
                Some(AnyDimensionEntity::Here(owner)) => owner
                    .as_any()
                    .downcast_ref::<ServerPlayer>()
                    .is_none_or(|p| p.seen_credits),
                Some(AnyDimensionEntity::Elsewhere(owner)) => {
                    !owner.is_player || owner.seen_credits
                }
                None => true,
            };
        owner_has_seen_credits && can_teleport_base(&self.base, level, from, to)
    }

    /// `teleport(transition)`: the resulting pearl places a portal ticket at its position (and, as a new entity, registers with
    /// its owner: `restoreFrom` reads the owner and `setOwner` registers).
    fn after_teleport(&mut self, level: &mut ServerLevel) {
        place_portal_ticket(
            level,
            BlockPos::containing(self.base.x(), self.base.y(), self.base.z()),
        );
        if let Some(owner) = self
            .projectile
            .owner
            .and_then(|uuid| PearlOwner::find(level, uuid))
        {
            register_ender_pearl(level, owner.uuid, owner.in_level.is_some(), self.id());
        }
    }

    fn on_inside_block(&mut self, level: &mut ServerLevel, state: BlockState) {
        if state.block() != Block::END_GATEWAY {
            return;
        }
        let Some(uuid) = self.projectile.owner else {
            return;
        };
        match level.get_entity_in_any_dimension(uuid) {
            Some(AnyDimensionEntity::Here(owner)) if owner.is_player() => {
                let owner = owner.id();
                level.with_player(owner, |level, player| player.on_inside_block(level, state));
            }
            Some(AnyDimensionEntity::Elsewhere(owner)) if owner.is_player => {
                level.queue_server_action(move |server, level| {
                    deliver(server, level, uuid, move |level, player| {
                        player.on_inside_block(level, state)
                    });
                });
            }
            _ => {}
        }
    }

    fn ender_pearl_owner_to_deregister(&self) -> Option<Uuid> {
        (self.base.removal_reason() != Some(RemovalReason::UnloadedWithPlayer))
            .then_some(self.projectile.owner)
            .flatten()
    }

    fn add_additional_save_data(&self, out: &mut CompoundTag) {
        throwable::add_additional_save_data(self, out);
    }

    fn read_additional_save_data(&mut self, level: Option<&ServerLevel>, input: &CompoundTag) {
        throwable::read_additional_save_data(self, level, input, Item::ENDER_PEARL);
    }
}

#[distributed_slice(ENTITY_FACTORIES)]
static ENDER_PEARL_FACTORY: EntityRegistration = EntityRegistration {
    entity_type: EntityType::ENDER_PEARL,
    create: |ty, id, _| Box::new(ThrownEnderpearl::new(ty, id)),
};
