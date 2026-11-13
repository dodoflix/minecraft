//! `net.minecraft.world.entity.Interaction`: an invisible, pickable box that remembers the last player who attacked it and
//! the last one who used it (`/execute on attacker` and `/execute on target` read them back), and whether the client sees a
//! hit as a response.

use super::dimensions::EntityDimensions;
use super::factory::{ENTITY_FACTORIES, EntityRegistration};
use super::{Entity, EntityData, EntityId, entity};
use crate::advancements::triggers::{CriteriaTriggers, fire_with_entity};
use crate::define_accessor;
use crate::server::level::ServerLevel;
use crate::world::item::{InteractionHand, InteractionResult};
use linkme::distributed_slice;
use mc_core::phys::{AABB, Vec3};
use mc_data::EntityType;
use mc_data::enums::{Pose, PushReaction};
use mc_nbt::{CompoundTag, Tag};
use std::any::Any;
use uuid::Uuid;

define_accessor!(DATA_WIDTH_ID: Float = Interaction);
define_accessor!(DATA_HEIGHT_ID: Float = Interaction);
define_accessor!(DATA_RESPONSE_ID: Boolean = Interaction);

/// `Interaction.PlayerAction`: who and at which game time.
#[derive(Clone, Copy, PartialEq, Debug)]
struct PlayerAction {
    player: Uuid,
    timestamp: i64,
}

impl PlayerAction {
    /// `PlayerAction.CODEC` (decode): `None` when a field is missing or malformed.
    fn read(input: &CompoundTag, name: &str) -> Option<Self> {
        let tag = input.get_compound(name)?;
        Some(Self {
            player: entity::read_uuid(tag.get("player"))?,
            timestamp: tag.get("timestamp")?.as_number()?.long_value(),
        })
    }

    /// `PlayerAction.CODEC` (encode).
    fn write(&self, out: &mut CompoundTag, name: &str) {
        let mut tag = CompoundTag::new();
        tag.put_int_array(
            "player",
            mc_core::uuid_util::uuid_to_int_array(&self.player).to_vec(),
        );
        tag.put_long("timestamp", self.timestamp);
        out.put(name, Tag::Compound(tag));
    }
}

pub struct Interaction {
    base: EntityData,
    attack: Option<PlayerAction>,
    interaction: Option<PlayerAction>,
}

impl Interaction {
    pub fn new(ty: EntityType, id: EntityId) -> Self {
        let mut base = EntityData::new(ty, id, entity::TOTAL_AIR_SUPPLY, |b| {
            b.define(DATA_WIDTH_ID, 1.0);
            b.define(DATA_HEIGHT_ID, 1.0);
            b.define(DATA_RESPONSE_ID, false);
        });
        base.no_physics = true;
        Self {
            base,
            attack: None,
            interaction: None,
        }
    }

    fn set_width(&mut self, width: f32) {
        if self.base.entity_data.set(DATA_WIDTH_ID, width) {
            self.on_synced_data_updated(DATA_WIDTH_ID.id());
        }
    }

    fn width(&self) -> f32 {
        self.base.entity_data.get(DATA_WIDTH_ID)
    }

    fn set_height(&mut self, height: f32) {
        if self.base.entity_data.set(DATA_HEIGHT_ID, height) {
            self.on_synced_data_updated(DATA_HEIGHT_ID.id());
        }
    }

    fn height(&self) -> f32 {
        self.base.entity_data.get(DATA_HEIGHT_ID)
    }

    fn set_response(&mut self, response: bool) {
        self.base.entity_data.set(DATA_RESPONSE_ID, response);
    }

    fn response(&self) -> bool {
        self.base.entity_data.get(DATA_RESPONSE_ID)
    }

    fn dimensions(&self) -> EntityDimensions {
        EntityDimensions::scalable(self.width(), self.height())
    }
}

impl Entity for Interaction {
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

    /// Empty: an interaction takes no `baseTick`.
    fn tick(&mut self, _level: &mut ServerLevel) {}

    fn on_synced_data_updated(&mut self, accessor_id: u8) {
        if accessor_id == entity::DATA_POSE.id()
            || accessor_id == DATA_HEIGHT_ID.id()
            || accessor_id == DATA_WIDTH_ID.id()
        {
            self.refresh_dimensions();
        }
    }

    fn get_dimensions(&self, _pose: Pose) -> EntityDimensions {
        self.dimensions()
    }

    fn make_bounding_box(&self, position: Vec3) -> AABB {
        self.dimensions().make_bounding_box_vec(position)
    }

    fn can_be_hit_by_projectile(&self) -> bool {
        false
    }

    fn is_pickable(&self) -> bool {
        true
    }

    fn get_piston_push_reaction(&self) -> PushReaction {
        PushReaction::IgnoreEntity
    }

    fn is_ignoring_block_triggers(&self) -> bool {
        true
    }

    /// `skipAttackInteraction(source)`: a player's attack is recorded and fires `player_hurt_entity`; the attack is handled
    /// (no damage) unless the interaction answers it.
    fn skip_attack_interaction(&mut self, level: &mut ServerLevel, source: &dyn Entity) -> bool {
        if !source.is_player() {
            return false;
        }
        self.attack = Some(PlayerAction {
            player: source.uuid(),
            timestamp: level.game_time(),
        });
        let damage = level.damage_sources().generic();
        fire_with_entity(level, source.id(), &*self, move |level, player, victim| {
            CriteriaTriggers::PLAYER_HURT_ENTITY
                .matches(level, player, victim, &damage, 1.0, 1.0, false)
        });
        !self.response()
    }

    fn hurt_server(
        &mut self,
        _level: &mut ServerLevel,
        _source: &crate::world::damagesource::DamageSource,
        _amount: f32,
    ) -> bool {
        false
    }

    fn interact(
        &mut self,
        level: &mut ServerLevel,
        player: &mut super::ServerPlayer,
        _hand: InteractionHand,
        _location: Vec3,
    ) -> InteractionResult {
        self.interaction = Some(PlayerAction {
            player: player.uuid(),
            timestamp: level.game_time(),
        });
        InteractionResult::CONSUME
    }

    /// `Attackable.getLastAttacker()`: the player of the last attack while it is online.
    fn attackable_last_attacker(&self, level: &ServerLevel) -> Option<EntityId> {
        level.get_player_by_uuid(self.attack?.player)
    }

    /// `Targeting.getTarget()`: the player of the last use while it is online.
    fn targeting_target(&self, level: &ServerLevel) -> Option<EntityId> {
        level.get_player_by_uuid(self.interaction?.player)
    }

    fn add_additional_save_data(&self, out: &mut CompoundTag) {
        out.put_float("width", self.width());
        out.put_float("height", self.height());
        if let Some(attack) = &self.attack {
            attack.write(out, "attack");
        }
        if let Some(interaction) = &self.interaction {
            interaction.write(out, "interaction");
        }
        out.put_boolean("response", self.response());
    }

    fn read_additional_save_data(&mut self, input: &CompoundTag) {
        self.set_width(input.get_float_or("width", 1.0));
        self.set_height(input.get_float_or("height", 1.0));
        self.attack = PlayerAction::read(input, "attack");
        self.interaction = PlayerAction::read(input, "interaction");
        self.set_response(input.get_boolean_or("response", false));
        let bb = self.make_bounding_box(self.base.position());
        self.base.set_bounding_box(bb);
    }
}

#[distributed_slice(ENTITY_FACTORIES)]
static INTERACTION_FACTORY: EntityRegistration = EntityRegistration {
    entity_type: EntityType::INTERACTION,
    create: |ty, id, _registries| Box::new(Interaction::new(ty, id)),
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nbt_round_trip_and_dimensions() {
        let mut interaction = Interaction::new(EntityType::INTERACTION, 1);
        let player = Uuid::from_u128(0x1234_5678_9abc_def0_1234_5678_9abc_def0);
        let mut tag = CompoundTag::new();
        tag.put_float("width", 2.5);
        tag.put_float("height", 0.25);
        tag.put_boolean("response", true);
        PlayerAction {
            player,
            timestamp: 77,
        }
        .write(&mut tag, "attack");
        interaction.read_additional_save_data(&tag);
        assert_eq!(
            interaction.attack,
            Some(PlayerAction {
                player,
                timestamp: 77
            })
        );
        assert_eq!(interaction.interaction, None);
        assert!(interaction.response());
        let bb = interaction.base.bounding_box();
        assert_eq!((bb.max_x - bb.min_x, bb.max_y - bb.min_y), (2.5, 0.25));
        let mut saved = CompoundTag::new();
        interaction.add_additional_save_data(&mut saved);
        assert_eq!(saved, tag);
    }
}
