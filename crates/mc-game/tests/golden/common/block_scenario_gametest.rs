//! The test block and test instance block verbs and dumps of the block replay (grammar and recorded twin: `tests/oracle/GameTestVerbs.java`):
//! `set_test_block PLAYER X Y Z MODE ["MESSAGE"]`, `test_instance_action PLAYER X Y Z ACTION [test=ID] [size=X,Y,Z] [rotation=NAME]
//! [ignore_entities=BOOL]`, dumps `structure_file:ID`, `gametest`, `logs:info`.
use super::Scenario;
use crate::common::entity_scenario::canon;
use crate::common::menu_actions::deliver;
use mc_core::ident::Identifier;
use mc_core::pos::BlockPos;
use mc_core::pos::vec3i::Vec3i;
use mc_core::rotation::Rotation;
use mc_game::server::MinecraftServer;
use mc_game::server::level::ServerLevel;
use mc_game::server::packs::resource_manager::MultiPackResourceManager;
use mc_game::server::packs::{PackType, ResourceManager};
use mc_game::world::level::levelgen::GenEnv;
use mc_game::world::level::levelgen::structure::templatesystem::StructureTemplateManager;
use mc_protocol::packets::play::{
    ServerboundPlayPacket, ServerboundSetTestBlockPacket, ServerboundTestInstanceBlockActionPacket,
    ServerboundTestInstanceBlockActionPacketAction as Action, TestBlockMode,
    TestInstanceBlockEntityData, TestInstanceBlockEntityStatus,
};
use std::sync::Arc;

fn pos(a: &[String]) -> BlockPos {
    let n = |i: usize| {
        a[i].parse()
            .unwrap_or_else(|_| panic!("not an integer: {}", a[i]))
    };
    BlockPos::new(n(2), n(3), n(4))
}

fn unquote(text: &str) -> String {
    text.strip_prefix('"')
        .and_then(|t| t.strip_suffix('"'))
        .unwrap_or(text)
        .to_owned()
}

/// `TestInstanceBlockEntity.getStructureSize`, `placeStructure` and `saveTest` find templates through the level's generation
/// environment: it reads the installed server's data packs and writes into its world's `generated` directory.
pub(super) fn with_server_templates(env: Arc<GenEnv>) -> Arc<GenEnv> {
    let templates = mc_game::commands::server_access::with_server(|server| {
        Arc::new(StructureTemplateManager::new(
            Arc::new(MultiPackResourceManager::new(
                PackType::ServerData,
                server.resources.resource_manager.list_packs(),
            )),
            server
                .storage_source
                .get_level_path(mc_world::level_storage::LevelResource::GENERATED_DIR),
        ))
    });
    let mut env =
        Arc::try_unwrap(env).unwrap_or_else(|_| panic!("the generation environment is shared"));
    env.structure_template_manager = templates;
    Arc::new(env)
}

impl Scenario {
    /// The test block verbs; `false` for any other command.
    pub(super) fn gametest_command(&mut self, a: &[String]) -> bool {
        let packet = match a[0].as_str() {
            "set_test_block" => {
                ServerboundPlayPacket::SetTestBlock(ServerboundSetTestBlockPacket {
                    position: pos(a),
                    mode: match a[5].as_str() {
                        "start" => TestBlockMode::Start,
                        "log" => TestBlockMode::Log,
                        "fail" => TestBlockMode::Fail,
                        "accept" => TestBlockMode::Accept,
                        other => panic!("test block mode {other}"),
                    },
                    message: a.get(6).map_or_else(String::new, |m| unquote(m)),
                })
            }
            "test_instance_action" => {
                let mut data = TestInstanceBlockEntityData {
                    test: None,
                    size: Vec3i::ZERO,
                    rotation: Rotation::None,
                    ignore_entities: false,
                    status: TestInstanceBlockEntityStatus::Cleared,
                    error_message: None,
                };
                for option in &a[6..] {
                    let (key, value) = option
                        .split_once('=')
                        .unwrap_or_else(|| panic!("option {option}"));
                    match key {
                        "test" => data.test = Some(Identifier::parse(value).expect("test id")),
                        "size" => {
                            let n: Vec<i32> =
                                value.split(',').map(|v| v.parse().expect("size")).collect();
                            data.size = Vec3i::new(n[0], n[1], n[2]);
                        }
                        "rotation" => {
                            data.rotation = Rotation::by_name(value).expect("rotation name");
                        }
                        "ignore_entities" => data.ignore_entities = value.parse().expect("bool"),
                        _ => panic!("option {option}"),
                    }
                }
                ServerboundPlayPacket::TestInstanceBlockAction(
                    ServerboundTestInstanceBlockActionPacket {
                        pos: pos(a),
                        action: match a[5].as_str() {
                            "init" => Action::Init,
                            "query" => Action::Query,
                            "set" => Action::Set,
                            "reset" => Action::Reset,
                            "save" => Action::Save,
                            "export" => Action::Export,
                            "run" => Action::Run,
                            other => panic!("test instance action {other}"),
                        },
                        data,
                    },
                )
            }
            _ => return false,
        };
        deliver(self, &a[1], |_, _| packet);
        true
    }

    /// The test dumps; `false` for any other kind.
    pub(super) fn dump_gametest(&mut self, kind: &str, arg: Option<&str>) -> bool {
        match (kind, arg) {
            ("structure_file", Some(id)) => {
                self.world();
                let id = Identifier::parse(id).expect("structure id");
                let file = mc_game::commands::server_access::with_server(|server| {
                    server
                        .storage_source
                        .get_level_path(mc_world::level_storage::LevelResource::GENERATED_DIR)
                        .join(id.namespace())
                        .join("structure")
                        .join(format!("{}.nbt", id.path()))
                });
                let line = match std::fs::read(&file) {
                    Err(_) => format!("STRUCTFILE {id} none"),
                    Ok(bytes) => {
                        let root = mc_nbt::compress::read_compressed(
                            &bytes,
                            &mut mc_nbt::NbtAccounter::unlimited_heap(),
                        )
                        .expect("the structure file");
                        format!("STRUCTFILE {id} {}", canon(&mc_nbt::Tag::Compound(root)))
                    }
                };
                self.out.push(line);
            }
            ("gametest", None) => {
                self.world();
                for info in mc_game::gametest::game_test_ticker::infos() {
                    self.out.push(format!(
                        "GAMETEST {} started={} done={} passed={} failed={} tick={}",
                        info.id(),
                        info.has_started(),
                        info.is_done(),
                        info.has_succeeded(),
                        info.has_failed(),
                        info.get_tick()
                    ));
                }
            }
            ("logs", Some("info")) => {
                self.world();
                // the log of the test block entity and of the runner, in order
                let lines: Vec<String> = super::super::take_logs()
                    .into_iter()
                    .filter_map(|l| l.strip_prefix("INFO: ").map(str::to_owned))
                    .filter(|l| l.starts_with("Test ") || l.starts_with("Running test environment"))
                    .map(|l| format!("LOGINFO {}", l.replace('\\', "\\\\").replace('\n', "\\n")))
                    .collect();
                self.out.extend(lines);
            }
            _ => return false,
        }
        true
    }

    /// `GameTestTicker.tick()` of `MinecraftServer.tickServer`, after the players.
    pub(super) fn tick_game_tests(&mut self) {
        if !mc_game::commands::server_access::has_server() {
            return;
        }
        let level: &mut ServerLevel = self.level.as_mut().expect("world");
        mc_game::commands::server_access::with_server(|server: &mut MinecraftServer| {
            if server.tick_rate_manager.runs_normally() {
                server.with_lent_level(level, |server| {
                    mc_game::gametest::game_test_ticker::tick(server)
                });
            }
        });
    }
}
