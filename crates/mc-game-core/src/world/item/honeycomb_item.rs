//! `HoneycombItem`: waxing copper blocks (`useOn`) and signs (`SignApplicator`).

use super::{InteractionResult, ItemBehaviour, ItemStack, SignApplicator, UseOnContext};
use crate::register_item_class;
use crate::server::level::ServerLevel;
use crate::world::block_entity::BlockEntity;
use crate::world::block_entity::sign_block_entity::{SignBlockEntity, SignText, SignTextSlot};
use crate::world::entity::ServerPlayer;
use crate::world::level::block::BlockStateExt;
use crate::world::level::block::impls::chest_block;
use crate::world::level::gameevent::GameEventContext;
use crate::world::level::{BlockGetter, LevelAccessor};
use mc_core::phys::Vec3;
use mc_data::block::{Block, BlockState};
use mc_data::enums::ChestType;
use mc_data::{GameEvent, SoundEvent, props};
use mc_protocol::packets::play::SoundSource;
use std::collections::HashMap;
use std::sync::OnceLock;

/// The unwaxed `unaffected` block names of the `WeatheringCopperCollection`s in `HoneycombItem.WAXABLES`.
const WAXABLE_FAMILIES: [&str; 15] = [
    "copper_block",
    "cut_copper",
    "cut_copper_slab",
    "cut_copper_stairs",
    "chiseled_copper",
    "copper_door",
    "copper_trapdoor",
    "copper_bars",
    "copper_grate",
    "copper_bulb",
    "copper_chest",
    "copper_golem_statue",
    "lightning_rod",
    "copper_lantern",
    "copper_chain",
];

/// `HoneycombItem.WAXABLES`: unwaxed block -> waxed block.
fn waxables() -> &'static HashMap<Block, Block> {
    static MAP: OnceLock<HashMap<Block, Block>> = OnceLock::new();
    MAP.get_or_init(|| {
        let mut map = HashMap::new();
        for family in WAXABLE_FAMILIES {
            // `copper_block` -> `exposed_copper`, ...; everything else keeps its name behind the weathering prefix
            let stem = if family == "copper_block" {
                "copper"
            } else {
                family
            };
            for (prefix, unwaxed_name) in [
                ("", family),
                ("exposed_", ""),
                ("weathered_", ""),
                ("oxidized_", ""),
            ] {
                let unwaxed = if prefix.is_empty() {
                    unwaxed_name.to_owned()
                } else {
                    format!("{prefix}{stem}")
                };
                let waxed = format!("waxed_{unwaxed}");
                let unwaxed_block = Block::by_name(&format!("minecraft:{unwaxed}"))
                    .unwrap_or_else(|| panic!("no block {unwaxed}"));
                let waxed_block = Block::by_name(&format!("minecraft:{waxed}"))
                    .unwrap_or_else(|| panic!("no block {waxed}"));
                map.insert(unwaxed_block, waxed_block);
            }
        }
        map
    })
}

/// `HoneycombItem.getWaxed(oldState)`.
pub fn get_waxed(old_state: BlockState) -> Option<BlockState> {
    waxables()
        .get(&old_state.block())
        .map(|b| b.default_state().with_properties_of(old_state))
}

/// `HoneycombItem.WAXABLES` as `(unwaxed, waxed)` pairs.
pub fn waxable_pairs() -> impl Iterator<Item = (Block, Block)> {
    waxables().iter().map(|(a, b)| (*a, *b))
}

pub struct HoneycombItem;

impl ItemBehaviour for HoneycombItem {
    fn as_sign_applicator(&self) -> Option<&dyn SignApplicator> {
        Some(self)
    }
    fn use_on(
        &self,
        context: &mut UseOnContext,
        level: &mut ServerLevel,
        player: &mut ServerPlayer,
    ) -> InteractionResult {
        let pos = context.get_clicked_pos();
        let old_state = level.get_block_state(pos);
        let Some(waxed_state) = get_waxed(old_state) else {
            return InteractionResult::PASS;
        };
        let player_id = Some(player.base_id());
        crate::advancements::triggers::CriteriaTriggers::ITEM_USED_ON_BLOCK.trigger(
            level,
            player,
            pos,
            &context.item_stack,
        );
        context.item_stack.shrink(1);
        level.set_block_flags(pos, waxed_state, 11);
        level.game_event(
            GameEvent::BLOCK_CHANGE,
            Vec3::at_center_of(&pos),
            &GameEventContext::of(player_id, Some(waxed_state)),
        );
        level.level_event(player_id, 3003, pos, 0);
        level.play_sound(
            player_id,
            pos,
            SoundEvent::ITEM_HONEYCOMB_WAX_ON,
            SoundSource::Blocks,
            1.0,
            1.0,
        );
        if old_state.block().data().classes.contains(&"ChestBlock")
            && old_state.get(props::CHEST_TYPE) != ChestType::Single
        {
            let neighbor_pos = pos.relative(chest_block::get_connected_direction(old_state));
            let neighbor_state = level.get_block_state(neighbor_pos);
            level.game_event(
                GameEvent::BLOCK_CHANGE,
                Vec3::at_center_of(&neighbor_pos),
                &GameEventContext::of(player_id, Some(neighbor_state)),
            );
            level.level_event(player_id, 3003, neighbor_pos, 0);
            level.play_sound(
                player_id,
                pos,
                SoundEvent::ITEM_HONEYCOMB_WAX_ON,
                SoundSource::Blocks,
                1.0,
                1.0,
            );
        }
        InteractionResult::SUCCESS
    }
}

impl SignApplicator for HoneycombItem {
    fn can_apply_to_sign(
        &self,
        _text: &SignText,
        _item: &ItemStack,
        _player: &ServerPlayer,
    ) -> bool {
        true
    }

    fn try_apply_to_sign(
        &self,
        level: &mut ServerLevel,
        sign: &mut SignBlockEntity,
        _slot: SignTextSlot,
        _item: &ItemStack,
        _player: &ServerPlayer,
    ) -> bool {
        if sign.set_waxed(level, true) {
            let pos = sign.get_block_pos();
            level.level_event(None, 3003, pos, 0);
            level.play_sound(
                None,
                pos,
                SoundEvent::ITEM_HONEYCOMB_WAX_ON,
                SoundSource::Blocks,
                1.0,
                1.0,
            );
            true
        } else {
            false
        }
    }
}

register_item_class!("HoneycombItem", |_| Box::new(HoneycombItem));
