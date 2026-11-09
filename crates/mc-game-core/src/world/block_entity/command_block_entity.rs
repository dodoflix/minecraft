//! `CommandBlockEntity`: powered / auto / condition-met flags and its [`BaseCommandBlock`].
//!
//! Vanilla's methods that read `this.level` take the level as a parameter here; the block entity is out of its chunk while
//! they run (see `ServerLevel::with_typed_block_entity`), so they never look themselves up.

use super::ComponentReader;
use super::{BLOCK_ENTITY_TYPES, BlockEntity, BlockEntityData, BlockEntityRegistration};
use crate::commands::CommandSourceStack;
use crate::server::level::ServerLevel;
use crate::server::permissions::LevelBasedPermissionSet;
use crate::world::level::BlockGetter;
use crate::world::level::base_command_block::{BaseCommandBlock, Begin};
use linkme::distributed_slice;
use mc_core::phys::{Vec2, Vec3};
use mc_core::pos::BlockPos;
use mc_data::BlockEntityType;
use mc_data::block::{Block, BlockState};
use mc_data::props;
use mc_nbt::CompoundTag;
use mc_protocol::packets::play::CommandBlockEntityMode as Mode;

pub struct CommandBlockEntity {
    base: BlockEntityData,
    powered: bool,
    auto: bool,
    condition_met: bool,
    command_block: BaseCommandBlock,
}

/// `state.getBlock() instanceof CommandBlock`.
pub fn is_command_block(block: Block) -> bool {
    block == Block::COMMAND_BLOCK
        || block == Block::REPEATING_COMMAND_BLOCK
        || block == Block::CHAIN_COMMAND_BLOCK
}

impl CommandBlockEntity {
    /// `CommandBlock.newBlockEntity`: `setAutomatic(this.automatic)` (chain command blocks) before any level is attached.
    pub fn new(pos: BlockPos, state: BlockState) -> Self {
        Self {
            base: BlockEntityData::new(BlockEntityType::COMMAND_BLOCK, pos, state),
            powered: false,
            auto: state.block() == Block::CHAIN_COMMAND_BLOCK,
            condition_met: false,
            command_block: BaseCommandBlock::default(),
        }
    }

    /// `getCommandBlock()`.
    pub fn get_command_block(&self) -> &BaseCommandBlock {
        &self.command_block
    }

    pub fn get_command_block_mut(&mut self) -> &mut BaseCommandBlock {
        &mut self.command_block
    }

    pub fn set_powered(&mut self, powered: bool) {
        self.powered = powered;
    }

    pub fn is_powered(&self) -> bool {
        self.powered
    }

    pub fn is_automatic(&self) -> bool {
        self.auto
    }

    /// `setAutomatic(auto)`; `level` is `this.level` (`None` while the entity is not attached to a level yet).
    pub fn set_automatic(&mut self, level: Option<&mut ServerLevel>, auto: bool) {
        let previous_auto = self.auto;
        self.auto = auto;
        if !previous_auto
            && auto
            && !self.powered
            && let Some(level) = level
            && self.get_mode() != Mode::Sequence
        {
            self.schedule_tick(level);
        }
    }

    /// `onModeSwitch()`.
    pub fn on_mode_switch(&mut self, level: &mut ServerLevel) {
        if self.get_mode() == Mode::Auto && (self.powered || self.auto) {
            self.schedule_tick(level);
        }
    }

    fn schedule_tick(&mut self, level: &mut ServerLevel) {
        let command_block = self.get_block_state().block();
        if is_command_block(command_block) {
            self.mark_condition_met(level);
            level.schedule_tick_block(self.get_block_pos(), command_block, 1);
        }
    }

    pub fn was_condition_met(&self) -> bool {
        self.condition_met
    }

    /// `markConditionMet()`.
    pub fn mark_condition_met(&mut self, level: &ServerLevel) -> bool {
        self.condition_met = true;
        if self.is_conditional(level) {
            let pos = self.get_block_pos();
            let relative =
                pos.relative(level.get_block_state(pos).get(props::FACING).get_opposite());
            if is_command_block(level.get_block_state(relative).block()) {
                self.condition_met = level
                    .get_block_entity(relative)
                    .and_then(|be| be.as_any().downcast_ref::<CommandBlockEntity>())
                    .is_some_and(|be| be.get_command_block().get_success_count() > 0);
            } else {
                self.condition_met = false;
            }
        }
        self.condition_met
    }

    /// `getMode()`.
    pub fn get_mode(&self) -> Mode {
        let block = self.get_block_state().block();
        if block == Block::COMMAND_BLOCK {
            Mode::Redstone
        } else if block == Block::REPEATING_COMMAND_BLOCK {
            Mode::Auto
        } else if block == Block::CHAIN_COMMAND_BLOCK {
            Mode::Sequence
        } else {
            Mode::Redstone
        }
    }

    /// `isConditional()`.
    pub fn is_conditional(&self, level: &ServerLevel) -> bool {
        let state = level.get_block_state(self.get_block_pos());
        is_command_block(state.block()) && state.get(props::CONDITIONAL)
    }

    /// `createCommandSourceStack(level, source)` of the anonymous `BaseCommandBlock`.
    fn create_command_source_stack(
        level: &ServerLevel,
        pos: BlockPos,
        state: BlockState,
        name: mc_text::Component,
        source: std::sync::Arc<dyn crate::commands::CommandSource>,
    ) -> CommandSourceStack {
        let facing = state.get(props::FACING);
        CommandSourceStack::named(
            source,
            Vec3::at_center_of(&pos),
            Vec2::new(0.0, facing.to_y_rot()),
            level.dimension.clone(),
            LevelBasedPermissionSet::GAMEMASTER.into(),
            name,
        )
    }

    /// `commandBlock.setCommand(command)` (`BaseCommandBlock.setCommand` + `setChanged()`).
    pub fn set_command(level: &mut ServerLevel, pos: BlockPos, command: &str) {
        level.with_typed_block_entity::<CommandBlockEntity, _>(pos, |level, be| {
            be.command_block.set_command(command);
            level.block_entity_set_changed(pos, be.get_block_state());
        });
    }

    /// `onUpdated(level)`: `level.sendBlockUpdated(pos, state, state, 3)`.
    pub fn on_updated(level: &mut ServerLevel, pos: BlockPos) {
        let state = level.get_block_state(pos);
        level.send_block_updated(pos, state, state, 3);
    }

    /// `getCommandBlock().performCommand(level)` for the command block entity at `pos`.
    pub fn perform_command(level: &mut ServerLevel, pos: BlockPos) -> bool {
        let begin = level.with_typed_block_entity::<CommandBlockEntity, _>(pos, |level, be| {
            let name = be.command_block.get_name();
            let state = be.get_block_state();
            be.command_block.begin_perform(level, |source| {
                Self::create_command_source_stack(level, pos, state, name, source)
            })
        });
        match begin {
            // the entity vanished: nothing to run
            None => true,
            Some(Begin::Done(result)) => result,
            Some(Begin::Run(pending)) => {
                pending.run(level);
                let game_time = level.game_time();
                let updated = level
                    .with_typed_block_entity::<CommandBlockEntity, _>(pos, |_, be| {
                        be.command_block.finish_perform(game_time);
                        be.command_block.take_updated()
                    })
                    .unwrap_or(false);
                if updated {
                    Self::on_updated(level, pos);
                }
                true
            }
        }
    }
}

impl BlockEntity for CommandBlockEntity {
    block_entity_base!();

    fn load_additional(&mut self, tag: &CompoundTag) {
        self.command_block.load(tag);
        self.powered = tag.get_boolean_or("powered", false);
        self.condition_met = tag.get_boolean_or("conditionMet", false);
        self.auto = tag.get_boolean_or("auto", false);
    }

    fn load_additional_in_level(
        &mut self,
        level: &mut ServerLevel,
        tag: &CompoundTag,
    ) -> Option<BlockState> {
        self.command_block.load(tag);
        self.powered = tag.get_boolean_or("powered", false);
        self.condition_met = tag.get_boolean_or("conditionMet", false);
        self.set_automatic(Some(level), tag.get_boolean_or("auto", false));
        None
    }

    fn save_additional(&self, tag: &mut CompoundTag) {
        self.command_block.save(tag);
        tag.put_boolean("powered", self.is_powered());
        tag.put_boolean("conditionMet", self.was_condition_met());
        tag.put_boolean("auto", self.is_automatic());
    }

    fn collect_implicit_components(
        &self,
        builder: &mut mc_protocol::components::map::DataComponentMapBuilder,
    ) {
        *builder = std::mem::take(builder).set_opt(
            mc_protocol::components::CUSTOM_NAME,
            self.command_block.get_custom_name().cloned(),
        );
    }

    /// `removeComponentsFromTag(output)`.
    fn remove_components_from_tag(&self, tag: &mut CompoundTag) {
        tag.remove("CustomName");
        tag.remove("conditionMet");
        tag.remove("powered");
    }

    /// `applyImplicitComponents`: the custom name.
    fn apply_implicit_components(&mut self, components: &ComponentReader) {
        self.command_block.set_custom_name(
            components
                .get(&mc_protocol::components::CUSTOM_NAME)
                .cloned(),
        );
    }
}

#[distributed_slice(BLOCK_ENTITY_TYPES)]
static COMMAND_BLOCK: BlockEntityRegistration = BlockEntityRegistration {
    ty: BlockEntityType::COMMAND_BLOCK,
    create: |pos, state| Box::new(CommandBlockEntity::new(pos, state)),
};
