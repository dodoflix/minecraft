//! The command verbs of the scenario DSL, replayed the same way by the block replay (`block_scenario.rs`) and the entity replay
//! (`entity_scenario.rs`): `cmd`, `cmd_quiet`, `cmd_json`, `cmd_as WHO`, `cmd_as_json WHO`, `cmd_inform WHO` (and `op` for a replay whose players
//! have no player list entry). The grammar and the printed lines are those of `EntityOracleDriver.commandAs`:
//!
//! * `CF msg TEXT` per feedback message, `CF fail TEXT` for the red failure form of `sendFailure` (`\` and newlines escaped),
//! * `CF json JSON` after each message of `cmd_json`/`cmd_as_json` (canonical JSON: sorted keys),
//! * `CF result success|failure N` for every call of the source's result callback.
//!
//! WHO is a player name (that player's position, rotation, entity and permissions: `op` makes it level 4, otherwise level 0) or `-N`
//! (the console with permission level N). The command runs through the server `command_env` installs, so a scenario that uses
//! these verbs starts with `command_env`. The source of `cmd` and `cmd_as` never informs admins (`cmd_inform` does: the other operators get the
//! admin message and the console logs it, `dump console` prints `CON TEXT` per console message since the last dump); a player's source accepts
//! success feedback only while `send_command_feedback` is on, read on every message (the quiet oracle world turns it off: `gamerule send_command_feedback true`).
//! `find_spawn X Y Z TIMES` runs the spawn search of a first join TIMES times and prints what does not depend on its random order.
//!
//! The installed server holds the oracle world's quiet game rules ([`install_command_env`]) and the replayed level reads the server's own
//! state ([`attach_server_state`]: rule handle, scoreboard, command storage; the entity replay also takes the server's world data, so
//! difficulty, respawn position and game time are shared): `/gamerule`, `/difficulty`, `/scoreboard` and `/data` act on what the level
//! reads. A block replay with a command environment also ticks through the server (weather, tick rate manager, clocks, difficulty).
//!
//! The console source acts in the overworld like `MinecraftServer.createCommandSourceStack`: the replay of another dimension runs its
//! commands through `execute in`.
//!
//! Limits of the replay (a scenario that needs one of them adds it here): the entity replay's `cmd` without `command_env` prints no `CF` lines;
//! the entity replay keeps its own overworld clock and never ticks through the server, so `/time`, `/weather` and `/tick` change only the
//! server's state there.
use super::entity_scenario::canon;
use super::mob_verbs::MobHost;
use super::player_actions::ActionHost;
use mc_core::math::java::{java_double_to_string as jd, java_float_to_string as jf};
use mc_core::phys::{Vec2, Vec3};
use mc_core::pos::BlockPos;
use mc_game::commands::arguments::selector::{EntityRef, EntitySelectorParser};
use mc_game::commands::{CommandResultCallback, CommandSource, CommandSourceStack, server_access};
use mc_game::server::MinecraftServer;
use mc_game::server::level::ServerLevel;
use mc_game::server::level::level_rules::LiveRules;
use mc_game::server::network::{ConnectionSender, Outbound};
use mc_game::server::permissions::{LevelBasedPermissionSet, PermissionLevel};
use mc_game::server::players::{NameAndId, PlayerHandle};
use mc_game::world::clock::ClockInstance as _;
use mc_game::world::entity::Entity;
use mc_game::world::entity::EntityId;
use mc_game::world::level::DimensionKey;
use mc_game::world::level::gamerules::GameRules;
use mc_game::world::level::saveddata::world_border::BorderStatus;
use mc_nbt::{CompoundTag, Tag};
use mc_protocol::codec::GameProfile;
use mc_text::{Component, TextColor};
use parking_lot::Mutex;
use serde_json::Value;
use std::sync::Arc;
use tokio::sync::mpsc::UnboundedReceiver;

/// Who a `cmd` verb runs as.
enum Who {
    /// The console; `Some(level)` replaces the owner permissions by that permission level (`-N`).
    Console(Option<i32>),
    Player(String),
}

/// `EntityOracleDriver.commandAs`'s sink: records the messages (and the result callback calls) as `CF` lines.
struct Feedback {
    lines: Mutex<Vec<String>>,
    json: bool,
    /// The player's `acceptsSuccess`: `send_command_feedback`, read on every message; the console always accepts.
    player_feedback: Option<LiveRules>,
    /// `shouldInformAdmins` (`cmd_inform`): the other operators get the admin message and the console logs it.
    inform: bool,
}

impl CommandSource for Feedback {
    fn send_system_message(&self, message: &Component) {
        let failure = message.style().color() == Some(TextColor::RED);
        // the oracle writes its lines as UTF-8: a lone surrogate becomes `?`
        let text = message
            .get_java_string()
            .to_utf8_string()
            .replace('\\', "\\\\")
            .replace('\n', "\\n");
        let mut lines = self.lines.lock();
        lines.push(format!(
            "CF {} {text}",
            if failure { "fail" } else { "msg" }
        ));
        if self.json {
            let json = message.to_json().expect("feedback component encodes");
            lines.push(format!(
                "CF json {}",
                canonical_json(&mc_text::json::value_from_element(&json))
            ));
        }
    }
    fn accepts_success(&self) -> bool {
        self.player_feedback
            .as_ref()
            .is_none_or(|rules| rules.get().send_command_feedback)
    }
    fn accepts_failure(&self) -> bool {
        true
    }
    fn should_inform_admins(&self) -> bool {
        self.inform
    }
}

/// `EntityOracleDriver.canonJson`: the JSON text with the keys of every object sorted.
fn canonical_json(value: &Value) -> String {
    match value {
        Value::Object(map) => {
            let mut entries: Vec<_> = map.iter().collect();
            entries.sort_by(|a, b| a.0.cmp(b.0));
            let body: Vec<String> = entries
                .into_iter()
                .map(|(k, v)| format!("{}:{}", Value::String(k.clone()), canonical_json(v)))
                .collect();
            format!("{{{}}}", body.join(","))
        }
        Value::Array(items) => {
            let body: Vec<String> = items.iter().map(canonical_json).collect();
            format!("[{}]", body.join(","))
        }
        other => other.to_string(),
    }
}

/// The scenario verbs of this module; `line` is the whole (trimmed) scenario line, `a` its tokens. `false` for any other verb.
pub fn command<H: ActionHost>(h: &mut H, a: &[String], line: &str) -> bool {
    let rest = line[a[0].len()..].trim();
    match a[0].as_str() {
        "cmd" => run(h, Who::Console(None), rest, false, false, false),
        "cmd_quiet" => run(h, Who::Console(None), rest, false, true, false),
        "cmd_json" => run(h, Who::Console(None), rest, true, false, false),
        "cmd_as" | "cmd_as_json" | "cmd_inform" => {
            let who = match a[1].strip_prefix('-') {
                Some(level) => Who::Console(Some(level.parse().expect("cmd_as -LEVEL"))),
                None => Who::Player(a[1].clone()),
            };
            let command = rest[a[1].len()..].trim();
            run(
                h,
                who,
                command,
                a[0] == "cmd_as_json",
                false,
                a[0] == "cmd_inform",
            );
        }
        // `PlayerSpawnFinder.findSpawn` TIMES times: whether every result stays inside the `respawn_radius` square, whether any is off the
        // suggested column, and the height of the last (the candidate order is random, so only these are comparable)
        "find_spawn" => {
            h.boot();
            let [x, y, z, times] =
                [1, 2, 3, 4].map(|i| a[i].parse::<i32>().expect("find_spawn X Y Z TIMES"));
            let radius = server_access::with_server(|server| {
                server.game_rule(GameRules::RESPAWN_RADIUS).max(0)
            });
            let level = h.action_level();
            let (mut within, mut spread, mut last_y) = (true, false, 0.0);
            for _ in 0..times {
                let found = mc_game::server::players::respawn::find_spawn_position(
                    level,
                    false,
                    radius,
                    BlockPos::new(x, y, z),
                );
                let dist = ((found.x.floor() as i32) - x)
                    .abs()
                    .max(((found.z.floor() as i32) - z).abs());
                within &= dist <= radius;
                spread |= dist > 0;
                last_y = found.y;
            }
            h.emit(format!(
                "FINDSPAWN {x} {y} {z} times={times} within={within} spread={spread} y={last_y:?}"
            ));
        }
        // `PlayerList.op` for a replay whose players have no player list entry (the block replay sets the entry's permissions itself)
        "op" => {
            h.boot();
            let id = NameAndId::create_offline(&a[1]);
            server_access::with_server(|server| server.player_list.op_entry(id, None, None));
        }
        // bossbars_load [SNBT]   the custom boss bars through their saved codec and back, or replaced by the bars of SNBT
        "bossbars_load" => {
            h.boot();
            server_access::with_server(|server| {
                let ty = mc_game::server::bossevents::CustomBossEvents::type_();
                let saved = match rest.is_empty() {
                    true => (ty.encode)(server.custom_boss_events()).expect("the boss bars encode"),
                    false => Tag::Compound(
                        mc_nbt::snbt::parse_compound(rest).expect("bossbars_load SNBT"),
                    ),
                };
                // vanilla draws random bar ids: the loaded bars get fixed ones, in order of their custom ids
                let Tag::Compound(bars) = &saved else {
                    panic!("the boss bars encode as a compound")
                };
                let mut keys: Vec<String> = bars
                    .keys()
                    .map(|k| {
                        mc_core::ident::Identifier::parse(k.as_lossy_str())
                            .expect("bar id")
                            .to_string()
                    })
                    .collect();
                keys.sort();
                let loaded =
                    mc_game::server::bossevents::CustomBossEvents::decode_with(&saved, &mut |id| {
                        let n = keys
                            .iter()
                            .position(|k| **k == id.to_string())
                            .expect("bar id") as u128;
                        uuid::Uuid::from_u128(0xB055_B000_0000_0000_0000_0000_0000_0000 + n)
                    })
                    .unwrap_or_else(|e| panic!("{}", e.error));
                server
                    .saved_data
                    .set(&server.saved_types.custom_boss_events, loaded);
            });
        }
        // `pack_file PACK PATH TEXT...`: a file inside the world's data pack PACK (`\n` = line break); the server runs in the world's folder
        "pack_file" => {
            h.boot();
            assert!(
                server_access::has_server(),
                "`pack_file` needs the `command_env` verb"
            );
            let after_pack =
                a[0].len() + line[a[0].len()..].find(a[1].as_str()).expect("pack") + a[1].len();
            let start =
                after_pack + line[after_pack..].find(a[2].as_str()).expect("path") + a[2].len();
            let target = std::path::Path::new("world/datapacks")
                .join(&a[1])
                .join(&a[2]);
            std::fs::create_dir_all(target.parent().expect("pack path")).expect("pack folder");
            std::fs::write(target, line[start..].trim().replace("\\n", "\n")).expect("pack file");
        }
        "pack_remove" => {
            h.boot();
            assert!(
                server_access::has_server(),
                "`pack_remove` needs the `command_env` verb"
            );
            std::fs::remove_file(
                std::path::Path::new("world/datapacks")
                    .join(&a[1])
                    .join(&a[2]),
            )
            .expect("pack file");
        }
        _ => return false,
    }
    true
}

fn run<H: ActionHost>(h: &mut H, who: Who, command: &str, json: bool, quiet: bool, inform: bool) {
    h.boot();
    assert!(
        server_access::has_server(),
        "`cmd {command}` needs the `command_env` verb"
    );
    super::player_actions::remember_players(h);
    let player_feedback = matches!(who, Who::Player(_))
        .then(|| server_access::with_server(|server| server.game_rules().live()));
    let feedback = Arc::new(Feedback {
        lines: Mutex::default(),
        json,
        player_feedback,
        inform,
    });
    let source: Arc<dyn CommandSource> = feedback.clone();
    let stack = match who {
        Who::Player(name) => player_stack(h, &name, source),
        Who::Console(permission_level) => {
            let stack = console_stack(h.action_level(), source);
            match permission_level {
                Some(n) => stack.with_permission(
                    LevelBasedPermissionSet::for_level(PermissionLevel::by_id(n)).into(),
                ),
                None => stack,
            }
        }
    };
    let recorded = feedback.clone();
    let stack = stack.with_callback(CommandResultCallback::new(move |success, result| {
        recorded.lines.lock().push(format!(
            "CF result {} {result}",
            if success { "success" } else { "failure" }
        ));
    }));
    let level = h.action_level();
    server_access::with_server(|server| {
        let commands = server.commands.clone();
        server.with_lent_level(level, |server| {
            commands.perform_prefixed_command(server, stack, command)
        });
        server.flush_scoreboard();
    });
    let succeeded = feedback
        .lines
        .lock()
        .iter()
        .any(|l| l.starts_with("CF result success"));
    h.command_done(command, succeeded);
    if !quiet {
        for line in std::mem::take(&mut *feedback.lines.lock()) {
            h.emit(line);
        }
    }
}

/// `player.createCommandSourceStack()` with `source` as the feedback sink. A player the server removed (`leave`, a kick) is the
/// removed `ServerPlayer` the driver still holds: its last position, rotation and entity, and the permissions of the operator list.
fn player_stack<H: ActionHost>(
    h: &mut H,
    name: &str,
    source: Arc<dyn CommandSource>,
) -> CommandSourceStack {
    let id = h.action_ref(name);
    let level = h.action_level();
    let level_dimension = level.dimension.clone();
    let live = level.with_player(id, |_, p| {
        // `getProfilePermissions`: the player list entry's permissions when it has one, else the operator list
        let permissions = match &p.handle {
            Some(handle) => handle.shared.lock().permissions,
            None => profile_permissions(name),
        };
        (
            p.position(),
            Vec2::new(p.get_x_rot(), p.get_y_rot()),
            permissions,
            super::player_actions::command_entity(p),
            level_dimension,
        )
    });
    let (position, rotation, permissions, entity, dimension) = live
        .or_else(|| {
            let left = h.departed()?.get(name)?;
            Some((
                left.position,
                left.rotation,
                profile_permissions(name),
                left.entity.clone(),
                left.dimension.clone(),
            ))
        })
        .unwrap_or_else(|| panic!("`{name}` is not a player"));
    CommandSourceStack::for_entity(
        source,
        position,
        rotation,
        dimension,
        permissions.into(),
        entity,
    )
}

fn profile_permissions(name: &str) -> LevelBasedPermissionSet {
    server_access::with_server(|server| {
        server
            .player_list
            .get_profile_permissions(&NameAndId::create_offline(name))
    })
}

/// Every `[dd:dd:dd]` (the wall-clock stamp a command block puts before its last output) as `[HH:MM:SS]`, in text and inside a hex dump of
/// packet or synched data bytes, like the oracle driver prints it.
pub fn normalize_clock_stamps(line: &str) -> String {
    const STAMP: &[u8] = b"[dd:dd:dd]";
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let bytes = line.as_bytes();
    let text_stamp = |at: usize| {
        STAMP.iter().enumerate().all(|(i, &t)| {
            bytes.get(at + i).is_some_and(|&b| {
                if t == b'd' {
                    b.is_ascii_digit()
                } else {
                    b == t
                }
            })
        })
    };
    let hex_stamp = |at: usize| {
        STAMP.iter().enumerate().all(|(i, &t)| {
            let pair = (bytes.get(at + 2 * i), bytes.get(at + 2 * i + 1));
            match (pair, t) {
                ((Some(&hi), Some(&lo)), b'd') => hi == b'3' && lo.is_ascii_digit(),
                ((Some(&hi), Some(&lo)), t) => {
                    hi == HEX[usize::from(t >> 4)] && lo == HEX[usize::from(t & 15)]
                }
                _ => false,
            }
        })
    };
    let mut out = String::with_capacity(line.len());
    let mut i = 0;
    while i < line.len() {
        if text_stamp(i) {
            out.push_str("[HH:MM:SS]");
            i += 10;
        } else if hex_stamp(i) {
            out.push_str("5b48483a4d4d3a53535d");
            i += 20;
        } else {
            let c = line[i..].chars().next().expect("a char boundary");
            out.push(c);
            i += c.len_utf8();
        }
    }
    out
}

/// `MinecraftServer.createCommandSourceStack`: the overworld at the respawn position, no rotation, owner permissions, named `Server`.
fn console_stack(level: &ServerLevel, source: Arc<dyn CommandSource>) -> CommandSourceStack {
    let respawn = level.get_respawn_data().pos;
    CommandSourceStack::named(
        source,
        Vec3::new(
            f64::from(respawn.x()),
            f64::from(respawn.y()),
            f64::from(respawn.z()),
        ),
        Vec2::ZERO,
        mc_game::server::world_dimensions::overworld_key(),
        LevelBasedPermissionSet::OWNER.into(),
        Component::literal("Server"),
    )
}

// ------------------------------------------------------------------------------------------------ dumps

/// `EntityOracleDriver.canon`, with the elements of the list `sorted_key` of a compound sorted (a saved set is written in hash order).
fn canon_with_sorted(tag: &Tag, sorted_key: &str) -> String {
    let Tag::Compound(compound) = tag else {
        return canon(tag);
    };
    let mut keys: Vec<&mc_core::java_string::JavaString> = compound.keys().collect();
    keys.sort_by(|a, b| a.java_cmp(b));
    let parts: Vec<String> = keys
        .into_iter()
        .map(|key| {
            let name = key.as_lossy_str();
            let value = compound.get_java(key).expect("key exists");
            match value {
                Tag::List(list) if name == sorted_key => {
                    let mut items: Vec<String> = list.iter().map(canon).collect();
                    items.sort();
                    format!("{name}:[{}]", items.join(","))
                }
                other => format!("{name}:{}", canon(other)),
            }
        })
        .collect();
    format!("{{{}}}", parts.join(","))
}

/// One `PREFIX canonical-SNBT` line per element of `list` (compounds), sorted by line; the `Players` list of an element is sorted too.
fn sorted_lines(prefix: &str, list: Option<&Tag>) -> Vec<String> {
    let Some(Tag::List(list)) = list else {
        return Vec::new();
    };
    let mut lines: Vec<String> = list
        .iter()
        .map(|entry| format!("{prefix} {}", canon_with_sorted(entry, "Players")))
        .collect();
    lines.sort();
    lines
}

/// The dump kinds of this module (`EntityOracleDriver.dump`): `scoreboard` (`OBJ`/`SCORE`/`TEAM` entries of the scoreboard's saved form,
/// sorted, and `DISPLAY` slots), `bossbars` (`BOSSBAR ID SNBT` per custom boss bar), `saved:ID` (the data file the storage writes, see
/// [`saved_lines`]), `storage:ID` (`STORAGE ID SNBT`) and
/// `select:EXECUTOR/X,Y,Z/SELECTOR` (`SELECT SELECTOR EXECUTOR POS -> REF..`: what the selector finds for an operator in result order;
/// `selectsorted:` and `selectcount:` give the sorted names and the count), `world` (the level state lines: game time, difficulty, clocks, weather,
/// world spawn, border, tick rate manager), `forced` (`FORCED cx,cz ..`) and `logs` (`LOG LEVEL: message` per WARN or ERROR log line since the last
/// `logs`, sorted: a library logs its broken functions in a hash order; a newline of the message is written `\n`).
/// `false` for any other kind.
pub fn dump<H: MobHost>(h: &mut H, kind: &str, arg: Option<&str>) -> bool {
    let lines = match (kind, arg) {
        // `CON TEXT`: what the server console logged since the last dump (the capturing logger is installed by the scenario's first
        // `dump console`, so the scenario puts one before the command that logs)
        ("console", None) => {
            super::init_logger();
            super::take_logs()
                .into_iter()
                .filter_map(|l| l.strip_prefix("INFO: System chat: ").map(str::to_owned))
                // the oracle leaves the join and leave broadcasts (`multiplayer.player.*`) out of its console record
                .filter(|text| {
                    !text.ends_with(" joined the game") && !text.ends_with(" left the game")
                })
                .map(|text| format!("CON {}", text.replace('\\', "\\\\").replace('\n', "\\n")))
                .collect()
        }
        ("scoreboard", None) => {
            h.boot();
            let saved =
                server_access::with_server(|server| server.scoreboard.read().store_to_tag())
                    .expect("the scoreboard encodes");
            let Tag::Compound(saved) = saved else {
                panic!("the scoreboard encodes as a compound")
            };
            let mut lines = sorted_lines("OBJ", saved.get("Objectives"));
            lines.extend(sorted_lines("SCORE", saved.get("PlayerScores")));
            lines.push(format!(
                "DISPLAY {}",
                canon(
                    saved
                        .get("DisplaySlots")
                        .unwrap_or(&Tag::Compound(CompoundTag::new()))
                )
            ));
            lines.extend(sorted_lines("TEAM", saved.get("Teams")));
            lines
        }
        ("bossbars", None) => {
            h.boot();
            let saved = server_access::with_server(|server| {
                (mc_game::server::bossevents::CustomBossEvents::type_().encode)(
                    server.custom_boss_events(),
                )
            })
            .expect("the boss bars encode");
            let Tag::Compound(saved) = saved else {
                panic!("the boss bars encode as a compound")
            };
            let mut bars: Vec<String> = saved
                .iter()
                .map(|(id, packed)| {
                    format!(
                        "BOSSBAR {1} {}",
                        canon_with_sorted(packed, "Players"),
                        id.as_lossy_str()
                    )
                })
                .collect();
            bars.sort();
            bars
        }
        ("saved", Some(id)) => {
            h.boot();
            saved_lines(id)
        }
        ("storage", Some(id)) => {
            h.boot();
            let identifier = mc_core::ident::Identifier::parse(id).expect("storage id");
            let tag = server_access::with_server(|server| {
                mc_game::commands::host::command_storage_get(server, &identifier)
            });
            vec![format!("STORAGE {id} {}", canon(&Tag::Compound(tag)))]
        }
        ("world", None) => world_lines(h),
        ("forced", None) => {
            h.boot();
            let mut chunks: Vec<String> = h
                .action_level()
                .chunk_source()
                .get_force_loaded_chunks()
                .iter()
                .map(|c| format!("{},{}", c as i32, (c >> 32) as i32))
                .collect();
            chunks.sort();
            vec![format!("FORCED {}", chunks.join(" "))]
        }
        ("select" | "selectsorted" | "selectcount", Some(arg)) => {
            let mut parts = arg.splitn(3, '/');
            let (Some(executor), Some(position), Some(selector)) =
                (parts.next(), parts.next(), parts.next())
            else {
                panic!("select:EXECUTOR/X,Y,Z/SELECTOR")
            };
            vec![select_line(h, kind, executor, position, selector)]
        }
        ("logs", None) => {
            h.boot();
            let mut logs: Vec<String> = super::take_logs()
                .into_iter()
                .filter(|l| l.starts_with("WARN: ") || l.starts_with("ERROR: "))
                // the registries the replays load from the extracted vanilla data directory list its root marker file; vanilla reads a jar
                .filter(|l| {
                    !l.ends_with("/data/.mcassetsroot found in namespace directory, rejecting")
                })
                .collect();
            logs.sort();
            logs.iter()
                .map(|l| format!("LOG {}", l.replace('\\', "\\\\").replace('\n', "\\n")))
                .collect()
        }
        _ => return false,
    };
    for line in lines {
        h.emit(line);
    }
    true
}

/// `dump saved:ID`: `SAVED ID DataVersion=N` and a `SAVEDBAR ID SNBT` per entry (players sorted) of the data file the saved-data storage
/// writes for the boss bars (`SAVED ID none` when no file exists; only dirty data is written).
fn saved_lines(id: &str) -> Vec<String> {
    let identifier = mc_core::ident::Identifier::parse(id).expect("saved data id");
    let file = server_access::with_server(|server| {
        server.saved_data.save_and_join();
        server
            .saved_data
            .storage()
            .get_data_file(&identifier)
            .expect("data file path")
    });
    if !file.exists() {
        return vec![format!("SAVED {id} none")];
    }
    let root = mc_world::level_storage::SavedDataStorage::read_tag_from_disk(&file)
        .expect("the data file reads");
    let version = match root.get("DataVersion") {
        Some(Tag::Int(version)) => *version,
        _ => -1,
    };
    let mut lines = vec![format!("SAVED {id} DataVersion={version}")];
    if let Some(Tag::Compound(data)) = root.get("data") {
        let mut ids: Vec<&mc_core::java_string::JavaString> = data.keys().collect();
        ids.sort_by(|a, b| a.java_cmp(b));
        for key in ids {
            lines.push(format!(
                "SAVEDBAR {} {}",
                key.as_lossy_str(),
                canon_with_sorted(data.get_java(key).expect("key exists"), "Players")
            ));
        }
    }
    lines
}

/// `dump world`: the `WORLD`, `CLOCK`, `WEATHER`, `SPAWN`, `BORDER` and `TICKRATE` lines of `EntityOracleDriver.dump`.
fn world_lines<H: MobHost>(h: &mut H) -> Vec<String> {
    h.boot();
    let level = h.action_level();
    let game_time = level.game_time();
    let border = level.get_world_border();
    let status = match border.get_status() {
        BorderStatus::Growing => "GROWING",
        BorderStatus::Shrinking => "SHRINKING",
        BorderStatus::Stationary => "STATIONARY",
    };
    let border = format!(
        "BORDER center={},{} size={} target={} lerp={} safe={} damage={} warn_blocks={} warn_time={} status={status}",
        jd(border.get_center_x()),
        jd(border.get_center_z()),
        jd(border.get_size()),
        jd(border.get_lerp_target()),
        border.get_lerp_time(),
        jd(border.get_safe_zone()),
        jd(border.get_damage_per_block()),
        border.get_warning_blocks(),
        border.get_warning_time(),
    );
    server_access::with_server(|server| {
        let (difficulty, locked) = {
            let data = server.world_data.lock();
            let settings = data.settings.difficulty_settings;
            (settings.difficulty, settings.locked)
        };
        let mut lines = vec![format!(
            "WORLD gametime={game_time} difficulty={} locked={locked}",
            difficulty.get_serialized_name()
        )];
        let clocks = server.clock_manager();
        let mut ids: Vec<String> = clocks.clock_ids().map(ToString::to_string).collect();
        ids.sort();
        for id in ids {
            let instance = clocks.get_instance(&mc_core::ident::Identifier::parse(&id).unwrap());
            lines.push(format!(
                "CLOCK {id} total={} partial={} rate={} paused={}",
                instance.total_ticks(),
                jf(instance.partial_tick()),
                jf(instance.rate()),
                instance.is_paused()
            ));
        }
        let weather = server.weather_data_ref();
        lines.push(format!(
            "WEATHER raining={} thundering={} clear={} rain={} thunder={}",
            weather.is_raining(),
            weather.is_thundering(),
            weather.get_clear_weather_time(),
            weather.get_rain_time(),
            weather.get_thunder_time()
        ));
        let spawn = server.get_respawn_data();
        lines.push(format!(
            "SPAWN {} {},{},{} yaw={} pitch={}",
            spawn.dimension,
            spawn.pos.x(),
            spawn.pos.y(),
            spawn.pos.z(),
            jf(spawn.yaw),
            jf(spawn.pitch)
        ));
        lines.push(border);
        let rate = &server.tick_rate_manager;
        lines.push(format!(
            "TICKRATE rate={} frozen={} steps={} sprinting={}",
            jf(rate.tickrate()),
            rate.is_frozen(),
            rate.frozen_ticks_to_run(),
            rate.is_sprinting()
        ));
        lines
    })
}

/// `dump select:EXECUTOR/X,Y,Z/SELECTOR`; `selectsorted:` lists the names sorted and `selectcount:` only counts them (`sort=random` shuffles
/// with an unseeded generator in vanilla).
fn select_line<H: MobHost>(
    h: &mut H,
    kind: &str,
    executor: &str,
    position: &str,
    selector: &str,
) -> String {
    h.boot();
    let c: Vec<f64> = position
        .split(',')
        .map(|s| s.parse().expect("select position"))
        .collect();
    let position_vec = Vec3::new(c[0], c[1], c[2]);
    let executor_id = (executor != "-").then(|| h.action_ref(executor));
    let names = h.dump_names().0;
    let level = h.action_level();
    let dimension = level.dimension.clone();
    let mut stack = console_stack(level, mc_game::commands::null_source());
    let found = server_access::with_server(|server| {
        server.with_lent_level(level, |server| {
            if let Some(id) = executor_id {
                let entity = mc_game::commands::host::command_entity(
                    server,
                    &EntityRef {
                        dimension: dimension.clone(),
                        id,
                    },
                )
                .expect("the executor is in the level");
                stack = stack.with_entity(entity);
            }
            stack = stack.with_position(position_vec);
            let mut reader = mc_brigadier::StringReader::new(selector);
            let parsed = EntitySelectorParser::new(&mut reader, true).parse()?;
            parsed.find_entities(&stack, server)
        })
    });
    let mut line = format!(
        "{} {selector} {executor} {position} ->",
        kind.to_uppercase()
    );
    match found {
        Ok(found) if kind == "selectcount" => line.push_str(&format!(" {}", found.len())),
        Ok(found) => {
            let mut listed: Vec<&str> = found
                .iter()
                .map(|entity| names.get(&entity.id).map_or("?", String::as_str))
                .collect();
            if kind == "selectsorted" {
                listed.sort_unstable();
            }
            for name in listed {
                line.push(' ');
                line.push_str(name);
            }
        }
        Err(e) => {
            line.push_str(" !");
            line.push_str(&e.get_raw_message().get_string());
        }
    }
    line
}

// ------------------------------------------------------------------------------------------------ the command environment

/// The command environment of a replay (`command_env`): a real `MinecraftServer` (nothing started) with the parsers the dedicated server
/// installs, whose world seed is the oracle's default (`seed N` changes both through [`set_world_seed`]). `packs`: the data packs
/// of the scenario's `datapack` lines, `features` the feature packs of its `features` lines.
pub fn install_command_env(
    packs: &[String],
    features: &[String],
) -> (MinecraftServer, Box<dyn std::any::Any>) {
    let mut server = crate::command_common::test_server_with_packs(packs, features);
    mc_game::commands::BrigadierExceptions::install();
    mc_game::commands::install_text_validators();
    mc_protocol::components::install_text_hooks();
    server.0.world_gen_settings.options.seed = 4242;
    // the oracle world runs the quiet rule set from boot
    let enabled = server
        .0
        .world_data
        .lock()
        .settings
        .data_configuration
        .enabled_features;
    *server.0.game_rules() = super::block_scenario::quiet_game_rules(enabled);
    // the oracle server's player list runs at view and simulation distance 4 (`VanillaLevel.Options`)
    mc_game::server::players::player_ops::set_view_distance(&mut server.0, 4);
    mc_game::server::players::player_ops::set_simulation_distance(&mut server.0, 4);
    server
}

/// The replayed level reads the installed server's rule handle, scoreboard, command storage and clocks (what `/gamerule`, `/scoreboard`,
/// `/data` and `/time` change).
pub fn attach_server_state(level: &mut ServerLevel, server: &mut MinecraftServer) {
    level.server_state.rules = server.game_rules().live();
    level.server_state.effective_respawn = server.effective_respawn_data.clone();
    level.server_state.scoreboard = server.scoreboard.clone();
    let storage = server.command_storage.view();
    level.server_state.loot.command_storage = Arc::new(move |id| storage.get(id));
    let totals = server.clock_manager().totals();
    level.server_state.loot.clock_total_ticks =
        Arc::new(move |clock| Some(totals.total_ticks(clock)));
}

/// The installed server's world seed (`/seed`), when a command environment is installed.
pub fn set_world_seed(seed: i64) {
    if server_access::has_server() {
        server_access::with_server(|server| server.world_gen_settings.options.seed = seed);
    }
}

/// A player of a replay without a player list entry joins the installed server's player list and name cache
/// (`PlayerList.placeNewPlayer`), so that names and selectors reach it: its connection handle and the queue its packets go to.
pub fn join_command_env(
    profile: &GameProfile,
    id: EntityId,
    dimension: DimensionKey,
) -> (PlayerHandle, ConnectionSender, UnboundedReceiver<Outbound>) {
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
    let sender = ConnectionSender::new(tx);
    let handle = PlayerHandle::new(
        id,
        profile.clone(),
        sender.clone(),
        "<unknown>".to_owned(),
        dimension,
    );
    server_access::with_server(|server| {
        server
            .name_to_id_cache
            .add(NameAndId::new(profile.id, profile.name.clone()));
        server.player_list.add_player(handle.clone());
    });
    (handle, sender, rx)
}
