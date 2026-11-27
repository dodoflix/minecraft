//! The dragon fight verbs and dumps of the block replay (grammar and recorded twin: `tests/oracle/FightVerbs.java`): `end fight` (the End level
//! runs an `EnderDragonFight`), `fight_skip_arena`, `fight_try_respawn`, `fight_remove_gateways`, `dragon_killed DRAGON`, dumps `fight`, `fightbar`,
//! `fightfile`, `fighthash`, `fightpackets:PLAYER[:type,..]`.
use super::{Scenario, state_string};
use mc_core::pos::BlockPos;
use mc_game::server::level::ServerLevel;
use mc_game::world::level::BlockGetter;
use mc_game::world::level::dimension::end::{self, EnderDragonFight};
use mc_game::world::level::saveddata::encode_codec;
use mc_world::level_storage::SavedDataStorage;

/// `end fight`: the End level carries the fight (`ServerLevel`'s constructor creates it for a dimension type with `has_ender_dragon_fight`);
/// the fight places features, so the level needs a generation environment.
pub(super) fn install(level: &mut ServerLevel, seed: i64) {
    if level.gen_env.is_none() {
        level.gen_env = Some(super::super::feature_harness::gen_env(seed));
    }
    level.install_dragon_fight();
}

fn fight(level: &mut ServerLevel) -> &mut EnderDragonFight {
    level
        .dragon_fight_mut()
        .expect("no dragon fight in this level (`end fight`)")
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
        let gx = (96.0 * mc_core::math::java::cos(angle)).floor() as i32;
        let gz = (96.0 * mc_core::math::java::sin(angle)).floor() as i32;
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
            "fight_skip_arena" => {
                self.world();
                fight(self.level()).skip_arena_loaded_check();
            }
            "fight_try_respawn" => {
                self.world();
                end::try_respawn(self.level());
            }
            "fight_remove_gateways" => {
                self.world();
                fight(self.level()).remove_all_gateways();
            }
            "dragon_killed" => {
                self.world();
                let id = self.entity_id(&a[1]);
                let level = self.level();
                let uuid = level
                    .get_entity(id)
                    .unwrap_or_else(|| panic!("entity {} is gone", a[1]))
                    .uuid();
                end::set_dragon_killed(level, uuid);
            }
            _ => return false,
        }
        true
    }

    /// `FIGHT killed= prev= stage= rtime= alive= dragon= portal= gateways= scan= seen= pscan= cscan= crystals=`.
    fn fight_state(&mut self) -> String {
        let uuid = fight(self.level()).dragon_uuid();
        // the scenario name of the entity (removed ones too) with the fight's dragon uuid, `gone` for a uuid no entity has
        let dragon = match uuid {
            None => "-".to_string(),
            Some(uuid) => {
                let refs = self.actors.refs.clone();
                let level = self.level();
                refs.iter()
                    .find(|(_, id)| {
                        level
                            .get_entity_or_removed(*id)
                            .is_some_and(|e| e.uuid() == uuid)
                    })
                    .map_or("gone".to_string(), |(name, _)| name.clone())
            }
        };
        let f = fight(self.level());
        let portal = f.exit_portal_location().map_or("-".to_string(), |p| {
            format!("{},{},{}", p.x(), p.y(), p.z())
        });
        let stage = f
            .respawn_stage()
            .map_or("-".to_string(), |s| s.get_serialized_name().to_uppercase());
        let gateways: Vec<String> = f.gateways().iter().map(ToString::to_string).collect();
        format!(
            "FIGHT killed={} prev={} stage={stage} rtime={} alive={} dragon={dragon} portal={portal} gateways={} scan={} seen={} pscan={} cscan={} crystals={}",
            f.is_dragon_killed(),
            f.has_previously_killed_dragon(),
            f.respawn_time(),
            f.alive_crystals(),
            gateways.join(","),
            f.needs_state_scanning(),
            f.ticks_since_dragon_seen(),
            f.ticks_since_last_player_scan(),
            f.ticks_since_crystals_scanned(),
            f.respawn_crystal_count(),
        )
    }

    /// `FIGHTBAR visible= progress= players= name= color= overlay= music= fog=`.
    fn fight_bar(&mut self) -> String {
        let names: Vec<(String, mc_game::world::entity::EntityId)> = self
            .actors
            .players
            .clone()
            .into_iter()
            .map(|n| {
                let id = self.entity_id(&n);
                (n, id)
            })
            .collect();
        let bar = fight(self.level()).dragon_event();
        let mut players: Vec<String> = bar
            .get_players()
            .map(|id| {
                names
                    .iter()
                    .find(|(_, p)| *p == id)
                    .map_or_else(|| format!("#{id}"), |(n, _)| n.clone())
            })
            .collect();
        players.sort();
        format!(
            "FIGHTBAR visible={} progress={} players={} name={} color={} overlay={} music={} fog={}",
            bar.is_visible(),
            mc_core::math::java::java_float_to_string(bar.get_progress()),
            if players.is_empty() {
                "-".to_string()
            } else {
                players.join(",")
            },
            bar.get_name().get_string(),
            bar.get_color().get_name(),
            bar.get_overlay().get_name(),
            bar.should_play_boss_music(),
            bar.should_create_world_fog(),
        )
    }

    /// `FIGHTFILE HEX`: the bytes of the `ender_dragon_fight.dat` the level would save (uncompressed NBT).
    fn fight_file(&mut self) -> String {
        let data = encode_codec(&EnderDragonFight::codec(), fight(self.level()))
            .expect("the fight encodes");
        let bytes = mc_nbt::nbt_io::to_bytes(&SavedDataStorage::encode_file(data));
        format!(
            "FIGHTFILE {}",
            bytes.iter().map(|b| format!("{b:02x}")).collect::<String>()
        )
    }

    /// The fight dumps; `false` for any other kind.
    pub(super) fn dump_fight(&mut self, kind: &str) -> bool {
        match kind {
            "fight" => {
                self.world();
                let line = self.fight_state();
                self.out.push(line);
            }
            "fightbar" => {
                self.world();
                let line = self.fight_bar();
                self.out.push(line);
            }
            "fightfile" => {
                self.world();
                let line = self.fight_file();
                self.out.push(line);
            }
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
