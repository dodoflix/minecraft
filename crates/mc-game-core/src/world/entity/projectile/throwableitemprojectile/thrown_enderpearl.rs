//! `net.minecraft.world.entity.projectile.throwableitemprojectile.ThrownEnderpearl`.
//!
//! The owner's pearl set (`ServerPlayer.enderPearls`) holds entity ids and a removed pearl does not deregister itself in
//! `onRemoval`: every reader (save, unload on leave) skips ids without a live pearl.
//! shortcut: the list grows by one id per pearl thrown while the player is online, drop an id when the pearl is removed
//! (needs a level in `onRemoval`).
//!
//! Not implemented: an owner in another dimension than the pearl (`getEntityInAnyDimension`, the cached owner reference
//! of the vanilla pearl, `canUsePortal(true)` + the cross-dimension `teleport`): a level only sees its own entities, so such
//! a pearl discards without teleporting. Not implemented: `Entity.teleport` override placing a portal ticket at the pearl's
//! new position after it changed dimension itself.

use super::super::throwable::{self, DATA_ITEM_STACK};
use super::super::{EntityHitResult, HitResult, Projectile, ProjectileData, on_hit_base};
use crate::server::level::ServerLevel;
use crate::server::players::dimension_change::{
    PostTeleportTransition, TeleportTransition, teleport_same_dimension,
};
use crate::server::players::ender_pearl_ticket::register_and_update_ender_pearl_ticket;
use crate::server::world_dimensions::{end_key, overworld_key};
use crate::world::entity::entity::TOTAL_AIR_SUPPLY;
use crate::world::entity::factory::{
    self, ENTITY_FACTORIES, EntityRegistration, EntitySpawnRequest,
};
use crate::world::entity::portal::can_teleport_base;
use crate::world::entity::{Entity, EntityData, EntityId, ServerPlayer};
use crate::world::item::ItemStack;
use crate::{impl_throwable_entity, impl_throwable_projectile};
use linkme::distributed_slice;
use mc_core::ident::Identifier;
use mc_core::math::mth;
use mc_core::phys::Vec3;
use mc_core::random::RandomSource;
use mc_data::block::{Block, BlockState};
use mc_data::enums::EntitySpawnReason;
use mc_data::{EntityType, Item, SoundEvent};
use mc_nbt::CompoundTag;
use mc_protocol::packets::play::{Relatives, SoundSource};
use mc_world::level_storage::Difficulty;

pub struct ThrownEnderpearl {
    base: EntityData,
    projectile: ProjectileData,
    /// `ticketTimer`: ticks until the chunk ticket is refreshed (not saved).
    ticket_timer: i64,
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

    /// `findOwnerIncludingDeadPlayer` as a player: `(id, alive, wonGame)`. A killed player left the level but is still
    /// the owner.
    fn owner_player(&self, level: &ServerLevel) -> Option<(EntityId, bool, bool)> {
        let uuid = self.projectile.owner?;
        let owner = level.get_entity_by_uuid(uuid).or_else(|| {
            level
                .dead_players
                .values()
                .map(|p| &**p)
                .find(|p| p.uuid() == uuid)
        })?;
        let player = owner.as_any().downcast_ref::<ServerPlayer>()?;
        Some((player.id(), player.is_alive(), player.won_game))
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
        let Some(owner) = self
            .get_owner(level)
            // `isAllowedToTeleportOwner` (same dimension)
            .filter(|o| o.is_alive() && !o.is_sleeping())
            .map(|o| o.id())
        else {
            self.discard_in_level(level);
            return;
        };
        let teleport_pos = self.base.old_position();
        let on_portal_cooldown = self.is_on_portal_cooldown();
        let random = &mut self.base.random;
        level.with_entity(owner, |level, owner| {
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
                    return;
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
                owner.reset_fall_distance();
                owner.reset_current_impulse_context();
                let source = level.damage_sources().ender_pearl();
                owner.hurt_server(level, &source, 5.0);
            } else {
                let t = transition(owner.base().y_rot(), owner.base().x_rot());
                level.stop_riding(owner);
                teleport_same_dimension(level, owner, &t);
                owner.reset_fall_distance();
                if owner.living().is_some() {
                    owner.reset_current_impulse_context();
                }
            }
            Self::play_sound(level, teleport_pos);
        });
        self.discard_in_level(level);
    }
}

impl Entity for ThrownEnderpearl {
    // the pearl keeps `Entity`'s bubble column handling (a stasis chamber), not `Projectile`'s
    impl_throwable_entity!(@shared 0.03, arrow);

    fn tick(&mut self, level: &mut ServerLevel) {
        let start_x = mth::floor_f64(self.base.x()) >> 4;
        let start_z = mth::floor_f64(self.base.z()) >> 4;
        let owner = self.owner_player(level);
        if owner.is_some_and(|(_, alive, won_game)| !alive && !won_game)
            && level.ender_pearls_vanish_on_death()
        {
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
                && let Some((owner, _, _)) = owner
            {
                self.ticket_timer = register_and_update_ender_pearl_ticket(
                    level,
                    owner,
                    self.id(),
                    self.base.chunk_position(),
                );
            }
        }
    }

    fn can_teleport(&self, level: &ServerLevel, from: &Identifier, to: &Identifier) -> bool {
        let owner_has_seen_credits = *from != end_key()
            || *to != overworld_key()
            || level
                .get_entity_by_uuid(self.projectile.owner.unwrap_or_default())
                .and_then(|o| o.as_any().downcast_ref::<ServerPlayer>())
                .is_none_or(|p| p.seen_credits);
        owner_has_seen_credits && can_teleport_base(&self.base, level, from, to)
    }

    fn on_inside_block(&mut self, level: &mut ServerLevel, state: BlockState) {
        if state.block() == Block::END_GATEWAY
            && let Some(owner) = self
                .get_owner(level)
                .filter(|o| o.is_player())
                .map(|o| o.id())
        {
            level.with_player(owner, |level, player| player.on_inside_block(level, state));
        }
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
