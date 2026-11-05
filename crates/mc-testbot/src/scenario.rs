//! Text scenarios for the bot, the same file run against vanilla and against our server (`mc-testbot run`). One command per line, `#`
//! comments, tokens split on whitespace (the rest of the line for `cmd`/`chat`). The observation lines the scenario prints
//! (`observe`) go to the returned list; the packet log (`--log`) is diffed with `mc-testbot diff` as usual.
//!
//! ```text
//! cmd COMMAND...                 /command as the bot (the server must have op'd it: vanilla `op Bot`, ours the same)
//! chat TEXT...                   unsigned chat message
//! wait_ticks N                   N client ticks            wait SECONDS   real time
//! wait_chat TEXT [SECONDS=10]    wait for a chat/system line containing TEXT
//! look_at X Y Z                  walk X Z [sprint]         jump [TICKS=1]           sneak on|off      select N (hotbar 0-8)
//! wait_entity TYPE [SECONDS=10]  wait for an entity of TYPE that did not exist before (the `add_entity` packet); TYPE = `zombie` or `minecraft:zombie`
//! attack REF                     ServerboundAttackPacket   interact REF [main|off]  ServerboundInteractPacket (location 0,0.5,0 relative to the entity)
//! use_item_on X Y Z FACE [main|off]    ServerboundUseItemOnPacket (FACE: down up north south west east, cursor = face centre)
//! use [main|off]                 ServerboundUseItemPacket  trade_select N         ServerboundSelectTradePacket
//! observe [LABEL]                prints `PLAYER ...`, one `ENT type#n pos= vel= rot= head= og= vehicle= data=` per tracked entity, and the
//!                                trading offers (`OFFERS`/`OFFER`) of the last merchant_offers packet
//! note TEXT...                   prints `# TEXT`
//! ```
//! `REF` = `TYPE#N`: the N-th (1-based, ascending entity id) tracked entity of that type, e.g. `zombie#1` or `minecraft:villager#2`.
use std::time::Duration;

use mc_core::direction::Direction;
use mc_core::phys::Vec3;
use mc_core::pos::block_pos::BlockPos;
use mc_data::EntityType;
use mc_protocol::packets::play::InteractionHand;

use crate::bot::Bot;
use crate::error::{BotError, Result};

fn fail(line_no: usize, line: &str, msg: impl std::fmt::Display) -> BotError {
    BotError::Protocol(format!("scenario line {line_no} `{line}`: {msg}"))
}

fn entity_type(name: &str) -> Option<EntityType> {
    EntityType::by_name(&if name.contains(':') { name.to_string() } else { format!("minecraft:{name}") })
}

fn hand(t: Option<&&str>) -> InteractionHand {
    if t == Some(&"off") { InteractionHand::OffHand } else { InteractionHand::MainHand }
}

fn face(name: &str) -> Option<Direction> {
    Some(match name {
        "down" => Direction::Down,
        "up" => Direction::Up,
        "north" => Direction::North,
        "south" => Direction::South,
        "west" => Direction::West,
        "east" => Direction::East,
        _ => return None,
    })
}

/// Resolve `TYPE#N` to a tracked entity id.
fn resolve(bot: &Bot, reference: &str) -> Option<i32> {
    let (ty, n) = reference.split_once('#')?;
    let ids = bot.entities_of(entity_type(ty)?);
    ids.get(n.parse::<usize>().ok()?.checked_sub(1)?).copied()
}

/// Run a scenario; returns the observation lines.
pub async fn run(bot: &Bot, text: &str) -> Result<Vec<String>> {
    let mut out = Vec::new();
    for (i, raw) in text.lines().enumerate() {
        let (no, line) = (i + 1, raw.trim());
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        bot.assert_connected()?;
        let t: Vec<&str> = line.split_whitespace().collect();
        let rest = line.split_once(char::is_whitespace).map_or("", |(_, r)| r.trim());
        let num = |s: &str| s.parse::<f64>().map_err(|_| fail(no, line, format!("not a number: {s}")));
        let arg = |k: usize| t.get(k).copied().ok_or_else(|| fail(no, line, "missing argument"));
        match t[0] {
            "cmd" => bot.command(rest.trim_start_matches('/')),
            "chat" => bot.chat(rest),
            "wait_ticks" => bot.wait_ticks(num(arg(1)?)? as u64).await?,
            "wait" => tokio::time::sleep(Duration::from_secs_f64(num(arg(1)?)?)).await,
            "wait_chat" => {
                let secs = t.get(2).map_or(Ok(10.0), |s| num(s))?;
                let needle = arg(1)?;
                bot.wait_chat(needle, 0, Duration::from_secs_f64(secs)).await?;
            }
            "look_at" => bot.look_at(Vec3::new(num(arg(1)?)?, num(arg(2)?)?, num(arg(3)?)?)),
            "walk" => bot.walk_to(num(arg(1)?)?, num(arg(2)?)?, 0.6, t.get(3) == Some(&"sprint"), Duration::from_secs(30)).await?,
            "jump" => bot.jump(t.get(1).map_or(Ok(1.0), |s| num(s))? as u32),
            "sneak" => bot.set_sneaking(arg(1)? == "on"),
            "select" => bot.select_hotbar(num(arg(1)?)? as u8),
            "wait_entity" => {
                let ty = entity_type(arg(1)?).ok_or_else(|| fail(no, line, "unknown entity type"))?;
                let secs = t.get(2).map_or(Ok(10.0), |s| num(s))?;
                let known = bot.entities_of(ty);
                bot.wait_entity(ty, &known, Duration::from_secs_f64(secs)).await?;
            }
            "attack" | "interact" => {
                let r = arg(1)?;
                let id = resolve(bot, r).ok_or_else(|| fail(no, line, format!("no entity {r}")))?;
                if t[0] == "attack" {
                    bot.attack(id);
                } else {
                    bot.interact(id, hand(t.get(2)), Vec3::new(0.0, 0.5, 0.0));
                }
            }
            "use_item_on" => {
                let pos = BlockPos::new(num(arg(1)?)? as i32, num(arg(2)?)? as i32, num(arg(3)?)? as i32);
                let f = face(arg(4)?).ok_or_else(|| fail(no, line, "face"))?;
                let cursor = (0.5 + 0.5 * f.get_step_x() as f32, 0.5 + 0.5 * f.get_step_y() as f32, 0.5 + 0.5 * f.get_step_z() as f32);
                bot.use_item_on(hand(t.get(5)), pos, f, cursor, false);
            }
            "use" => bot.use_item(hand(t.get(1))),
            "trade_select" => bot.select_trade(num(arg(1)?)? as i32),
            "observe" => {
                out.push(format!("# observe {}", t.get(1).copied().unwrap_or("")));
                let p = bot.position();
                out.push(format!("PLAYER pos={:?},{:?},{:?} health={:?} food={}", p.x, p.y, p.z, bot.health(), bot.food()));
                out.extend(bot.entity_lines());
                out.extend(bot.merchant_offer_lines());
            }
            "note" => out.push(format!("# {rest}")),
            other => return Err(fail(no, line, format!("unknown scenario command `{other}`"))),
        }
    }
    Ok(out)
}
