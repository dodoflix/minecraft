//! `net.minecraft.world.level.block.BeehiveBlock` (`bee_nest`, `beehive`).
use mc_core::ident::Identifier;
use mc_core::phys::Vec3;
use mc_data::{EntityType, GameEvent, Item, SoundEvent};
use mc_game_core::advancements::triggers::CriteriaTriggers;
use mc_game_core::server::level::ServerLevel;
use mc_game_core::stats::Stat;
use mc_game_core::world::block_entity::{BeeReleaseStatus, BeehiveBlockEntity, BlockEntity as _};
use mc_game_core::world::entity::item::ItemEntity;
use mc_game_core::world::entity::{Entity, ServerPlayer};
use mc_game_core::world::inventory::Prediction;
use mc_game_core::world::item::enchantment::enchantment_tags::PREVENTS_BEE_SPAWNS_WHEN_MINING;
use mc_game_core::world::item::{BlockPlaceContext, InteractionHand, InteractionResult, ItemStack};
use mc_game_core::world::level::block::impls::campfire_block::CampfireBlock;
use mc_game_core::world::level::block::impls::prelude::*;
use mc_game_core::world::level::explosion::{Explosion, default_on_explosion_hit};
use mc_game_core::world::level::storage::loot::block_drops::block_interact_drops;
use mc_game_core::{register_block_class, update_shape};
use mc_protocol::packets::play::level::SoundSource;

pub const MAX_HONEY_LEVELS: i32 = 5;

pub struct BeehiveBlock;

/// `BeehiveBlock.resetHoneyLevel(level, state, pos)`.
pub fn reset_honey_level(level: &mut ServerLevel, state: BlockState, pos: BlockPos) {
    level.set_block_and_update(pos, state.set(props::LEVEL_HONEY, 0));
}

/// `BeehiveBlock.dropHoneycomb(level, tool, state, blockEntity, entity, pos)`: the `harvest/beehive` loot, popped at the hive.
pub fn drop_honeycomb(
    level: &mut ServerLevel,
    tool: &ItemStack,
    state: BlockState,
    entity: Option<&dyn Entity>,
    pos: BlockPos,
) {
    for stack in block_interact_drops(level, "harvest/beehive", pos, state, Some(tool), entity) {
        helpers::pop_resource(level, pos, stack);
    }
}

/// `BeehiveBlock.releaseBeesAndResetHoneyLevel(level, state, pos, player, releaseStatus)`: the `player` is the `Bee`
/// target (not implemented, see [`BeehiveBlockEntity::empty_all_living_from_hive`]).
pub fn release_bees_and_reset_honey_level(
    level: &mut ServerLevel,
    state: BlockState,
    pos: BlockPos,
    release_status: BeeReleaseStatus,
) {
    reset_honey_level(level, state, pos);
    level.with_typed_block_entity::<BeehiveBlockEntity, _>(pos, |level, hive| {
        hive.empty_all_living_from_hive(level, state, release_status);
    });
}

impl BlockBehaviour for BeehiveBlock {
    /// `getDrops`: TNT, creepers, wither skulls, withers and TNT minecarts make the bees leave the hive angry.
    fn before_get_drops(
        &self,
        state: BlockState,
        level: &mut ServerLevel,
        pos: BlockPos,
        this_entity: Option<EntityType>,
    ) {
        if matches!(
            this_entity,
            Some(
                EntityType::TNT
                    | EntityType::CREEPER
                    | EntityType::WITHER_SKULL
                    | EntityType::WITHER
                    | EntityType::TNT_MINECART
            )
        ) {
            level.with_block_entity(pos, |level, be| {
                if let Some(hive) = be.as_any_mut().downcast_mut::<BeehiveBlockEntity>() {
                    hive.empty_all_living_from_hive(level, state, BeeReleaseStatus::Emergency);
                }
            });
        }
    }
    fn on_explosion_hit(
        &self,
        state: BlockState,
        level: &mut ServerLevel,
        pos: BlockPos,
        explosion: &dyn Explosion,
        on_hit: &mut dyn FnMut(ItemStack, BlockPos),
    ) {
        default_on_explosion_hit(state, level, pos, explosion, on_hit);
        // Not implemented: `angerNearbyBees(level, pos)` (needs the `Bee` entity).
    }
    fn use_item_on(
        &self,
        stack: &ItemStack,
        state: BlockState,
        level: &mut ServerLevel,
        pos: BlockPos,
        player: &mut ServerPlayer,
        hand: InteractionHand,
        _hit: &mc_core::phys::BlockHitResult,
    ) -> InteractionResult {
        let mut hive_emptied = false;
        if state.get(props::LEVEL_HONEY) >= MAX_HONEY_LEVELS {
            let at = player.base().position();
            let (x, y, z) = (at.x, at.y, at.z);
            let mut held = stack.clone();
            if stack.is(Item::SHEARS) {
                drop_honeycomb(level, stack, state, Some(&*player), pos);
                level.play_sound_at(
                    None,
                    x,
                    y,
                    z,
                    SoundEvent::BLOCK_BEEHIVE_SHEAR,
                    SoundSource::Blocks,
                    1.0,
                    1.0,
                );
                mc_game_core::world::item::hurt_and_break(
                    &mut held,
                    1,
                    level,
                    player,
                    hand.as_equipment_slot(),
                );
                player.set_item_in_hand(hand, held);
                hive_emptied = true;
                level.game_event_from_entity_ref_in_state(
                    &*player,
                    None,
                    GameEvent::SHEAR,
                    Vec3::at_center_of(&pos),
                );
            } else if stack.is(Item::GLASS_BOTTLE) {
                held.shrink(1);
                level.play_sound_at(
                    Some(player.base_id()),
                    x,
                    y,
                    z,
                    SoundEvent::ITEM_BOTTLE_FILL,
                    SoundSource::Blocks,
                    1.0,
                    1.0,
                );
                let mut honey = ItemStack::new(Item::HONEY_BOTTLE);
                if held.is_empty() {
                    player.set_item_in_hand(hand, honey);
                } else {
                    player.set_item_in_hand(hand, held);
                    let infinite = player.has_infinite_materials();
                    if !player.player.inventory.add(&mut honey, infinite) {
                        player.drop(
                            level,
                            ItemStack::new(Item::HONEY_BOTTLE),
                            false,
                            Prediction::Predicted,
                        );
                    }
                }
                hive_emptied = true;
                level.game_event_from_entity_ref_in_state(
                    &*player,
                    None,
                    GameEvent::FLUID_PICKUP,
                    Vec3::at_center_of(&pos),
                );
            }
            if hive_emptied {
                player.award_stat_once(level, Stat::Used(stack.item()));
            }
        }
        if !hive_emptied {
            return InteractionResult::TRY_WITH_EMPTY_HAND;
        }
        if CampfireBlock::is_smokey_pos(&*level, pos) {
            reset_honey_level(level, state, pos);
        } else {
            // Not implemented: `angerNearbyBees(level, pos)` when the hive holds bees (needs the `Bee` entity).
            release_bees_and_reset_honey_level(level, state, pos, BeeReleaseStatus::Emergency);
        }
        InteractionResult::SUCCESS
    }
    fn player_will_destroy(
        &self,
        state: BlockState,
        level: &mut ServerLevel,
        pos: BlockPos,
        player: &mut ServerPlayer,
    ) -> BlockState {
        if player.prevents_block_drops()
            && level.block_drops()
            && let Some(hive) = level
                .get_block_entity(pos)
                .and_then(|be| be.as_any().downcast_ref::<BeehiveBlockEntity>())
        {
            let honey_level = state.get(props::LEVEL_HONEY);
            if !hive.is_empty() || honey_level > 0 {
                let mut stack = ItemStack::new(state.block().item().expect("beehive item"));
                for (t, v) in hive.collect_components().iter() {
                    stack.set_value(t, Some(v.clone()));
                }
                helpers::set_block_state_property(
                    &mut stack,
                    "honey_level",
                    &honey_level.to_string(),
                );
                let mut entity = ItemEntity::with_stack(
                    level,
                    f64::from(pos.x()),
                    f64::from(pos.y()),
                    f64::from(pos.z()),
                    stack,
                );
                entity.set_default_pick_up_delay();
                level.add_fresh_entity(Box::new(entity));
            }
        }
        mc_game_core::server::level::block_break::player_will_destroy(level, pos, state, player)
    }
    fn player_destroy(
        &self,
        level: &mut ServerLevel,
        player: &mut ServerPlayer,
        pos: BlockPos,
        state: BlockState,
        block_entity: Option<&dyn mc_game_core::world::block_entity::BlockEntity>,
        destroyed_with: &ItemStack,
    ) {
        mc_game_core::server::level::block_break::player_destroy(
            level,
            player,
            pos,
            state,
            block_entity,
            destroyed_with,
        );
        if let Some(hive) =
            block_entity.and_then(|be| be.as_any().downcast_ref::<BeehiveBlockEntity>())
        {
            let prevents =
                Identifier::try_parse(PREVENTS_BEE_SPAWNS_WHEN_MINING).is_some_and(|tag| {
                    mc_game_core::world::item::enchantment_helper::has_tag(
                        &level.registries,
                        destroyed_with,
                        &tag,
                    )
                });
            let mut remaining = hive.get_occupant_count();
            if !prevents {
                // the snapshot of the destroyed hive: release its bees through a detached copy (the original is gone)
                let mut copy = BeehiveBlockEntity::new(pos, state);
                copy.set_bees(&mc_protocol::components::Bees {
                    bees: hive.get_bees(),
                });
                copy.saved_flower_pos = hive.saved_flower_pos;
                copy.empty_all_living_from_hive(level, state, BeeReleaseStatus::Emergency);
                level.update_neighbour_for_output_signal(pos, state.block());
                // Not implemented: `angerNearbyBees(level, pos)` (needs the `Bee` entity).
                remaining = copy.get_occupant_count();
            }
            CriteriaTriggers::BEE_NEST_DESTROYED.trigger(
                level,
                player,
                state,
                destroyed_with,
                remaining as i32,
            );
        }
    }
    fn get_state_for_placement(
        &self,
        block: Block,
        _level: &mut ServerLevel,
        context: &BlockPlaceContext,
    ) -> Option<BlockState> {
        Some(block.default_state().set(
            props::HORIZONTAL_FACING,
            context.get_horizontal_direction().get_opposite(),
        ))
    }
    fn get_analog_output_signal(
        &self,
        state: BlockState,
        _level: &ServerLevel,
        _pos: BlockPos,
        _direction: Direction,
    ) -> i32 {
        state.get(props::LEVEL_HONEY)
    }
    fn get_clone_item_stack(
        &self,
        state: BlockState,
        _level: &ServerLevel,
        _pos: BlockPos,
        include_data: bool,
    ) -> ItemStack {
        let mut stack = match state.block().item() {
            Some(item) => ItemStack::new(item),
            None => ItemStack::empty(),
        };
        if include_data {
            helpers::set_block_state_property(
                &mut stack,
                "honey_level",
                &state.get(props::LEVEL_HONEY).to_string(),
            );
        }
        stack
    }
    update_shape!(
        |self, state, level, pos, direction, neighbour_pos, neighbour_state| {
            if helpers::is_instance_of(level.get_block_state(neighbour_pos), "FireBlock")
                && let Some(server_level) = level.as_server_level_mut()
            {
                server_level.with_block_entity(pos, |level, be| {
                    if let Some(hive) = be.as_any_mut().downcast_mut::<BeehiveBlockEntity>() {
                        hive.empty_all_living_from_hive(
                            level,
                            state,
                            mc_game_core::world::block_entity::BeeReleaseStatus::Emergency,
                        );
                    }
                });
            }
            state
        }
    );
    fn rotate(&self, state: BlockState, rotation: Rotation) -> BlockState {
        helpers::rotate_facing(state, props::HORIZONTAL_FACING, rotation)
    }
    fn mirror(&self, state: BlockState, mirror: Mirror) -> BlockState {
        helpers::mirror_facing(state, props::HORIZONTAL_FACING, mirror)
    }
}

register_block_class!("BeehiveBlock", |_| Box::new(BeehiveBlock));
