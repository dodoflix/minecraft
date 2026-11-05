//! `BlockTransformer` (`core.component.BlockTransformer`): the data-driven `block_transformer` registry behind axes,
//! hoes and shovels (`Item.useOn` with the `block_transformer` component). Decoded together with the feature layer
//! (`GenRegistries`: the block state providers and block predicates of the transform rules are worldgen codecs) and
//! evaluated through [`transform_block`], installed as the item hook in [`install`].

use super::{InteractionResult, ItemStack, ItemStackExt, UseOnContext, hurt_and_break};
use crate::server::level::ServerLevel;
use crate::world::level::{BlockGetter, LevelAccessor, LevelReader};
use crate::world::entity::equipment::EquipmentSlot;
use crate::world::entity::{Entity, ServerPlayer};
use crate::world::level::block::impls::chest_block;
use crate::world::level::block::{BlockStateExt, helpers};
use crate::world::level::gameevent::GameEventContext;
use crate::world::level::levelgen::codec::{Fields, LoadError, LoadResult, parse_bool, parse_direction, parse_identifier, parse_int, parse_list, parse_str};
use crate::world::level::levelgen::registries::DecodeCtx;
use crate::world::level::levelgen::stateproviders::BlockStateProvider;
use mc_core::direction::Direction;
use mc_core::ident::Identifier;
use mc_core::phys::Vec3;
use mc_core::random::RandomSource;
use mc_data::enums::ChestType;
use mc_data::{DataComponentType, GameEvent, SoundEvent, props};
use mc_protocol::components::BLOCK_TRANSFORMER;
use mc_protocol::packets::play::SoundSource;
use serde_json::Value;
use std::sync::Arc;

/// Registry key of `Registries.BLOCK_TRANSFORMER` (path under `data/<ns>/`).
pub const BLOCK_TRANSFORMER_REGISTRY: &str = "block_transformer";

/// `BlockTransformer.TransformParticle`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TransformParticle {
    None,
    Scrape,
    WaxOn,
    WaxOff,
}

impl TransformParticle {
    fn by_name(name: &str) -> Option<Self> {
        Some(match name {
            "none" => Self::None,
            "scrape" => Self::Scrape,
            "wax_on" => Self::WaxOn,
            "wax_off" => Self::WaxOff,
            _ => return None,
        })
    }

    fn level_event(self) -> i32 {
        match self {
            Self::None => 0,
            Self::Scrape => 3005,
            Self::WaxOn => 3003,
            Self::WaxOff => 3004,
        }
    }

    /// `send(level, player, pos)`.
    pub fn send(self, level: &mut ServerLevel, player: Option<i32>, pos: mc_core::pos::BlockPos) {
        if self != Self::None {
            level.level_event(player, self.level_event(), pos, 0);
        }
    }
}

/// `BlockTransformer.DropStrategy`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DropStrategy {
    ClickedFace,
    FromMiddle,
}

/// `BlockTransformer.TransformType`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TransformType {
    SingleBlock,
    CopperChest,
}

/// `BlockTransformer.BlockTransformData`.
pub struct BlockTransformData {
    pub block_state_provider: Arc<dyn BlockStateProvider>,
    pub sound: SoundEvent,
    pub particle: TransformParticle,
    pub disallowed_faces: Vec<Direction>,
    pub loot: Option<Identifier>,
    pub drop_strategy: DropStrategy,
    pub update_from_neighbors: bool,
    pub transform_type: TransformType,
    pub consume_on_use: bool,
    pub item_damage_per_use: i32,
}

/// `BlockTransformer`.
pub struct BlockTransformer {
    pub transforms: Vec<BlockTransformData>,
}

fn parse_enum<T>(value: &Value, by_name: impl Fn(&str) -> Option<T>) -> LoadResult<T> {
    let name = parse_str(value)?;
    by_name(name).ok_or_else(|| LoadError::new(format!("Unknown element name:{name}")))
}

/// `SoundEvent.CODEC` (`RegistryFileCodec` over `SoundEvent.DIRECT_CODEC`): a registered id, or an inline definition.
fn parse_sound(value: &Value) -> LoadResult<SoundEvent> {
    match value {
        Value::String(_) => {
            let id = parse_identifier(value)?;
            SoundEvent::by_name(&id.to_string()).ok_or_else(|| LoadError::new(format!("Failed to get element ResourceKey[minecraft:sound_event / {id}]")))
        }
        // `SoundEvent.DIRECT_CODEC` (`sound_id` + optional `range`): the id selects the registered sound
        other => {
            let fields = Fields::new(other)?;
            let id = fields.required("sound_id", parse_identifier)?;
            SoundEvent::by_name(&id.to_string()).ok_or_else(|| LoadError::new(format!("Unknown sound event {id}")))
        }
    }
}

impl BlockTransformData {
    /// `BlockTransformData.CODEC`.
    fn decode(value: &Value, ctx: &DecodeCtx<'_>) -> LoadResult<BlockTransformData> {
        let fields = Fields::new(value)?;
        let provider = fields.required("block_state_provider", |v| ctx.block_state_provider(v))?;
        let sound = fields.optional("sound", parse_sound)?.unwrap_or_else(|| SoundEvent::by_name("minecraft:intentionally_empty").expect("intentionally_empty sound"));
        let particle = fields.optional("particle", |v| parse_enum(v, TransformParticle::by_name))?.unwrap_or(TransformParticle::None);
        let disallowed_faces = fields.optional("disallowed_faces", |v| parse_list(v, parse_direction))?.unwrap_or_default();
        let loot = fields.optional("loot", parse_identifier)?;
        let drop_strategy = fields
            .optional("drop_strategy", |v| {
                parse_enum(v, |n| match n {
                    "clicked_face" => Some(DropStrategy::ClickedFace),
                    "from_middle" => Some(DropStrategy::FromMiddle),
                    _ => None,
                })
            })?
            .unwrap_or(DropStrategy::FromMiddle);
        let update_from_neighbors = fields.optional("update_from_neighbors", parse_bool)?.unwrap_or(true);
        let transform_type = fields
            .optional("transform_type", |v| {
                parse_enum(v, |n| match n {
                    "single_block" => Some(TransformType::SingleBlock),
                    "copper_chest" => Some(TransformType::CopperChest),
                    _ => None,
                })
            })?
            .unwrap_or(TransformType::SingleBlock);
        let consume_on_use = fields.optional("consume_on_use", parse_bool)?.unwrap_or(true);
        let item_damage_per_use = fields
            .optional("item_damage_per_use", |v| {
                let n = parse_int(v)?;
                if n < 0 { Err(LoadError::new(format!("Value {n} outside of range [0:2147483647]"))) } else { Ok(n) }
            })?
            .unwrap_or(0);
        Ok(BlockTransformData {
            block_state_provider: provider.value().clone(),
            sound,
            particle,
            disallowed_faces,
            loot,
            drop_strategy,
            update_from_neighbors,
            transform_type,
            consume_on_use,
            item_damage_per_use,
        })
    }
}

impl BlockTransformer {
    /// `BlockTransformer.DIRECT_CODEC`: `BlockTransformData.CODEC.listOf(1, 200)`.
    pub fn decode(value: &Value, ctx: &DecodeCtx<'_>) -> LoadResult<Arc<BlockTransformer>> {
        let transforms = parse_list(value, |v| BlockTransformData::decode(v, ctx))?;
        if !(1..=200).contains(&transforms.len()) {
            return Err(LoadError::new(format!("List length ({}) not in range [1, 200]", transforms.len())));
        }
        Ok(Arc::new(BlockTransformer { transforms }))
    }
}

/// `Item.useOn(context)` of an item with the `block_transformer` component (`PASS` without it).
pub fn use_on(context: &mut UseOnContext, level: &mut ServerLevel, player: &mut ServerPlayer) -> InteractionResult {
    let Some(holder) = context.item_stack.get(&BLOCK_TRANSFORMER) else { return InteractionResult::PASS };
    let Some(name) = holder.name() else { return InteractionResult::PASS };
    let Some(transformer) = level.gen_env.as_ref().and_then(|env| env.registries.block_transformers.get(name)).map(|h| h.value().clone()) else {
        return InteractionResult::PASS;
    };
    transform_block(&transformer, context, level, player)
}

/// `BlockTransformer.playerHasBlockingItemUseIntent(context)`.
fn player_has_blocking_item_use_intent(context: &UseOnContext, player: &ServerPlayer) -> bool {
    context.hand == super::InteractionHand::MainHand
        && player.player.inventory.equipment_get(EquipmentSlot::OffHand).has(DataComponentType::BLOCKS_ATTACKS)
        && !context.is_secondary_use_active()
}

/// `BlockTransformer.transformBlock(context)`.
pub fn transform_block(transformer: &BlockTransformer, context: &mut UseOnContext, level: &mut ServerLevel, player: &mut ServerPlayer) -> InteractionResult {
    if player_has_blocking_item_use_intent(context, player) {
        return InteractionResult::PASS;
    }
    let pos = context.get_clicked_pos();
    for data in &transformer.transforms {
        let clicked_face = context.get_clicked_face();
        if data.disallowed_faces.contains(&clicked_face) {
            continue;
        }
        // `getOptionalState(level, level.getRandom(), pos)`: the provider reads the level while drawing from its RNG
        let mut random = level.tick_state.random.clone();
        let new_state = data.block_state_provider.get_optional_state(&*level as &dyn LevelAccessor, &mut random as &mut dyn RandomSource, pos);
        level.tick_state.random = random;
        let Some(new_state) = new_state else { continue };
        let updated_shape = if data.update_from_neighbors { helpers::update_from_neighbour_shapes(new_state, level, pos) } else { new_state };
        let player_id = player.base_id();
        crate::advancements::triggers::CriteriaTriggers::ITEM_USED_ON_BLOCK.trigger(level, player, pos, &context.item_stack);
        let old_state = level.get_block_state(pos);
        if let Some(loot) = &data.loot {
            let drops = drop_from_block_interact_loot_table(level, loot, pos, old_state, &context.item_stack, player);
            for stack in drops {
                match data.drop_strategy {
                    DropStrategy::ClickedFace => helpers::pop_resource_from_face(level, pos, clicked_face, stack),
                    DropStrategy::FromMiddle => helpers::pop_resource(level, pos, stack),
                }
            }
        }
        let infinite = player.player.abilities.instabuild;
        if context.item_stack.is_stackable() {
            context.item_stack.consume(if data.consume_on_use { 1 } else { 0 }, infinite);
        } else {
            hurt_and_break(&mut context.item_stack, data.item_damage_per_use, level, player, context.hand.as_equipment_slot());
        }
        level.set_block_flags(pos, updated_shape, 11);
        level.play_sound(Some(player_id), pos, data.sound, SoundSource::Blocks, 1.0, 1.0);
        data.particle.send(level, Some(player_id), pos);
        level.game_event(GameEvent::BLOCK_CHANGE, Vec3::at_center_of(&pos), &GameEventContext::of(Some(player_id), Some(updated_shape)));
        if data.transform_type == TransformType::CopperChest
            && old_state.block().data().classes.contains(&"CopperChestBlock")
            && old_state.get(props::CHEST_TYPE) != ChestType::Single
        {
            let neighbor_pos = pos.relative(chest_block::get_connected_direction(old_state));
            let neighbor_state = level.get_block_state(neighbor_pos);
            level.game_event(GameEvent::BLOCK_CHANGE, Vec3::at_center_of(&neighbor_pos), &GameEventContext::of(Some(player_id), Some(neighbor_state)));
            data.particle.send(level, Some(player_id), neighbor_pos);
        }
        return InteractionResult::SUCCESS;
    }
    InteractionResult::PASS
}

/// `Block.dropFromBlockInteractLootTable(level, key, pos, state, blockEntity, tool, interactingEntity, consumer)`: the
/// rolled drops (the consumer pops them once the level is mutable again).
fn drop_from_block_interact_loot_table(level: &ServerLevel, key: &Identifier, pos: mc_core::pos::BlockPos, state: mc_data::block::BlockState, tool: &ItemStack, player: &ServerPlayer) -> Vec<ItemStack> {
    let mut drops = Vec::new();
    crate::world::level::storage::loot::block_drops::drop_from_loot_table(
        level,
        key,
        |params| {
            params
                .block_state(state)
                .origin(Vec3::at_center_of(&pos))
                .block_entity(level.get_block_entity(pos))
                .interacting_entity(Some(player as &dyn Entity))
                .tool(tool)
                .create(&crate::world::level::storage::loot::parameters::LootContextParamSets::BLOCK_INTERACT)
        },
        &mut |stack| drops.push(stack),
    );
    drops
}
