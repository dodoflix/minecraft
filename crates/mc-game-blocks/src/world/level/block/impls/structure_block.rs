//! `net.minecraft.world.level.block.StructureBlock`: redstone trigger, creator tracking and the editor access check.
use super::prelude::*;
use crate::server::level::ServerLevel;
use crate::world::block_entity::StructureBlockEntity;
use crate::world::item::InteractionResult;
use crate::world::level::redstone::Orientation;
use crate::world::level::signal_getter::has_neighbor_signal;
use mc_data::enums::StructureMode;

pub struct StructureBlock;

impl StructureBlock {
    /// `trigger(level, structureBlock)`.
    fn trigger(level: &mut ServerLevel, pos: BlockPos, mode: StructureMode) {
        match mode {
            StructureMode::Save => {
                StructureBlockEntity::save_structure_at(level, pos, false);
            }
            StructureMode::Load => StructureBlockEntity::place_structure(level, pos),
            StructureMode::Corner => StructureBlockEntity::unload_structure(level, pos),
            StructureMode::Data => {}
        }
    }
}

impl BlockBehaviour for StructureBlock {
    fn use_without_item(&self, _state: BlockState, level: &mut ServerLevel, pos: BlockPos, player: &mut crate::world::entity::ServerPlayer, _hit: &mc_core::phys::BlockHitResult) -> InteractionResult {
        match level.get_block_entity_immediate(pos) {
            Some(be) if be.as_any().is::<StructureBlockEntity>() && StructureBlockEntity::used_by(player.can_use_game_master_blocks()) => InteractionResult::SUCCESS,
            _ => InteractionResult::PASS,
        }
    }

    fn set_placed_by(&self, _state: BlockState, level: &mut ServerLevel, pos: BlockPos, placer: Option<&mut crate::world::entity::ServerPlayer>, _stack: &crate::world::item::ItemStack) {
        if let Some(placer) = placer {
            let name = placer.player.game_profile.name.clone();
            level.with_typed_block_entity::<StructureBlockEntity, _>(pos, |_, be| be.created_by(&name));
        }
    }

    fn neighbor_changed(&self, _state: BlockState, level: &mut ServerLevel, pos: BlockPos, _block: Block, _orientation: Option<Orientation>, _moved_by_piston: bool) {
        if !level.get_block_entity_immediate(pos).is_some_and(|be| be.as_any().is::<StructureBlockEntity>()) {
            return;
        }
        let should_trigger = has_neighbor_signal(&*level, pos);
        let Some((is_powered, mode)) = level.with_typed_block_entity::<StructureBlockEntity, _>(pos, |_, be| (be.is_powered(), be.get_mode())) else { return };
        if should_trigger && !is_powered {
            level.with_typed_block_entity::<StructureBlockEntity, _>(pos, |_, be| be.set_powered(true));
            Self::trigger(level, pos, mode);
        } else if !should_trigger && is_powered {
            level.with_typed_block_entity::<StructureBlockEntity, _>(pos, |_, be| be.set_powered(false));
        }
    }
}

register_block_class!("StructureBlock", |_| Box::new(StructureBlock));
