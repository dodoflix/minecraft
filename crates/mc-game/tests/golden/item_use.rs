//! Item use flows on a detached void level: block placement, consumables, swing gating, cooldowns, durability, drops.
//! Expected values follow the vanilla sources (`BlockItem.place`, `Consumable`, `LivingEntity.swing`, ...); the click
//! algorithms themselves are compared with the real vanilla classes in `menu_golden.rs`.

use mc_core::direction::Direction;
use mc_core::ident::Identifier;
use mc_core::phys::{BlockHitResult, Vec3};
use mc_core::pos::BlockPos;
use mc_data::Item;
use mc_data::block::Block;
use mc_game::server::level::ServerLevel;
use mc_game::world::entity::equipment::EquipmentSlot;
use mc_game::world::entity::{Entity, ServerPlayer};
use mc_game::world::item::{
    InteractionHand, InteractionResult, ItemStack, UseOnContext, hurt_and_break, item_stack_use_on,
};
use mc_game::world::level::{BlockGetter, LevelAccessor};
use mc_protocol::codec::GameProfile;
use mc_protocol::components::{DAMAGE, ItemUseAnimation};

fn level() -> ServerLevel {
    ServerLevel::detached(Identifier::parse("minecraft:overworld").unwrap())
}

fn player() -> ServerPlayer {
    let mut p = ServerPlayer::new(
        1,
        Identifier::parse("minecraft:overworld").unwrap(),
        GameProfile {
            id: uuid::Uuid::from_u128(1),
            name: "Steve".into(),
            properties: vec![],
        },
        Default::default(),
    );
    p.player.abilities.may_build = true;
    p
}

fn top_hit(pos: BlockPos) -> BlockHitResult {
    BlockHitResult {
        location: Vec3::new(
            pos.x() as f64 + 0.5,
            pos.y() as f64 + 1.0,
            pos.z() as f64 + 0.5,
        ),
        direction: Direction::Up,
        block_pos: pos,
        miss: false,
        inside: false,
        world_border_hit: false,
    }
}

fn use_on(level: &mut ServerLevel, p: &mut ServerPlayer, hit: BlockHitResult) -> InteractionResult {
    let stack = p.get_item_in_hand(InteractionHand::MainHand);
    let mut ctx = UseOnContext::new(
        Some(p.context_player()),
        InteractionHand::MainHand,
        stack,
        hit,
    );
    let result = item_stack_use_on(&mut ctx, level, p);
    p.set_item_in_hand(InteractionHand::MainHand, ctx.item_stack);
    result
}

#[test]
fn block_item_places_and_consumes() {
    let mut level = level();
    let mut p = player();
    let ground = BlockPos::new(0, 63, 0);
    level.set_block(ground, Block::STONE.default_state(), 3, 512);
    p.set_item_in_hand(
        InteractionHand::MainHand,
        ItemStack::with_count(Item::COBBLESTONE, 3),
    );
    let result = use_on(&mut level, &mut p, top_hit(ground));
    assert!(result.consumes_action());
    assert_eq!(
        level.get_block_state(ground.above()),
        Block::COBBLESTONE.default_state()
    );
    assert_eq!(p.get_item_in_hand(InteractionHand::MainHand).count(), 2);
    // the target is now occupied by a full block: the next placement on the same face fails (not replaceable)
    let again = use_on(
        &mut level,
        &mut p,
        BlockHitResult {
            block_pos: ground,
            ..top_hit(ground)
        },
    );
    assert_eq!(
        again,
        InteractionResult::FAIL,
        "the target position is occupied: `BlockItem.useOn` returns the place result (FAIL)"
    );
    assert_eq!(p.get_item_in_hand(InteractionHand::MainHand).count(), 2);
    // creative: the stack is not consumed
    p.player.abilities.instabuild = true;
    let above = ground.above();
    let r = use_on(&mut level, &mut p, top_hit(above));
    assert!(r.consumes_action());
    assert_eq!(
        level.get_block_state(above.above()),
        Block::COBBLESTONE.default_state()
    );
    assert_eq!(p.get_item_in_hand(InteractionHand::MainHand).count(), 2);
}

#[test]
fn replaceable_block_is_replaced_and_adventure_blocks_use() {
    let mut level = level();
    let mut p = player();
    let ground = BlockPos::new(4, 63, 4);
    level.set_block(ground, Block::STONE.default_state(), 3, 512);
    level.set_block(ground.above(), Block::SHORT_GRASS.default_state(), 3, 512);
    p.set_item_in_hand(
        InteractionHand::MainHand,
        ItemStack::with_count(Item::DIRT, 1),
    );
    // clicking the grass itself: `replaceClicked` -> the dirt goes into the grass' position
    let r = use_on(&mut level, &mut p, top_hit(ground.above()));
    assert!(r.consumes_action());
    assert_eq!(
        level.get_block_state(ground.above()),
        Block::DIRT.default_state()
    );
    assert!(p.get_item_in_hand(InteractionHand::MainHand).is_empty());
    // adventure mode (`mayBuild == false`) without `can_place_on`: `ItemStack.useOn` passes
    p.player.abilities.may_build = false;
    p.set_item_in_hand(
        InteractionHand::MainHand,
        ItemStack::with_count(Item::DIRT, 1),
    );
    let r = use_on(&mut level, &mut p, top_hit(ground.above()));
    assert_eq!(r, InteractionResult::PASS);
}

#[test]
fn eating_an_apple_takes_32_ticks_and_feeds() {
    let mut level = level();
    let mut p = player();
    p.player.food_data.food_level = 10;
    p.player.food_data.saturation_level = 0.0;
    p.set_item_in_hand(
        InteractionHand::MainHand,
        ItemStack::with_count(Item::APPLE, 2),
    );
    let r = p.use_item(&mut level, InteractionHand::MainHand);
    assert_eq!(r, InteractionResult::CONSUME);
    assert!(p.is_using_item());
    assert_eq!(p.get_use_item_remaining_ticks(), 32);
    for tick in 0..31 {
        p.updating_using_item(&mut level);
        assert!(p.is_using_item(), "tick {tick}");
    }
    p.updating_using_item(&mut level);
    assert!(!p.is_using_item());
    assert_eq!(p.player.food_data.food_level, 14);
    assert!(
        (p.player.food_data.saturation_level - 2.4).abs() < 1e-6,
        "{}",
        p.player.food_data.saturation_level
    );
    assert_eq!(p.get_item_in_hand(InteractionHand::MainHand).count(), 1);
    let stack = p.get_item_in_hand(InteractionHand::MainHand);
    assert_eq!(
        mc_game::world::item::behaviour(stack.item()).get_use_animation(&stack),
        ItemUseAnimation::Eat
    );
}

#[test]
fn full_player_cannot_eat_ordinary_food() {
    let mut level = level();
    let mut p = player();
    p.set_item_in_hand(
        InteractionHand::MainHand,
        ItemStack::with_count(Item::APPLE, 1),
    );
    assert_eq!(
        p.use_item(&mut level, InteractionHand::MainHand),
        InteractionResult::FAIL
    );
    assert!(!p.is_using_item());
    // a golden apple can always be eaten
    p.set_item_in_hand(
        InteractionHand::MainHand,
        ItemStack::with_count(Item::GOLDEN_APPLE, 1),
    );
    assert_eq!(
        p.use_item(&mut level, InteractionHand::MainHand),
        InteractionResult::CONSUME
    );
}

#[test]
fn every_boat_item_places_its_boat() {
    // the first `behaviour` call builds the table for every item: a boat item without a boat type must not fail it
    let _ = mc_game::world::item::behaviour(Item::POPLAR_BOAT);
    let boat_items = mc_data::EntityType::iter()
        .filter_map(mc_game::world::entity::vehicle::boat::abstract_boat::boat_spec)
        .map(|(item, _, _)| item);
    for item in boat_items {
        let mut level = level();
        let mut p = player();
        level.set_block(
            BlockPos::new(0, 62, 0),
            Block::STONE.default_state(),
            3,
            512,
        );
        p.base_mut().set_pos_raw_and_box(0.5, 63.0, 0.5);
        p.base_mut().set_rot(0.0, 90.0);
        p.set_item_in_hand(InteractionHand::MainHand, ItemStack::with_count(item, 1));
        let result = p.use_item(&mut level, InteractionHand::MainHand);
        assert!(result.consumes_action(), "{item:?}: {result:?}");
        assert!(p.get_item_in_hand(InteractionHand::MainHand).is_empty());
    }
}

#[test]
fn swing_is_gated_by_swing_state() {
    use mc_game::world::entity::player::item_use::SWING_ANIMATION_DEFAULT as ANIM;
    let mut level = level();
    let mut p = player();
    assert!(p.swing(&mut level, InteractionHand::MainHand, ANIM, true));
    // a second swing in the first half of the running one (ticks == 0 counts as "not started"): vanilla allows it
    // while `ticks <= duration / 2 && ticks > 0` is false
    assert!(p.swing(&mut level, InteractionHand::MainHand, ANIM, true));
    p.player.living.swing_state.tick();
    p.player.living.swing_state.tick();
    assert!(
        !p.swing(&mut level, InteractionHand::MainHand, ANIM, true),
        "ticks = 2 <= 3 and > 0"
    );
    for _ in 0..2 {
        p.player.living.swing_state.tick();
    }
    assert!(
        p.swing(&mut level, InteractionHand::MainHand, ANIM, true),
        "ticks = 4 > 3"
    );
}

#[test]
fn cooldowns_count_down_and_report() {
    let mut p = player();
    let stack = ItemStack::new(Item::ENDER_PEARL);
    p.player.cooldowns.add_cooldown(&stack, 3);
    assert!(p.player.cooldowns.is_on_cooldown(&stack));
    assert!(
        !p.player
            .cooldowns
            .is_on_cooldown(&ItemStack::new(Item::SNOWBALL))
    );
    assert_eq!(
        p.player.cooldowns.take_outbox(),
        vec![(Item::ENDER_PEARL.identifier(), 3)]
    );
    for _ in 0..2 {
        p.player.cooldowns.tick();
        assert!(p.player.cooldowns.is_on_cooldown(&stack));
    }
    p.player.cooldowns.tick();
    assert!(!p.player.cooldowns.is_on_cooldown(&stack));
    assert_eq!(
        p.player.cooldowns.take_outbox(),
        vec![(Item::ENDER_PEARL.identifier(), 0)]
    );
}

#[test]
fn durability_and_breaking() {
    let mut level = level();
    let mut p = player();
    let mut pick = ItemStack::new(Item::DIAMOND_PICKAXE);
    hurt_and_break(&mut pick, 5, &mut level, &mut p, EquipmentSlot::MainHand);
    assert_eq!(pick.get_damage_value(), 5);
    assert_eq!(pick.get(&DAMAGE), Some(&5));
    pick.set_damage_value(pick.get_max_damage() - 1);
    hurt_and_break(&mut pick, 1, &mut level, &mut p, EquipmentSlot::MainHand);
    assert!(pick.is_empty(), "broken tools are consumed");
    // creative players do not lose durability
    p.player.abilities.instabuild = true;
    let mut pick = ItemStack::new(Item::DIAMOND_PICKAXE);
    hurt_and_break(&mut pick, 5, &mut level, &mut p, EquipmentSlot::MainHand);
    assert_eq!(pick.get_damage_value(), 0);
}

#[test]
fn dropping_from_the_selected_slot_spawns_an_item_entity() {
    use mc_game::world::entity::item::ItemEntity;
    let mut level = level();
    let mut p = player();
    p.base_mut().set_pos_raw_and_box(0.5, 70.0, 0.5);
    p.player
        .inventory
        .set_selected_item(ItemStack::with_count(Item::STONE, 5));
    // entity ids come from a process-wide counter: scan from the first id this test can have seen
    let first = level.next_entity_id();
    let items = |level: &ServerLevel| -> Vec<i32> {
        let last = level.next_entity_id();
        let mut v: Vec<i32> = (first..=last + 64)
            .filter_map(|id| {
                level.entity_manager.get_any(id).and_then(|e| {
                    e.as_any()
                        .downcast_ref::<ItemEntity>()
                        .map(|i| i.get_item().count())
                })
            })
            .collect();
        v.sort();
        v
    };
    p.drop_selected(&mut level, false);
    assert_eq!(p.player.inventory.get_selected_item().count(), 4);
    assert_eq!(items(&level), vec![1]);
    p.drop_selected(&mut level, true);
    assert!(p.player.inventory.get_selected_item().is_empty());
    assert_eq!(items(&level), vec![1, 4]);
}
