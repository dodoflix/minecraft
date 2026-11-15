//! The command verbs of the scenario DSL, replayed the same way by the block replay (`block_scenario.rs`) and the entity replay
//! (`entity_scenario.rs`): `cmd`, `cmd_quiet`, `cmd_json`, `cmd_as WHO`, `cmd_as_json WHO` (and `op` for a replay whose players
//! have no player list entry). The grammar and the printed lines are those of `EntityOracleDriver.commandAs`:
//!
//! * `CF msg TEXT` per feedback message, `CF fail TEXT` for the red failure form of `sendFailure` (`\` and newlines escaped),
//! * `CF json JSON` after each message of `cmd_json`/`cmd_as_json` (canonical JSON: sorted keys),
//! * `CF result success|failure N` for every call of the source's result callback.
//!
//! WHO is a player name (that player's position, rotation, entity and permissions: `op` makes it level 4, otherwise level 0) or `-N`
//! (the console with permission level N). The command runs through the server `command_env` installs, so a scenario that uses
//! these verbs starts with `command_env`. Like the driver's sink, the source never informs admins; a player's source accepts
//! success feedback only while `send_command_feedback` is on (the quiet oracle world turns it off: `gamerule send_command_feedback true`).
//!
//! Limits of the replay (a scenario that needs one of them adds it here): the installed server keeps vanilla's default game rules (the
//! oracle world runs the quiet set), so `gamerule` commands read different old values; a player's `send_command_feedback` is read when the
//! command starts (vanilla reads it on every message); the entity replay shares no difficulty, scoreboard or respawn position with the
//! installed server; the emulated `cmd weather|recipe` arms and the entity replay's `cmd` without `command_env` print no `CF` lines;
//! the console source always acts in the replay's dimension.
use super::entity_scenario::canon;
use super::mob_verbs::MobHost;
use super::player_actions::ActionHost;
use mc_core::phys::{Vec2, Vec3};
use mc_game::commands::arguments::selector::{EntityRef, EntitySelectorParser};
use mc_game::commands::{
    CommandEntity, CommandResultCallback, CommandSource, CommandSourceStack, server_access,
};
use mc_game::server::MinecraftServer;
use mc_game::server::level::ServerLevel;
use mc_game::server::network::{ConnectionSender, Outbound};
use mc_game::server::permissions::{LevelBasedPermissionSet, PermissionLevel};
use mc_game::server::players::name_resolver::UserNameToIdResolver as _;
use mc_game::server::players::{NameAndId, PlayerHandle};
use mc_game::world::entity::Entity;
use mc_game::world::entity::EntityId;
use mc_game::world::level::DimensionKey;
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
    /// The player's `acceptsSuccess` (`send_command_feedback`, read when the command starts); the console always accepts.
    player_feedback: Option<bool>,
}

impl CommandSource for Feedback {
    fn send_system_message(&self, message: &Component) {
        let failure = message.style().color() == Some(TextColor::RED);
        let text = message
            .get_string()
            .replace('\\', "\\\\")
            .replace('\n', "\\n");
        let mut lines = self.lines.lock();
        lines.push(format!(
            "CF {} {text}",
            if failure { "fail" } else { "msg" }
        ));
        if self.json {
            let json = message.to_json().expect("feedback component encodes");
            lines.push(format!("CF json {}", canonical_json(&json)));
        }
    }
    fn accepts_success(&self) -> bool {
        self.player_feedback.unwrap_or(true)
    }
    fn accepts_failure(&self) -> bool {
        true
    }
    fn should_inform_admins(&self) -> bool {
        false
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
        "cmd" => run(h, Who::Console(None), rest, false, false),
        "cmd_quiet" => run(h, Who::Console(None), rest, false, true),
        "cmd_json" => run(h, Who::Console(None), rest, true, false),
        "cmd_as" | "cmd_as_json" => {
            let who = match a[1].strip_prefix('-') {
                Some(level) => Who::Console(Some(level.parse().expect("cmd_as -LEVEL"))),
                None => Who::Player(a[1].clone()),
            };
            let command = rest[a[1].len()..].trim();
            run(h, who, command, a[0] == "cmd_as_json", false);
        }
        // `PlayerList.op` for a replay whose players have no player list entry (the block replay sets the entry's permissions itself)
        "op" => {
            h.boot();
            let id = NameAndId::create_offline(&a[1]);
            server_access::with_server(|server| server.player_list.op_entry(id, None, None));
        }
        _ => return false,
    }
    true
}

fn run<H: ActionHost>(h: &mut H, who: Who, command: &str, json: bool, quiet: bool) {
    h.boot();
    assert!(
        server_access::has_server(),
        "`cmd {command}` needs the `command_env` verb"
    );
    let player_feedback = matches!(who, Who::Player(_)).then(|| h.send_command_feedback());
    let feedback = Arc::new(Feedback {
        lines: Mutex::default(),
        json,
        player_feedback,
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

/// `player.createCommandSourceStack()` with `source` as the feedback sink.
fn player_stack<H: ActionHost>(
    h: &mut H,
    name: &str,
    source: Arc<dyn CommandSource>,
) -> CommandSourceStack {
    let id = h.action_ref(name);
    let level = h.action_level();
    let dimension = level.dimension.clone();
    level
        .with_player(id, |_, p| {
            let entity = server_access::with_server(|server| CommandEntity {
                id: p.id(),
                uuid: p.uuid(),
                is_player: true,
                display_name: mc_game::commands::host::entity_display_name(server, &*p),
                plain_text_name: p.get_plain_text_name(),
                eye_height: p.player.living.entity.eye_height,
                player: p.handle.clone(),
            });
            // `getProfilePermissions`: the player list entry's permissions when it has one, else the operator list
            let permissions = match &p.handle {
                Some(handle) => handle.shared.lock().permissions,
                None => server_access::with_server(|server| {
                    server
                        .player_list
                        .get_profile_permissions(&NameAndId::create_offline(name))
                }),
            };
            CommandSourceStack::for_entity(
                source,
                p.position(),
                Vec2::new(p.get_x_rot(), p.get_y_rot()),
                dimension,
                permissions.into(),
                entity,
            )
        })
        .unwrap_or_else(|| panic!("`{name}` is not a player"))
}

/// A fixture recorded before feedback lines existed has no `CF` line: the replay of its scenario prints the console feedback the way the
/// driver printed it then (`C TEXT` per message, nothing else), which the comparison ignores. Fixtures that have any `CF` line, and
/// scenarios without a fixture, keep the `CF` lines.
pub fn legacy_feedback(name: &str, lines: Vec<String>) -> Vec<String> {
    if !lines.iter().any(|l| l.starts_with("CF ")) {
        return lines;
    }
    let dir = super::entity_oracle::fixture_dir();
    let path = dir.join(format!("{name}.txt"));
    let gz = path.with_extension("txt.gz");
    let text = if gz.exists() {
        mc_testkit::read_gz(gz)
    } else if let Ok(text) = std::fs::read_to_string(&path) {
        text
    } else {
        return lines;
    };
    if text.lines().any(|l| l.starts_with("CF ")) {
        return lines;
    }
    lines
        .into_iter()
        .filter_map(|line| match line.strip_prefix("CF ") {
            Some(rest) => rest
                .strip_prefix("msg ")
                .or_else(|| rest.strip_prefix("fail "))
                .map(|text| format!("C {}", unescape(text))),
            None => Some(line),
        })
        .collect()
}

/// `MinecraftServer.createCommandSourceStack`: the respawn position, no rotation, owner permissions, named `Server`.
fn console_stack(level: &ServerLevel, source: Arc<dyn CommandSource>) -> CommandSourceStack {
    let respawn = level.level_data.get_respawn_data().pos;
    CommandSourceStack::named(
        source,
        Vec3::new(
            f64::from(respawn.x()),
            f64::from(respawn.y()),
            f64::from(respawn.z()),
        ),
        Vec2::ZERO,
        level.dimension.clone(),
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
    let mut keys: Vec<&String> = compound.keys().collect();
    keys.sort();
    let parts: Vec<String> = keys
        .into_iter()
        .map(|key| {
            let value = compound.get(key).expect("key exists");
            match value {
                Tag::List(list) if key == sorted_key => {
                    let mut items: Vec<String> = list.iter().map(canon).collect();
                    items.sort();
                    format!("{key}:[{}]", items.join(","))
                }
                other => format!("{key}:{}", canon(other)),
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
/// sorted, and `DISPLAY` slots), `bossbars` (`BOSSBAR ID SNBT` per custom boss bar), `storage:ID` (`STORAGE ID SNBT`) and
/// `select:EXECUTOR/X,Y,Z/SELECTOR` (`SELECT SELECTOR EXECUTOR POS -> REF..`: what the selector finds for an operator in result order).
/// `false` for any other kind.
pub fn dump<H: MobHost>(h: &mut H, kind: &str, arg: Option<&str>) -> bool {
    let lines = match (kind, arg) {
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
                    format!("BOSSBAR {id} {}", canon_with_sorted(packed, "Players"))
                })
                .collect();
            bars.sort();
            bars
        }
        ("storage", Some(id)) => {
            h.boot();
            let identifier = mc_core::ident::Identifier::parse(id).expect("storage id");
            let tag = server_access::with_server(|server| {
                mc_game::commands::host::command_storage_get(server, &identifier)
            });
            vec![format!("STORAGE {id} {}", canon(&Tag::Compound(tag)))]
        }
        ("select", Some(arg)) => {
            let mut parts = arg.splitn(3, '/');
            let (Some(executor), Some(position), Some(selector)) =
                (parts.next(), parts.next(), parts.next())
            else {
                panic!("select:EXECUTOR/X,Y,Z/SELECTOR")
            };
            vec![select_line(h, executor, position, selector)]
        }
        _ => return false,
    };
    for line in lines {
        h.emit(line);
    }
    true
}

/// `dump select:EXECUTOR/X,Y,Z/SELECTOR`.
fn select_line<H: MobHost>(h: &mut H, executor: &str, position: &str, selector: &str) -> String {
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
    let mut line = format!("SELECT {selector} {executor} {position} ->");
    match found {
        Ok(found) => {
            for entity in found {
                line.push(' ');
                line.push_str(names.get(&entity.id).map_or("?", String::as_str));
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
/// of the scenario's `datapack` lines.
pub fn install_command_env(packs: &[String]) -> (MinecraftServer, Box<dyn std::any::Any>) {
    let mut server = crate::command_common::test_server_with_packs(packs);
    mc_game::commands::BrigadierExceptions::install();
    mc_game::commands::install_text_validators();
    server.0.world_gen_settings.options.seed = 4242;
    server
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

/// The inverse of the `CF` text escaping (`\\` and `\n`), in one left-to-right pass.
fn unescape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        match (c, chars.clone().next()) {
            ('\\', Some('n')) => {
                out.push('\n');
                chars.next();
            }
            ('\\', Some('\\')) => {
                out.push('\\');
                chars.next();
            }
            _ => out.push(c),
        }
    }
    out
}
