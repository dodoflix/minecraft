//! Menu verbs of the scenario DSL: `click`, `creative_slot`, `rename`, `select_trade`, `set_beacon`, `slot_state`, `place_recipe`,
//! `recipe_seen`, `recipe_settings` and the dump kinds `menu:REF`, `recipes:REF` (the grammar of the Java driver's `MenuVerbs.java`, which lists
//! every verb and option).
//!
//! Each verb builds the serverbound packet the vanilla verb builds and hands it to the player's listener through
//! [`PlayCtx::deliver`] (the registered play handlers), so the listener-level checks (stale container id, state id,
//! spectator resync, `stillValid`, hashed slot reconciliation) are part of what the replay compares. The handlers need the
//! server: a scenario that uses these verbs starts with `command_env`.
use super::entity_scenario::item_full;
use super::player_actions::{ActionHost, item_stack, with_player};
use mc_game::server::level::ServerLevel;
use mc_game::server::network::game_packets::PlayCtx;
use mc_game::world::entity::ServerPlayer;
use mc_game::world::entity::player::menu_ctx::PlayerCtx;
use mc_game::world::inventory::PlayerMenus;
use mc_game::world::item::ItemStack;
use mc_protocol::components::HashedStack;
use mc_protocol::components::hashed::component_hash;
use mc_protocol::packets::play::{
    ContainerInput, RecipeBookType, ServerboundContainerClickPacket,
    ServerboundContainerSlotStateChangedPacket, ServerboundPlaceRecipePacket,
    ServerboundPlayPacket, ServerboundRecipeBookChangeSettingsPacket,
    ServerboundRecipeBookSeenRecipePacket, ServerboundRenameItemPacket,
    ServerboundSelectTradePacket, ServerboundSetBeaconPacket, ServerboundSetCreativeModeSlotPacket,
};

/// `ITEM[components][*COUNT]`, `-` = empty.
fn stack(spec: &str) -> ItemStack {
    if spec == "-" {
        return ItemStack::empty();
    }
    match spec.rfind('*') {
        Some(star) if spec.rfind(']').is_none_or(|bracket| star > bracket) => item_stack(
            &spec[..star],
            spec[star + 1..].parse().expect("stack count"),
        ),
        _ => item_stack(spec, 1),
    }
}

/// Splits at the commas outside brackets, braces and quotes.
fn split(s: &str) -> Vec<&str> {
    let (mut parts, mut start, mut depth, mut quote) = (Vec::new(), 0, 0, false);
    for (i, c) in s.char_indices() {
        match c {
            '"' => quote = !quote,
            '[' | '{' if !quote => depth += 1,
            ']' | '}' if !quote => depth -= 1,
            ',' if depth == 0 && !quote => {
                parts.push(&s[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    parts.push(&s[start..]);
    parts
}

/// The driver's `kv(a, from)` value of `key` (`cid=3`).
fn option<'a>(a: &'a [String], from: usize, key: &str) -> Option<&'a str> {
    a.get(from..)?
        .iter()
        .find_map(|t| t.strip_prefix(key).and_then(|r| r.strip_prefix('=')))
}

fn recipe_id(name: &str) -> mc_core::ident::Identifier {
    mc_core::ident::Identifier::parse(&if name.contains(':') {
        name.to_owned()
    } else {
        format!("minecraft:{name}")
    })
    .unwrap_or_else(|e| panic!("recipe id {name}: {e:?}"))
}

/// The first display id of a recipe (`RecipeManager.listDisplaysForRecipe`).
fn display(level: &ServerLevel, recipe: &str) -> mc_protocol::packets::play::RecipeDisplayId {
    let mut ids = Vec::new();
    level
        .recipe_access()
        .list_displays_for_recipe(&recipe_id(recipe), |entry| ids.push(entry.id));
    *ids.first()
        .unwrap_or_else(|| panic!("recipe {recipe} has no display"))
}

fn effect(name: &str) -> Option<mc_data::MobEffect> {
    (name != "-")
        .then(|| mc_data::MobEffect::by_name(name).unwrap_or_else(|| panic!("effect {name}")))
}

fn click(p: &ServerPlayer, a: &[String]) -> ServerboundPlayPacket {
    let base = p.player.menus.current().base();
    let (mut cid, mut state) = (base.container_id, base.get_state_id());
    let mut carried = HashedStack::create(base.get_carried(), &component_hash);
    let mut changed = Vec::new();
    for t in &a[5..] {
        let (key, value) = t
            .split_once('=')
            .unwrap_or_else(|| panic!("click option {t}"));
        match key {
            "cid" => cid = value.parse().expect("cid"),
            "state" if value == "stale" => state = (base.get_state_id() - 1) & 32767,
            "state" => state = value.parse().expect("state"),
            "changed" => {
                for entry in split(value) {
                    let (slot, spec) = entry.split_once(':').expect("changed=SLOT:STACK");
                    let slot: i16 = slot.parse().expect("changed slot");
                    // a map in the packet: the last entry of a slot wins
                    changed.retain(|(s, _)| *s != slot);
                    changed.push((slot, HashedStack::create(&stack(spec), &component_hash)));
                }
            }
            "carried" => carried = HashedStack::create(&stack(value), &component_hash),
            _ => panic!("click option {t}"),
        }
    }
    ServerboundPlayPacket::ContainerClick(ServerboundContainerClickPacket {
        container_id: cid,
        state_id: state,
        slot_num: a[2].parse().expect("slot"),
        button_num: a[3].parse().expect("button"),
        container_input: match a[4].as_str() {
            "PICKUP" => ContainerInput::Pickup,
            "QUICK_MOVE" => ContainerInput::QuickMove,
            "SWAP" => ContainerInput::Swap,
            "CLONE" => ContainerInput::Clone,
            "THROW" => ContainerInput::Throw,
            "QUICK_CRAFT" => ContainerInput::QuickCraft,
            "PICKUP_ALL" => ContainerInput::PickupAll,
            other => panic!("container input {other}"),
        },
        changed_slots: changed,
        carried_item: carried,
    })
}

/// `connection.handleXxx(packet)` of the packet `build` makes from the player.
pub fn deliver<H: ActionHost>(
    h: &mut H,
    name: &str,
    build: impl FnOnce(&mut ServerLevel, &mut ServerPlayer) -> ServerboundPlayPacket,
) {
    h.boot();
    with_player(h, name, |level, player| {
        let packet = build(level, player);
        mc_game::commands::server_access::with_server(|server| {
            PlayCtx {
                server,
                level,
                player,
            }
            .deliver(packet)
        });
    });
}

/// Run a menu verb; `false` when `a[0]` is not one of them.
pub fn command<H: ActionHost>(h: &mut H, a: &[String]) -> bool {
    use ServerboundPlayPacket as P;
    match a[0].as_str() {
        "click" => deliver(h, &a[1], |_, p| click(p, a)),
        "creative_slot" => deliver(h, &a[1], |_, _| {
            let mut stack = stack(&a[3]);
            if let Some(count) = a.get(4).filter(|_| !stack.is_empty()) {
                stack.set_count(count.parse().expect("count"));
            }
            P::SetCreativeModeSlot(ServerboundSetCreativeModeSlotPacket {
                slot_num: a[2].parse().expect("slot"),
                item_stack: stack,
            })
        }),
        "rename" => deliver(h, &a[1], |_, _| {
            let text = &a[2];
            P::RenameItem(ServerboundRenameItemPacket {
                name: text
                    .strip_prefix('"')
                    .and_then(|t| t.strip_suffix('"'))
                    .unwrap_or(text)
                    .to_owned(),
            })
        }),
        "select_trade" | "trade_select" => deliver(h, &a[1], |_, _| {
            P::SelectTrade(ServerboundSelectTradePacket {
                item: a[2].parse().expect("trade index"),
            })
        }),
        "set_beacon" => deliver(h, &a[1], |_, _| {
            P::SetBeacon(ServerboundSetBeaconPacket {
                primary: effect(&a[2]),
                secondary: effect(&a[3]),
            })
        }),
        "slot_state" => deliver(h, &a[1], |_, p| {
            P::ContainerSlotStateChanged(ServerboundContainerSlotStateChangedPacket {
                slot_id: a[2].parse().expect("slot"),
                container_id: option(a, 4, "cid")
                    .map_or(p.player.menus.current_container_id(), |c| {
                        c.parse().expect("cid")
                    }),
                new_state: a[3].parse().expect("true|false"),
            })
        }),
        "place_recipe" => deliver(h, &a[1], |level, p| {
            P::PlaceRecipe(ServerboundPlaceRecipePacket {
                container_id: p.player.menus.current_container_id(),
                recipe: display(level, &a[2]),
                use_max_items: a.get(3).is_some_and(|t| t == "all"),
            })
        }),
        "recipe_seen" => deliver(h, &a[1], |level, _| {
            P::RecipeBookSeenRecipe(ServerboundRecipeBookSeenRecipePacket {
                recipe: display(level, &a[2]),
            })
        }),
        "recipe_settings" => deliver(h, &a[1], |_, _| {
            P::RecipeBookChangeSettings(ServerboundRecipeBookChangeSettingsPacket {
                book_type: match a[2].as_str() {
                    "crafting" => RecipeBookType::Crafting,
                    "furnace" => RecipeBookType::Furnace,
                    "blast_furnace" => RecipeBookType::BlastFurnace,
                    "smoker" => RecipeBookType::Smoker,
                    other => panic!("recipe book type {other}"),
                },
                is_open: a[3].parse().expect("open"),
                is_filtering: a[4].parse().expect("filtering"),
            })
        }),
        _ => return false,
    }
    true
}

/// `RECIPES REF crafting=OF furnace=OF blast_furnace=OF smoker=OF known=ID,.. highlight=ID,..` (O = open, F = filtering as 0/1).
fn recipes_line(name: &str, p: &ServerPlayer) -> String {
    let book = &p.recipe_book;
    let mut line = format!("RECIPES {name}");
    for (label, ty) in [
        ("crafting", RecipeBookType::Crafting),
        ("furnace", RecipeBookType::Furnace),
        ("blast_furnace", RecipeBookType::BlastFurnace),
        ("smoker", RecipeBookType::Smoker),
    ] {
        line += &format!(
            " {label}={}{}",
            u8::from(book.is_open(ty)),
            u8::from(book.is_filtering(ty))
        );
    }
    for (label, ids) in [
        ("known", book.known().collect::<Vec<_>>()),
        ("highlight", book.highlighted().collect()),
    ] {
        let mut ids: Vec<String> = ids.into_iter().map(ToString::to_string).collect();
        ids.sort();
        line += &format!(
            " {label}={}",
            if ids.is_empty() {
                "-".into()
            } else {
                ids.join(",")
            }
        );
    }
    line
}

/// The dump kinds `menu:REF` and `recipes:REF`; `false` for any other kind.
pub fn dump<H: ActionHost>(h: &mut H, kind: &str, arg: Option<&str>) -> bool {
    let (Some(name), "menu" | "recipes") = (arg, kind) else {
        return false;
    };
    if kind == "recipes" {
        let line = with_player(h, name, |_, p| recipes_line(name, p));
        h.emit(line);
        return true;
    }
    let lines: Vec<String> = with_player(h, name, |level, player| {
        let in_inventory = !player.player.menus.has_container_open();
        let mut ctx = PlayerCtx { player, level };
        PlayerMenus::with_current(
            &mut ctx,
            |c| &mut c.player.player.menus,
            |menu, c| {
                let base = menu.base_mut();
                let type_name = if in_inventory {
                    "inventory".to_owned()
                } else {
                    base.get_type().name().to_owned()
                };
                // MENU REF cid=N type=KEY|inventory state=N carried=STACK
                let mut lines = vec![format!(
                    "MENU {name} cid={} type={type_name} state={} carried={}",
                    base.container_id,
                    base.get_state_id(),
                    item_full(base.get_carried())
                )];
                // MS I STACK per non-empty slot
                for i in 0..base.slots.len() {
                    let stack = base.with_slot(c, i, |slot, env| slot.get_item(env));
                    if !stack.is_empty() {
                        lines.push(format!("MS {i} {}", item_full(&stack)));
                    }
                }
                // MD I VALUE per data slot (block entity values are read live like `DataSlot.get()`)
                base.refresh_data_slots(c);
                for (i, v) in base.data_slot_values().into_iter().enumerate() {
                    lines.push(format!("MD {i} {v}"));
                }
                lines
            },
        )
    });
    for line in lines {
        h.emit(line);
    }
    true
}
