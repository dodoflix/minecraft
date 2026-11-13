//! Player actions of the scenario DSL, replayed the same way by the block replay (`block_scenario_actors.rs`), the entity replay
//! (`entity_scenario.rs`) and the item entity replay: `give`, `select`, `use`, `use_on [inside]`, `release`, `interact`, `drop [all]`,
//! `swing`, `swap`, `stab`, `pcommand` (sprint, sleep, gliding and open inventory actions), the menu verbs of `menu_actions.rs`, `dispense X Y Z` and the dump kinds `use:REF`, `cooldowns:REF`, `stats:REF`, `inventory:REF` (the
//! grammar of the Java driver, `tests/oracle/EntityOracleDriver.java`).
//!
//! The Rust level has no server, so the verbs do what the packet handlers of `ServerGamePacketListenerImpl` do around the player
//! methods (acknowledge the block changes, resolve the swing animation before the use, swing on success, stop using on a hotbar change),
//! not what they decode. A harness implements [`ActionHost`] (its level, its refs, the packet sequence counter, its output) and
//! forwards unknown commands to [`command`] and unknown dump kinds to [`dump`]. A verb needs one new arm here, not one per harness.
use mc_core::direction::Direction;
use mc_core::phys::{BlockHitResult, Vec3};
use mc_core::pos::BlockPos;
use mc_data::Item;
use mc_game::server::level::ServerLevel;
use mc_game::world::container::Container as _;
use mc_game::world::entity::equipment::EquipmentSlot;
use mc_game::world::entity::player::item_use::SWING_ANIMATION_DEFAULT;
use mc_game::world::entity::{Entity as _, EntityId, ServerPlayer};
use mc_game::world::item::{
    InteractionHand, InteractionResult, ItemStack, SwingSource, piercing_weapon,
};
use mc_game::world::level::block::BlockStateExt;
use mc_game::world::level::{BlockGetter, LevelReader};
use mc_protocol::components::{ATTACK_ANIMATION, INTERACT_ANIMATION, PIERCING_WEAPON};
use mc_protocol::packets::play::{ClientboundBlockUpdatePacket, SwingAnimation};
use std::collections::HashMap;

/// What the verbs need from a replay harness.
pub trait ActionHost {
    /// The first world command builds the world (the harness's lazy boot).
    fn boot(&mut self);
    fn action_level(&mut self) -> &mut ServerLevel;
    /// The entity a scenario name refers to (panics on an unknown ref).
    fn action_ref(&self, name: &str) -> EntityId;
    /// The driver's `seq(player)`: the sequence number of the player's next `use`/`use_on` packet.
    fn next_sequence(&mut self, player: &str) -> i32;
    /// One line of the replay output.
    fn emit(&mut self, line: String);
}

/// The per player packet counters behind [`ActionHost::next_sequence`].
#[derive(Default)]
pub struct Sequences(HashMap<String, i32>);

impl Sequences {
    pub fn next(&mut self, player: &str) -> i32 {
        let n = self.0.entry(player.to_owned()).or_insert(0);
        *n += 1;
        *n
    }
}

/// The driver's `stack(spec, count)`: an item id, or an item with components (`minecraft:iron_pickaxe[enchantments={...}]`).
pub fn item_stack(spec: &str, count: i32) -> ItemStack {
    if spec.contains('[') {
        use mc_game::commands::CommandBuildContext;
        use mc_game::commands::command_build_context::StaticRegistries;
        let context = CommandBuildContext::simple(
            std::sync::Arc::new(StaticRegistries),
            mc_world::level_storage::FeatureFlagSet::ALL,
        );
        let mut reader = mc_brigadier::StringReader::new(spec);
        let mut stack = mc_game::commands::arguments::item::item_parser::ItemParser::new(&context)
            .parse(&mut reader)
            .and_then(|input| input.create_item_stack(1))
            .unwrap_or_else(|e| panic!("stack {spec}: {e:?}"));
        // like the driver: counts above the item's maximum stack size are allowed
        stack.set_count(count);
        return stack;
    }
    let item = Item::by_name(spec).unwrap_or_else(|| panic!("unknown item {spec}"));
    ItemStack::with_count(item, count)
}

/// The driver's `give` slot names that are equipment slots.
pub fn equipment_slot(slot: &str) -> EquipmentSlot {
    match slot {
        "mainhand" => EquipmentSlot::MainHand,
        "offhand" => EquipmentSlot::OffHand,
        "head" => EquipmentSlot::Head,
        "chest" => EquipmentSlot::Chest,
        "legs" => EquipmentSlot::Legs,
        "feet" => EquipmentSlot::Feet,
        other => panic!("slot {other}"),
    }
}

/// The driver's `give` on a player: `hotbarN`/`invN` are `Inventory.setItem(N, stack)`, the rest equipment slots.
pub fn give(p: &mut ServerPlayer, slot: &str, stack: ItemStack) {
    match slot
        .strip_prefix("hotbar")
        .or_else(|| slot.strip_prefix("inv"))
    {
        Some(n) => {
            let n = n.parse().unwrap_or_else(|_| panic!("slot {slot}"));
            p.player.inventory.set_item(n, stack);
        }
        None => {
            p.player
                .inventory
                .equipment_set(equipment_slot(slot), stack);
        }
    }
}

/// `main|off` at token `i` (anything else is the main hand, like the driver's `hand(a, i)`).
pub fn hand(a: &[String], i: usize) -> InteractionHand {
    if a.get(i).map(String::as_str) == Some("off") {
        InteractionHand::OffHand
    } else {
        InteractionHand::MainHand
    }
}

pub(super) fn with_player<H: ActionHost, R>(
    h: &mut H,
    name: &str,
    f: impl FnOnce(&mut ServerLevel, &mut ServerPlayer) -> R,
) -> R {
    let id = h.action_ref(name);
    h.action_level()
        .with_player(id, f)
        .unwrap_or_else(|| panic!("{name} is not a player"))
}

/// `ItemStack.getInteractAnimation()` of the stack in `hand`, read before the item is used (it may be gone afterwards).
fn interact_animation(p: &ServerPlayer, hand: InteractionHand) -> SwingAnimation {
    p.get_item_in_hand(hand)
        .get(&INTERACT_ANIMATION)
        .copied()
        .unwrap_or(SWING_ANIMATION_DEFAULT)
}

/// `swingAndResetAttackStrength` when the result is a `Success` that swings.
fn swing_on_success(
    level: &mut ServerLevel,
    p: &mut ServerPlayer,
    hand: InteractionHand,
    animation: SwingAnimation,
    result: &InteractionResult,
) {
    if let InteractionResult::Success(s) = result
        && s.should_swing()
        && p.swing(
            level,
            hand,
            animation,
            s.swing_source != SwingSource::Predicted,
        )
    {
        p.reset_attack_strength_ticker();
    }
}

/// `ackBlockChangesUpTo(sequence)` (players without a listener have nothing to acknowledge).
pub fn acknowledge(p: &mut ServerPlayer, sequence: i32) {
    if let Some(listener) = p.listener.as_mut() {
        listener
            .ack_block_changes_up_to(sequence)
            .expect("positive sequence");
    }
}

/// `handleUseItem`: acknowledge, nothing for an empty or disabled stack, `ServerPlayerGameMode.useItem`, swing on a swinging success.
fn use_item(level: &mut ServerLevel, p: &mut ServerPlayer, a: &[String], sequence: i32) {
    acknowledge(p, sequence);
    let hand = hand(a, 2);
    let stack = p.get_item_in_hand(hand);
    if stack.is_empty() || !mc_game::world::item::is_item_enabled(&stack, level.enabled_features())
    {
        return;
    }
    let animation = interact_animation(p, hand);
    let result = use_item_with_listener_effects(level, p, hand);
    swing_on_success(level, p, hand, animation, &result);
}

/// `ServerPlayerGameMode.useItem` plus what the listener adds after it (`ItemSystems::use_item`: a book raised by the item is
/// resolved and opened). That half needs the server, so it runs only in scenarios with the `command_env` verb.
fn use_item_with_listener_effects(
    level: &mut ServerLevel,
    p: &mut ServerPlayer,
    hand: InteractionHand,
) -> InteractionResult {
    use mc_game::commands::server_access::{has_server, with_server};
    use mc_game::server::network::game_packets::PlayCtx;
    use mc_game::server::network::game_packets::hooks::ITEM_SYSTEMS;
    if !has_server() {
        return p.use_item(level, hand);
    }
    with_server(|server| {
        let mut ctx = PlayCtx {
            server,
            level,
            player: p,
        };
        ITEM_SYSTEMS
            .first()
            .expect("item systems")
            .use_item(&mut ctx, hand)
    })
}

/// `ServerboundUseItemOnPacket` minus the packet listener: reach and hit-location checks, then `ServerPlayerGameMode.useItemOn`.
/// `use_on PLAYER X Y Z FACE [main|off] [cx cy cz] [inside]`.
fn use_on(level: &mut ServerLevel, p: &mut ServerPlayer, a: &[String], sequence: i32) {
    let inside = a.iter().any(|t| t == "inside");
    let a: Vec<&String> = a.iter().filter(|t| *t != "inside").collect();
    let num = |t: &String| {
        t.parse::<f64>()
            .unwrap_or_else(|_| panic!("not a number: {t}"))
    };
    let pos = BlockPos::new(
        a[2].parse().unwrap(),
        a[3].parse().unwrap(),
        a[4].parse().unwrap(),
    );
    let face = Direction::by_name(a[5]).unwrap_or_else(|| panic!("face {}", a[5]));
    let hit = if a.len() > 9 {
        let n = a.len();
        Vec3::new(
            f64::from(pos.x()) + num(a[n - 3]),
            f64::from(pos.y()) + num(a[n - 2]),
            f64::from(pos.z()) + num(a[n - 1]),
        )
    } else {
        let c = Vec3::at_center_of(&pos);
        Vec3::new(
            c.x + f64::from(face.get_step_x()) * 0.5,
            c.y + f64::from(face.get_step_y()) * 0.5,
            c.z + f64::from(face.get_step_z()) * 0.5,
        )
    };
    let hand = if a.get(6).is_some_and(|t| t.as_str() == "off") {
        InteractionHand::OffHand
    } else {
        InteractionHand::MainHand
    };
    let result = BlockHitResult {
        location: hit,
        direction: face,
        block_pos: pos,
        miss: false,
        inside,
        world_border_hit: false,
    };
    acknowledge(p, sequence);
    if !mc_game::world::item::is_item_enabled(&p.get_item_in_hand(hand), level.enabled_features()) {
        return;
    }
    if !p.is_within_block_interaction_range(pos, 1.0) {
        return;
    }
    let d = hit.subtract(Vec3::at_center_of(&pos));
    if !(d.x.abs() < 1.0000001 && d.y.abs() < 1.0000001 && d.z.abs() < 1.0000001) {
        return;
    }
    // `handleUseItemOn`: nothing is used while a teleport waits for the client's confirmation (the block updates still go out)
    let awaiting_teleport = p
        .listener
        .as_ref()
        .is_some_and(|l| l.is_awaiting_position_from_client());
    if !awaiting_teleport {
        let animation = interact_animation(p, hand);
        let outcome = p.use_item_on(level, hand, &result);
        // a successful use swings (the decompiled handler swings in two branches, both reach the client)
        for _ in 0..2 {
            swing_on_success(level, p, hand, animation, &outcome);
        }
    }
    // ... and the client gets the clicked and the adjacent block back
    for at in [pos, pos.relative(face)] {
        p.send(&ClientboundBlockUpdatePacket {
            pos: at,
            block_state: level.get_block_state(at),
        });
    }
}

/// `handleInteract`: `interact PLAYER REF [main|off] [sneak] [center]` (location = the entity origin, `center` = bounding box centre
/// relative to it).
fn interact(level: &mut ServerLevel, p: &mut ServerPlayer, target: EntityId, a: &[String]) {
    let hand = hand(a, 3);
    let sneak = a.iter().any(|t| t == "sneak");
    let center = a.iter().any(|t| t == "center");
    let (block_pos, bounds, location) = {
        let t = level
            .get_entity(target)
            .unwrap_or_else(|| panic!("interact target {} is not in the level", a[2]));
        let bounds = t.base().bounding_box();
        let location = if center {
            bounds.get_center().subtract(t.position())
        } else {
            Vec3::ZERO
        };
        (t.block_position(), bounds, location)
    };
    p.set_shift_key_down(sneak);
    if !level.is_within_world_border(block_pos)
        || !p.is_within_entity_interaction_range(&bounds, 3.0)
        || !mc_game::world::item::is_item_enabled(
            &p.get_item_in_hand(hand),
            level.enabled_features(),
        )
    {
        return;
    }
    let animation = interact_animation(p, hand);
    let result = p.interact_on(level, target, hand, location);
    swing_on_success(level, p, hand, animation, &result);
}

/// `handleAttack`: range checks, then `Player.attack(target)` (an item entity, an orb or the player itself as target is a protocol
/// violation that vanilla answers with a disconnect: a scenario never does that).
fn attack(level: &mut ServerLevel, p: &mut ServerPlayer, target: EntityId) {
    if p.is_spectator() {
        return;
    }
    let (block_pos, bounds, ty) = {
        let Some(t) = level.get_entity(target) else {
            return;
        };
        (t.block_position(), t.base().bounding_box(), t.get_type())
    };
    let main_hand = p.get_item_in_hand(InteractionHand::MainHand);
    if !level.is_within_world_border(block_pos)
        || !p.is_within_attack_range(&main_hand, &bounds, 3.0)
        || p.main_hand_has_piercing_weapon()
    {
        return;
    }
    assert!(
        ty != mc_data::EntityType::ITEM
            && ty != mc_data::EntityType::EXPERIENCE_ORB
            && target != p.id(),
        "attack on an entity vanilla disconnects the player for"
    );
    if mc_game::world::item::is_item_enabled(&main_hand, level.enabled_features())
        && !p.cannot_attack_with_item(&main_hand, 5)
    {
        p.attack(level, target);
    }
}

/// `handlePunch`: swing the main hand with the attack animation of the held item.
fn swing(level: &mut ServerLevel, p: &mut ServerPlayer) {
    let animation = p
        .get_item_in_hand(InteractionHand::MainHand)
        .get(&ATTACK_ANIMATION)
        .copied()
        .unwrap_or(SWING_ANIMATION_DEFAULT);
    p.swing(level, InteractionHand::MainHand, animation, false);
    p.reset_attack_strength_ticker();
}

/// `SWAP_ITEM_WITH_OFFHAND` of `handlePlayerAction`: swap the hands, then stop using the item.
fn swap(level: &mut ServerLevel, p: &mut ServerPlayer) {
    if p.is_spectator() {
        return;
    }
    let in_offhand = p.get_item_in_hand(InteractionHand::OffHand);
    let in_main = p.get_item_in_hand(InteractionHand::MainHand);
    p.set_item_in_hand_slot(level, InteractionHand::OffHand, in_main);
    p.set_item_in_hand_slot(level, InteractionHand::MainHand, in_offhand);
    p.stop_using_item(level);
}

/// `STAB` of `handlePlayerAction`: the spear in the main hand attacks along the view line.
fn stab(level: &mut ServerLevel, p: &mut ServerPlayer) {
    if p.is_spectator() {
        return;
    }
    let item = p.get_item_in_hand(InteractionHand::MainHand);
    if p.cannot_attack_with_item(&item, 5) {
        return;
    }
    if let Some(piercing) = item.get(&PIERCING_WEAPON) {
        piercing_weapon::attack(piercing, level, p, EquipmentSlot::MainHand);
    }
}

/// `handlePlayerCommand` of the actions the scenarios use (the packet handler itself needs a connection).
fn player_command(level: &mut ServerLevel, p: &mut ServerPlayer, action: &str) {
    match action.to_ascii_lowercase().as_str() {
        "start_sprinting" => p.set_sprinting(true),
        "stop_sprinting" => p.set_sprinting(false),
        "stop_sleeping" => {
            if p.is_sleeping() {
                p.stop_sleep_in_bed(level, false, true);
            }
        }
        // `if (!tryToStartFallFlying()) stopFallFlying()`
        "start_fall_flying" => {
            if !p.try_to_start_fall_flying() {
                mc_game::world::entity::living_entity::fall_flying::stop_fall_flying(p);
            }
        }
        other => panic!("pcommand {other} is not supported by the Rust replay"),
    }
}

/// `handleSetCarriedItem` for the replays whose players have no listener (the entity replay); the block replay delivers the packet.
fn select(level: &mut ServerLevel, p: &mut ServerPlayer, slot: i32) {
    assert!((0..9).contains(&slot), "hotbar slot {slot}");
    if p.player.inventory.get_selected_slot() != slot
        && p.get_used_item_hand() == InteractionHand::MainHand
    {
        p.stop_using_item(level);
    }
    p.player
        .inventory
        .set_selected_slot(slot)
        .expect("hotbar slot");
}

/// Run a player action command; `false` when `a[0]` is not one of them.
pub fn command<H: ActionHost>(h: &mut H, a: &[String]) -> bool {
    match a[0].as_str() {
        // give REF SLOT STACK [count]   (players only; the entity replay handles mobs itself)
        "give" => {
            h.boot();
            let stack = item_stack(&a[3], a.get(4).map_or(1, |c| c.parse().unwrap()));
            with_player(h, &a[1], |_, p| give(p, &a[2], stack));
        }
        "select" => {
            h.boot();
            with_player(h, &a[1], |level, p| {
                select(level, p, a[2].parse().expect("hotbar slot"))
            });
        }
        "use" => {
            h.boot();
            let sequence = h.next_sequence(&a[1]);
            with_player(h, &a[1], |level, p| use_item(level, p, a, sequence));
        }
        "use_on" => {
            h.boot();
            let sequence = h.next_sequence(&a[1]);
            with_player(h, &a[1], |level, p| use_on(level, p, a, sequence));
        }
        // RELEASE_USE_ITEM
        "release" => {
            h.boot();
            with_player(h, &a[1], |level, p| p.release_using_item(level));
        }
        "interact" => {
            h.boot();
            let target = h.action_ref(&a[2]);
            with_player(h, &a[1], |level, p| interact(level, p, target, a));
        }
        "swing" => {
            h.boot();
            with_player(h, &a[1], swing);
        }
        "attack" => {
            h.boot();
            let target = h.action_ref(&a[2]);
            with_player(h, &a[1], |level, p| attack(level, p, target));
        }
        // DROP_ITEM / DROP_ALL_ITEMS (a spectator cannot drop)
        "drop" => {
            h.boot();
            let all = a.get(2).is_some_and(|t| t == "all");
            with_player(h, &a[1], |level, p| {
                if !p.is_spectator() {
                    p.drop_selected(level, all);
                }
            });
        }
        // OPEN_INVENTORY needs the listener (a vehicle with its own inventory screen opens it)
        "pcommand" if a[2].eq_ignore_ascii_case("open_inventory") => {
            super::menu_actions::deliver(h, &a[1], |_, p| {
                mc_protocol::packets::play::ServerboundPlayPacket::PlayerCommand(
                    mc_protocol::packets::play::ServerboundPlayerCommandPacket {
                        id: p.base_id(),
                        action: mc_protocol::packets::play::ServerboundPlayerCommandAction::OpenInventory,
                        data: a.get(3).map_or(0, |d| d.parse().expect("pcommand data")),
                    },
                )
            });
        }
        "pcommand" => {
            h.boot();
            with_player(h, &a[1], |level, p| player_command(level, p, &a[2]));
        }
        "swap" => {
            h.boot();
            with_player(h, &a[1], swap);
        }
        "stab" => {
            h.boot();
            with_player(h, &a[1], stab);
        }
        // dispense X Y Z   BlockStateBase.tick of the block there: a dispenser or dropper dispenses once
        "dispense" => {
            h.boot();
            let pos = BlockPos::new(
                a[1].parse().unwrap(),
                a[2].parse().unwrap(),
                a[3].parse().unwrap(),
            );
            let level = h.action_level();
            level.get_block_state(pos).tick(level, pos);
        }
        _ => return super::menu_actions::command(h, a),
    }
    true
}

/// The player dump kinds: `use:REF`, `cooldowns:REF`, `stats:REF`, `inventory:REF`, `menu:REF`, `recipes:REF`; `false` for any other kind.
pub fn dump<H: ActionHost>(h: &mut H, kind: &str, arg: Option<&str>) -> bool {
    if matches!(kind, "menu" | "recipes") {
        return super::menu_actions::dump(h, kind, arg);
    }
    let (Some(name), "use" | "cooldowns" | "stats" | "inventory") = (arg, kind) else {
        return false;
    };
    let lines: Vec<String> = with_player(h, name, |_, p| match kind {
        // USE REF hand=main|off item=ID*COUNT remaining=N ticks=N kinetic=N
        "use" => vec![format!(
            "USE {name} hand={} item={} remaining={} ticks={} kinetic={}",
            if p.get_used_item_hand() == InteractionHand::MainHand {
                "main"
            } else {
                "off"
            },
            super::entity_scenario::item_text(p.get_use_item()),
            p.get_use_item_remaining_ticks(),
            p.get_ticks_using_item(),
            p.player
                .living
                .recent_kinetic_enemies
                .as_ref()
                .map_or(0, |m| m.len())
        )],
        // CD REF group=ID remaining=endTime-tickCount
        "cooldowns" => p
            .player
            .cooldowns
            .running()
            .into_iter()
            .map(|(group, remaining)| format!("CD {name} group={group} remaining={remaining}"))
            .collect(),
        // STAT REF type.name:value.name N, sorted by name
        "stats" => {
            let mut stats: Vec<(String, i32)> = p
                .stats
                .lock()
                .counter
                .iter_fast_order()
                .into_iter()
                .map(|(stat, n)| (mc_game::stats::stat::build_name(&stat), n))
                .collect();
            stats.sort();
            stats
                .into_iter()
                .map(|(stat, n)| format!("STAT {name} {stat} {n}"))
                .collect()
        }
        // I REF SLOT ITEM: `Inventory.getItem(i)`, the 36 slots, then the equipment slots (feet, legs, chest, head, offhand, body, saddle)
        _ => {
            let inv = &p.player.inventory;
            let main = inv.get_non_equipment_items().iter().enumerate();
            let worn = (36..=42).filter_map(|i| {
                mc_game::world::entity::player::inventory::equipment_slot_for(i)
                    .map(|es| (i, inv.equipment_get(es)))
            });
            main.chain(worn)
                .filter(|(_, s)| !s.is_empty())
                .map(|(i, s)| format!("I {name} {i} {}", super::entity_scenario::item_full(s)))
                .collect()
        }
    });
    for line in lines {
        h.emit(line);
    }
    true
}
