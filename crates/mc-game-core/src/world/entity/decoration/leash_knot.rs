//! `net.minecraft.world.entity.decoration.LeashFenceKnotEntity`: the knot a lead is tied to on a fence.

use super::block_attached::{self, Attached, BlockAttached, Cause};
use super::hanging;
use crate::server::level::ServerLevel;
use crate::server::level::server_entity::ServerEntity;
use crate::world::damagesource::DamageSource;
use crate::world::entity::entity::EntityData;
use crate::world::entity::factory::{ENTITY_FACTORIES, EntityRegistration};
use crate::world::entity::mob::interact::entity_interact;
use crate::world::entity::mob::leashable::{self, LeashHolder};
use crate::world::entity::{Entity, EntityId, MoverType, RemovalReason, ServerPlayer};
use crate::world::item::{InteractionHand, InteractionResult, ItemStack};
use crate::world::level::BlockGetter;
use crate::world::level::block::BlockStateExt;
use linkme::distributed_slice;
use mc_core::phys::{AABB, Vec3};
use mc_core::pos::BlockPos;
use mc_data::{EntityType, GameEvent, Item, SoundEvent};
use std::any::Any;

/// `LeashFenceKnotEntity`.
pub struct LeashFenceKnotEntity {
    base: EntityData,
    attached: Attached,
}

impl LeashFenceKnotEntity {
    /// `LeashFenceKnotEntity(type, level)`.
    pub fn new(ty: EntityType, id: EntityId) -> Self {
        let base = EntityData::new(
            ty,
            id,
            crate::world::entity::entity::TOTAL_AIR_SUPPLY,
            |_| {},
        );
        let mut knot = Self {
            base,
            attached: Attached::default(),
        };
        knot.set_pos(0.0, 0.0, 0.0);
        knot
    }

    /// `LeashFenceKnotEntity(level, pos)`.
    fn at_pos(id: EntityId, pos: BlockPos) -> Self {
        let mut knot = Self::new(EntityType::LEASH_KNOT, id);
        knot.attached.pos = pos;
        knot.set_pos(f64::from(pos.x()), f64::from(pos.y()), f64::from(pos.z()));
        knot
    }
}

/// `LeashFenceKnotEntity.getKnot(level, pos)`.
pub fn get_knot(level: &ServerLevel, pos: BlockPos) -> Option<EntityId> {
    let (x, y, z) = (f64::from(pos.x()), f64::from(pos.y()), f64::from(pos.z()));
    let scan = AABB::new(x - 1.0, y - 1.0, z - 1.0, x + 1.0, y + 1.0, z + 1.0);
    level
        .get_entities_of_class(
            &scan,
            |e| e.get_type() == EntityType::LEASH_KNOT,
            |e| e.leash_knot_pos() == Some(pos),
        )
        .into_iter()
        .next()
}

/// `LeashFenceKnotEntity.createKnot(level, pos)`.
pub fn create_knot(level: &mut ServerLevel, pos: BlockPos) -> EntityId {
    let knot = LeashFenceKnotEntity::at_pos(level.next_entity_id(), pos);
    let id = knot.base.id;
    level.add_fresh_entity(Box::new(knot));
    id
}

/// `LeashFenceKnotEntity.getOrCreateKnot(level, pos)`.
pub fn get_or_create_knot(level: &mut ServerLevel, pos: BlockPos) -> EntityId {
    get_knot(level, pos).unwrap_or_else(|| create_knot(level, pos))
}

impl BlockAttached for LeashFenceKnotEntity {
    fn attached(&self) -> &Attached {
        &self.attached
    }
    fn attached_mut(&mut self) -> &mut Attached {
        &mut self.attached
    }
    /// `recalculateBoundingBox()`.
    fn recalculate_bounding_box(&mut self) {
        let pos = self.attached.pos;
        self.base.set_pos_raw(
            f64::from(pos.x()) + 0.5,
            f64::from(pos.y()) + 0.375,
            f64::from(pos.z()) + 0.5,
        );
        let half_width = f64::from(self.base.get_bb_width()) / 2.0;
        let height = f64::from(self.base.get_bb_height());
        let p = self.base.position();
        self.base.set_bounding_box(AABB::new(
            p.x - half_width,
            p.y,
            p.z - half_width,
            p.x + half_width,
            p.y + height,
            p.z + half_width,
        ));
    }
    /// `survives()`.
    fn survives(&self, level: &ServerLevel) -> bool {
        level.get_block_state(self.attached.pos).is_tag("fences")
    }
    /// `dropItem(level, causedBy)`.
    fn drop_item(&mut self, level: &mut ServerLevel, _caused_by: Option<&Cause>) {
        self.play_sound(level, SoundEvent::ITEM_LEAD_UNTIED, 1.0, 1.0);
    }
}

impl Entity for LeashFenceKnotEntity {
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
    fn is_pickable(&self) -> bool {
        true
    }
    fn tick(&mut self, level: &mut ServerLevel) {
        block_attached::tick(self, level);
    }
    fn set_pos(&mut self, x: f64, y: f64, z: f64) {
        block_attached::set_pos(self, x, y, z);
    }
    fn refresh_dimensions(&mut self) {}
    fn thunder_hit(&mut self, _level: &mut ServerLevel) {}
    fn reposition_entity_after_load(&self) -> bool {
        false
    }
    fn kill_in_level(&mut self, level: &mut ServerLevel) {
        block_attached::kill_self(self, level);
    }
    fn skip_attack_interaction(&mut self, level: &mut ServerLevel, source: &dyn Entity) -> bool {
        block_attached::skip_attack_interaction(self, level, source)
    }
    fn ignore_explosion_in_level(
        &self,
        level: &ServerLevel,
        explosion: &dyn crate::world::level::explosion::Explosion,
    ) -> bool {
        block_attached::ignore_explosion(level, explosion)
    }
    fn r#move(&mut self, level: &mut ServerLevel, _mover: MoverType, delta: Vec3) {
        block_attached::do_move(self, level, delta);
    }
    fn push_in_level(&mut self, level: &mut ServerLevel, impulse: Vec3) {
        block_attached::do_push(self, level, impulse.x, impulse.y, impulse.z);
    }
    fn hurt_server(&mut self, level: &mut ServerLevel, source: &DamageSource, damage: f32) -> bool {
        block_attached::hurt_server(self, level, source, damage)
    }
    fn get_pick_result(&self) -> Option<ItemStack> {
        Some(ItemStack::new(Item::LEAD))
    }
    fn leash_knot_pos(&self) -> Option<BlockPos> {
        Some(self.attached.pos)
    }
    /// `notifyLeasheeRemoved(entity)`.
    fn notify_leashee_removed(&mut self, level: &mut ServerLevel, _leashee: EntityId) {
        if leashable::leashable_leashed_to(level, self.as_dyn()).is_empty() {
            level.remove_held_entity(self.as_dyn_mut(), RemovalReason::Discarded);
        }
    }
    /// `interact(player, hand, location)`: shears, tying the player's mobs to the knot, or handing its mobs to the player.
    fn interact(
        &mut self,
        level: &mut ServerLevel,
        player: &mut ServerPlayer,
        hand: InteractionHand,
        _location: Vec3,
    ) -> InteractionResult {
        if player.get_item_in_hand(hand).is(Item::SHEARS) {
            let result = entity_interact(self, level, player, hand);
            if let InteractionResult::Success(s) = &result
                && s.was_item_interaction()
            {
                return result;
            }
        }
        let mut attached_mob = false;
        let holder = LeashHolder::of(self);
        for id in leashable::leashable_leashed_to(level, &*player) {
            let can = level
                .get_entity(id)
                .is_some_and(|m| leashable::can_have_a_leash_attached_to(level, m, self));
            if can {
                level.with_entity(id, |level, m| {
                    leashable::set_leashed_to(m, level, holder, true)
                });
                attached_mob = true;
            }
        }
        let mut any_dropped = false;
        if !attached_mob && !player.is_shift_key_down() {
            let holder = LeashHolder::of(&*player);
            let leashed = leashable::leashable_leashed_to(level, self.as_dyn());
            // the mobs tell their old holder, this knot, which discards itself with the last one
            level.lend(self.as_dyn_mut(), |level| {
                for id in leashed {
                    let can = level.get_entity(id).is_some_and(|m| {
                        leashable::can_have_a_leash_attached_to(level, m, &*player)
                    });
                    if can {
                        level.with_entity(id, |level, m| {
                            leashable::set_leashed_to(m, level, holder, true)
                        });
                        any_dropped = true;
                    }
                }
            });
        }
        if !attached_mob && !any_dropped {
            entity_interact(self, level, player, hand)
        } else {
            level.game_event_from_entity_ref(
                &*player,
                GameEvent::BLOCK_ATTACH,
                self.base.position(),
            );
            self.play_sound_default(level, SoundEvent::ITEM_LEAD_TIED);
            InteractionResult::SUCCESS
        }
    }
    /// `getAddEntityPacket(serverEntity)`: data 0 at the block position.
    fn get_add_entity_packet(
        &self,
        server_entity: &ServerEntity,
    ) -> mc_protocol::packets::play::ClientboundAddEntityPacket {
        hanging::add_entity_packet(server_entity, self, 0, self.attached.pos)
    }
}

#[distributed_slice(ENTITY_FACTORIES)]
static LEASH_KNOT_FACTORY: EntityRegistration = EntityRegistration {
    entity_type: EntityType::LEASH_KNOT,
    create: |ty, id, _| Box::new(LeashFenceKnotEntity::new(ty, id)),
};
