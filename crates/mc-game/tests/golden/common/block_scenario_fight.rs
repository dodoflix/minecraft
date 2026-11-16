//! The dragon fight verbs and dumps of the block replay (grammar and recorded twin: `tests/oracle/FightVerbs.java`): `end fight` (the End level
//! runs an `EnderDragonFight`), `fight_skip_arena`, `fight_try_respawn`, `fight_remove_gateways`, `dragon_killed DRAGON`, dumps `fight`, `fightbar`,
//! `fightfile`, `fighthash`, `fightpackets:PLAYER`.
//!
//! Not implemented: the fight itself (`EnderDragonFight`: scan, dragon creation, boss bar, respawn ritual, saved data). Everything that needs it
//! stops the replay with a message naming the missing piece: the level installer ([`install`]), the four verbs, and the dumps `fight`, `fightbar`
//! and `fightfile`. `fighthash` (blocks of the arena and of the gateway sites) and `fightpackets` do not depend on the fight and replay as recorded.
use super::{Scenario, state_string};
use mc_core::pos::BlockPos;
use mc_game::server::level::ServerLevel;
use mc_game::world::level::BlockGetter;

fn not_ported(what: &str) -> ! {
    panic!("Not implemented: the dragon fight ({what}): `EnderDragonFight` is not ported")
}

/// `end fight`: the End level carries the fight (`ServerLevel`'s constructor creates it for a dimension type with `has_ender_dragon_fight`).
pub(super) fn install(_level: &mut ServerLevel) {
    not_ported("the level's fight installer")
}

fn fnv(mut h: u64, s: &str) -> u64 {
    for b in s.bytes() {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

/// The block at `pos`; a chunk that is not loaded reads as air (the hash must not generate chunks).
fn block_text(level: &ServerLevel, x: i32, y: i32, z: i32) -> Option<String> {
    level.chunk_now(x >> 4, z >> 4)?;
    let state = level.get_block_state(BlockPos::new(x, y, z));
    (!state.is_air()).then(|| format!("{x},{y},{z},{}", state_string(state)))
}

/// `FIGHTHASH area N HASH`: FNV-1a over `x,y,z,state` of every non-air block of x,z -64..63, y 0..159.
fn area_hash(level: &ServerLevel) -> String {
    let (mut h, mut count) = (0xcbf2_9ce4_8422_2325_u64, 0_u64);
    for x in -64..64 {
        for z in -64..64 {
            for y in 0..160 {
                if let Some(text) = block_text(level, x, y, z) {
                    count += 1;
                    h = fnv(h, &text);
                }
            }
        }
    }
    format!("FIGHTHASH area {count} {h:x}")
}

/// `FIGHTHASH gateways N HASH`: the same over x-1..x+1, y 72..82, z-1..z+1 of the 20 gateway sites (`spawnNewGateway`).
fn gateway_hash(level: &ServerLevel) -> String {
    let (mut h, mut count) = (0xcbf2_9ce4_8422_2325_u64, 0_u64);
    for i in 0..20 {
        let angle = 2.0 * (-std::f64::consts::PI + (std::f64::consts::PI / 20.0) * f64::from(i));
        let gx = (96.0 * angle.cos()).floor() as i32;
        let gz = (96.0 * angle.sin()).floor() as i32;
        for x in gx - 1..=gx + 1 {
            for y in 72..=82 {
                for z in gz - 1..=gz + 1 {
                    if let Some(text) = block_text(level, x, y, z) {
                        count += 1;
                        h = fnv(h, &text);
                    }
                }
            }
        }
    }
    format!("FIGHTHASH gateways {count} {h:x}")
}

impl Scenario {
    /// The fight verbs; `false` for any other command.
    pub(super) fn fight_command(&mut self, a: &[String]) -> bool {
        match a[0].as_str() {
            "fight_skip_arena" => not_ported("skipArenaLoadedCheck"),
            "fight_try_respawn" => not_ported("tryRespawn"),
            "fight_remove_gateways" => not_ported("removeAllGateways"),
            "dragon_killed" => not_ported("setDragonKilled"),
            _ => false,
        }
    }

    /// The fight dumps; `false` for any other kind.
    pub(super) fn dump_fight(&mut self, kind: &str) -> bool {
        match kind {
            "fight" => not_ported("the `FIGHT` state line"),
            "fightbar" => not_ported("the boss bar"),
            "fightfile" => not_ported("the saved `ender_dragon_fight.dat`"),
            "fighthash" => {
                let level = self.level();
                let lines = [area_hash(level), gateway_hash(level)];
                self.out.extend(lines);
            }
            _ => return false,
        }
        true
    }
}
