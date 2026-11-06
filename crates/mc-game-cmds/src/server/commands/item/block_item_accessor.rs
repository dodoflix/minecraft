//! `BlockItemAccessor`: `/item ... block <pos>`.
use super::item_accessor::{ItemAccessor, ItemTarget, Setter, SlotSnapshot, provide_all};
use crate::server::commands::arg_provider::ArgProvider;
use crate::server::commands::command_response_tracker::{CommandResponseTracker, ElementType};
use crate::server::commands::item_commands::{
    ERROR_SOURCE_NOT_A_CONTAINER, ERROR_TARGET_NO_CHANGES, ERROR_TARGET_NO_CHANGES_KNOWN_ITEM,
    ERROR_TARGET_NOT_A_CONTAINER,
};
use mc_brigadier::{CommandSyntaxException, Dynamic3CommandExceptionType};
use mc_core::pos::BlockPos;
use mc_game_core::commands::arguments::coordinates::block_pos_argument::BlockPosArgument;
use mc_game_core::commands::{CommandSourceStack, argument};
use mc_game_core::server::commands::item::slots::{
    ContainerSlots, apply_container_writes, container_at, slot_params,
};
use mc_game_core::translatable;
use mc_game_core::world::item::ItemStackExt;
use mc_game_core::world::item::slot::{SlotCollection, SlotSourceRef};
use mc_game_core::world::level::storage::loot::context::LootParams;
use mc_protocol::components::ItemStack;
use mc_text::Component;

pub struct BlockItemAccessor {
    pos: BlockPos,
}

/// `BlockItemAccessor.PROVIDER`.
pub fn provider(arg: &'static str) -> ArgProvider<Box<dyn ItemAccessor>> {
    ArgProvider::create(
        "block",
        move || argument(arg, BlockPosArgument::block_pos()),
        move |c| {
            Ok(Box::new(BlockItemAccessor {
                pos: BlockPosArgument::get_loaded_block_pos(c, arg)?,
            }) as Box<dyn ItemAccessor>)
        },
    )
}

impl BlockItemAccessor {
    fn not_a_container(
        &self,
        exception: &Dynamic3CommandExceptionType<i32, i32, i32>,
    ) -> CommandSyntaxException {
        exception.create(self.pos.x(), self.pos.y(), self.pos.z())
    }
}

impl ItemAccessor for BlockItemAccessor {
    fn set_items(
        &self,
        source: &CommandSourceStack,
        slot_source: &SlotSourceRef,
        function: &mut Setter<'_>,
    ) -> Result<(), CommandSyntaxException> {
        source.with_server(|server| {
            let writes = {
                let server = &*server;
                let level = server
                    .level(source.get_level())
                    .ok_or_else(|| self.not_a_container(&ERROR_TARGET_NOT_A_CONTAINER))?;
                let container = container_at(level, self.pos)
                    .ok_or_else(|| self.not_a_container(&ERROR_TARGET_NOT_A_CONTAINER))?;
                let provider = ContainerSlots::new(container);
                let params: Vec<LootParams<'_>> = vec![slot_params(level, source, &provider)];
                let collections = provide_all(&params, slot_source);
                let pairs: Vec<(ItemTarget, SlotCollection<'_>)> = collections
                    .into_iter()
                    .map(|c| (ItemTarget::Block(self.pos), c))
                    .collect();
                function(level, &pairs);
                drop(pairs);
                drop(params);
                provider.into_writes()
            };
            if let Some(level) = server.level_mut(source.get_level()) {
                apply_container_writes(level, self.pos, writes);
            }
            Ok(())
        })
    }

    fn get_slots(
        &self,
        source: &CommandSourceStack,
        slot_source: &SlotSourceRef,
    ) -> Result<SlotSnapshot, CommandSyntaxException> {
        source.with_server(|server| {
            let level = server
                .level(source.get_level())
                .ok_or_else(|| self.not_a_container(&ERROR_SOURCE_NOT_A_CONTAINER))?;
            let container = container_at(level, self.pos)
                .ok_or_else(|| self.not_a_container(&ERROR_SOURCE_NOT_A_CONTAINER))?;
            let provider = ContainerSlots::new(container);
            let params: Vec<LootParams<'_>> = vec![slot_params(level, source, &provider)];
            let mut snapshot = SlotSnapshot::empty();
            for collection in provide_all(&params, slot_source) {
                snapshot.append(SlotSnapshot::of(&collection));
            }
            Ok(snapshot)
        })
    }

    fn get_replace_success(
        &self,
        source: &CommandSourceStack,
        tracker: &CommandResponseTracker<ItemTarget>,
        known_item: Option<&ItemStack>,
    ) -> Result<i32, CommandSyntaxException> {
        let (x, y, z) = (self.pos.x(), self.pos.y(), self.pos.z());
        match known_item {
            Some(item) => tracker.send_feedback(
                source,
                true,
                ElementType::NonZero,
                Some(|| ERROR_TARGET_NO_CHANGES_KNOWN_ITEM.create(item.get_display_name())),
                |_, slot_count| {
                    translatable!(
                        "commands.item.block.replace.success.known_item",
                        slot_count,
                        x,
                        y,
                        z,
                        item.get_display_name()
                    )
                },
                |_, _| Component::empty(),
            ),
            None => tracker.send_feedback(
                source,
                true,
                ElementType::NonZero,
                Some(|| ERROR_TARGET_NO_CHANGES.create()),
                |_, slot_count| {
                    translatable!("commands.item.block.replace.success", slot_count, x, y, z)
                },
                |_, _| Component::empty(),
            ),
        }
    }

    fn get_modify_success(
        &self,
        source: &CommandSourceStack,
        tracker: &CommandResponseTracker<ItemTarget>,
    ) -> Result<i32, CommandSyntaxException> {
        let (x, y, z) = (self.pos.x(), self.pos.y(), self.pos.z());
        tracker.send_feedback(
            source,
            true,
            ElementType::NonZero,
            Some(|| ERROR_TARGET_NO_CHANGES.create()),
            |_, slot_count| {
                translatable!("commands.item.block.modify.success", slot_count, x, y, z)
            },
            |_, _| Component::empty(),
        )
    }
}
