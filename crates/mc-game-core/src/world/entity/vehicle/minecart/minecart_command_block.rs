//! `net.minecraft.world.entity.vehicle.minecart.MinecartCommandBlock`: a cart that runs its command when an activator rail
//! powers it. Its [`BaseCommandBlock`] mirrors the command and the last output into synched data, and the synched data writes
//! them back (`onSyncedDataUpdated`), as it does on the server in vanilla.

use super::abstract_minecart::{self, AbstractMinecart, MinecartData};
use crate::commands::host::{decorate_display_name, entity_name, entity_plain_text_name};
use crate::commands::{CommandEntity, CommandSourceStack};
use crate::define_accessor;
use crate::minecart_entity_methods;
use crate::server::level::ServerLevel;
use crate::server::permissions::LevelBasedPermissionSet;
use crate::world::damagesource::DamageSource;
use crate::world::entity::factory::{ENTITY_FACTORIES, EntityRegistration};
use crate::world::entity::vehicle::vehicle_entity::{self, VehicleEntity};
use crate::world::entity::{Entity, EntityData, EntityId, ServerPlayer};
use crate::world::item::{InteractionHand, InteractionResult, ItemStack};
use crate::world::level::base_command_block::{BaseCommandBlock, Begin};
use crate::world::scores::PlayerTeam;
use linkme::distributed_slice;
use mc_core::phys::{Vec2, Vec3};
use mc_data::block::{Block, BlockState};
use mc_data::{EntityType, Item};
use mc_nbt::CompoundTag;
use mc_text::Component;

define_accessor!(pub DATA_ID_COMMAND_NAME: String = MinecartCommandBlock);
define_accessor!(pub DATA_ID_LAST_OUTPUT: Component = MinecartCommandBlock);

/// `ACTIVATION_DELAY`.
const ACTIVATION_DELAY: i32 = 4;

pub struct MinecartCommandBlock {
    base: EntityData,
    minecart: MinecartData,
    command_block: BaseCommandBlock,
    last_activated: i32,
}

impl MinecartCommandBlock {
    pub fn new(ty: EntityType, id: EntityId) -> Self {
        let (base, minecart) = abstract_minecart::new_base(ty, id, 6, |b| {
            b.define(DATA_ID_COMMAND_NAME, String::new());
            b.define(DATA_ID_LAST_OUTPUT, Component::empty());
        });
        Self {
            base,
            minecart,
            command_block: BaseCommandBlock::default(),
            last_activated: 0,
        }
    }

    /// `getCommandBlock()`.
    pub fn get_command_block(&self) -> &BaseCommandBlock {
        &self.command_block
    }

    /// `getCommandBlock()` for the packet handler and the scenario harness.
    pub fn get_command_block_mut(&mut self) -> &mut BaseCommandBlock {
        &mut self.command_block
    }

    /// `MinecartCommandBase.onUpdated(level)`: the command and the last output go to the synched data; a changed value runs
    /// `onSyncedDataUpdated`, which hands it back to the command block (a changed command name resets its success count).
    pub fn on_updated(&mut self) {
        let command = self.command_block.get_command().to_owned();
        if self.base.entity_data.set(DATA_ID_COMMAND_NAME, command) {
            self.on_synced_data_updated(DATA_ID_COMMAND_NAME.id());
        }
        let last_output = self.command_block.get_last_output();
        if self.base.entity_data.set(DATA_ID_LAST_OUTPUT, last_output) {
            self.on_synced_data_updated(DATA_ID_LAST_OUTPUT.id());
        }
    }

    /// `MinecartCommandBase.createCommandSourceStack(level, source)`'s entity: the cart (`@s`, its name and position).
    fn command_entity(&self, level: &ServerLevel) -> CommandEntity {
        let name = entity_name(self);
        let display_name = {
            let scoreboard = level.scoreboard();
            let team = scoreboard.get_players_team(&self.get_scoreboard_name());
            decorate_display_name(PlayerTeam::format_name_for_team(team, &name), self)
        };
        CommandEntity {
            id: self.base.id,
            uuid: self.base.uuid,
            is_player: false,
            display_name,
            plain_text_name: entity_plain_text_name(self),
            eye_height: self.base.eye_height,
            player: None,
        }
    }

    /// `getCommandBlock().performCommand(level)`.
    fn perform_command(&mut self, level: &mut ServerLevel) -> bool {
        let entity = self.command_entity(level);
        let position = self.base.position();
        let rotation = Vec2::new(self.base.x_rot(), self.base.y_rot());
        let dimension = level.dimension.clone();
        let begin = self.command_block.begin_perform(level, |source| {
            CommandSourceStack::for_entity(
                source,
                position,
                rotation,
                dimension,
                LevelBasedPermissionSet::GAMEMASTER.into(),
                entity,
            )
        });
        match begin {
            Begin::Done(result) => result,
            Begin::Run(pending) => {
                // the command reaches the cart itself (`@s`, a data merge) by id
                level.lend(self.as_dyn_mut(), |level| pending.run(level));
                self.command_block.finish_perform(level.game_time());
                if self.command_block.take_updated() {
                    self.on_updated();
                }
                true
            }
        }
    }
}

impl AbstractMinecart for MinecartCommandBlock {
    fn minecart(&self) -> &MinecartData {
        &self.minecart
    }
    fn minecart_mut(&mut self) -> &mut MinecartData {
        &mut self.minecart
    }
    fn get_default_display_block_state(&self) -> BlockState {
        Block::COMMAND_BLOCK.default_state()
    }
    /// `activateMinecart`: a powered activator rail runs the command at most every fourth tick.
    fn activate_minecart(
        &mut self,
        level: &mut ServerLevel,
        _x: i32,
        _y: i32,
        _z: i32,
        state: bool,
    ) {
        if state && self.base.tick_count - self.last_activated >= ACTIVATION_DELAY {
            self.perform_command(level);
            self.last_activated = self.base.tick_count;
        }
    }
}

impl VehicleEntity for MinecartCommandBlock {
    fn get_drop_item(&self) -> Item {
        Item::MINECART
    }
}

impl Entity for MinecartCommandBlock {
    minecart_entity_methods!();

    fn tick(&mut self, level: &mut ServerLevel) {
        abstract_minecart::tick(self, level);
    }

    /// `interact`: the editor screen opens on the client of a game master; the server only answers.
    fn interact(
        &mut self,
        _level: &mut ServerLevel,
        player: &mut ServerPlayer,
        _hand: InteractionHand,
        _location: Vec3,
    ) -> InteractionResult {
        if player.can_use_game_master_blocks() {
            InteractionResult::SUCCESS
        } else {
            InteractionResult::PASS
        }
    }

    fn get_pick_result(&self) -> Option<ItemStack> {
        Some(ItemStack::new(Item::COMMAND_BLOCK_MINECART))
    }

    fn hurt_server(&mut self, level: &mut ServerLevel, source: &DamageSource, amount: f32) -> bool {
        vehicle_entity::hurt_server(self, level, source, amount)
    }

    fn on_synced_data_updated(&mut self, accessor_id: u8) {
        if accessor_id == crate::world::entity::entity::DATA_POSE.id() {
            self.refresh_dimensions();
        }
        if accessor_id == DATA_ID_LAST_OUTPUT.id() {
            let last_output = self.base.entity_data.get(DATA_ID_LAST_OUTPUT);
            self.command_block.set_last_output(Some(last_output));
        } else if accessor_id == DATA_ID_COMMAND_NAME.id() {
            let command = self.base.entity_data.get(DATA_ID_COMMAND_NAME);
            self.command_block.set_command(&command);
        }
    }

    fn add_additional_save_data(&self, out: &mut CompoundTag) {
        abstract_minecart::add_additional_save_data(self, out);
        self.command_block.save(out);
    }

    fn read_additional_save_data(&mut self, input: &CompoundTag) {
        abstract_minecart::read_additional_save_data(self, input);
        self.command_block.load(input);
        self.on_updated();
    }
}

#[distributed_slice(ENTITY_FACTORIES)]
static MINECART_COMMAND_BLOCK_FACTORY: EntityRegistration = EntityRegistration {
    entity_type: EntityType::COMMAND_BLOCK_MINECART,
    create: |ty, id, _registries| Box::new(MinecartCommandBlock::new(ty, id)),
};
