//! `SpreadPlayersCommand`.
use mc_brigadier::ArgumentBuilder;
use mc_game_core::commands::arguments::coordinates::Vec2Argument;
use mc_game_core::commands::arguments::selector::entity_selector::EntityRef;
use mc_game_core::commands::message::message;
use mc_game_core::commands::{CommandSourceStack, LEVEL_GAMEMASTERS, argument, has_permission, host, literal};
use mc_game_core::commands::arguments::EntityArgument;
use mc_game_core::server::level::ServerLevel;
use mc_game_core::util::java_format::format_fixed;
use mc_game_core::world::level::{BlockGetter, LevelReader};
use mc_game_core::{register_command, translatable};
use mc_brigadier::arguments::{BoolArgumentType, IntegerArgumentType, float_arg_min, integer};
use mc_brigadier::{CommandSyntaxException, Dynamic2CommandExceptionType, Dynamic4CommandExceptionType};
use mc_core::math::mth;
use mc_core::direction::Direction;
use mc_core::phys::Vec2;
use mc_core::pos::BlockPos;
use mc_core::random::{RandomSource, create_thread_local_instance_new};
use mc_protocol::packets::play::Relatives;
use std::collections::HashMap;
use std::sync::LazyLock;

const MAX_ITERATION_COUNT: i32 = 10000;

type SpreadFailure = Dynamic4CommandExceptionType<i32, f32, f32, String>;

fn failure(key: &'static str) -> SpreadFailure {
    Dynamic4CommandExceptionType::new(move |count: i32, x: f32, z: f32, recommended: String| message(translatable!(key, count, x, z, recommended)))
}

static ERROR_FAILED_TO_SPREAD_TEAMS: LazyLock<SpreadFailure> = LazyLock::new(|| failure("commands.spreadplayers.failed.teams"));
static ERROR_FAILED_TO_SPREAD_ENTITIES: LazyLock<SpreadFailure> = LazyLock::new(|| failure("commands.spreadplayers.failed.entities"));
static ERROR_INVALID_MAX_HEIGHT: LazyLock<Dynamic2CommandExceptionType<i32, i32>> = LazyLock::new(|| {
    Dynamic2CommandExceptionType::new(|supplied_max_height: i32, world_min_height: i32| {
        message(translatable!("commands.spreadplayers.failed.invalid.height", supplied_max_height, world_min_height))
    })
});

register_command!("SpreadPlayersCommand", |ctx| {
    let spread = |c: &mc_brigadier::CommandContext<CommandSourceStack>, max_height: Option<i32>| {
        let source = c.get_source();
        let max_height = max_height.unwrap_or_else(|| source.with_server(|server| server.level_mut(source.get_level()).map_or(0, |l| l.get_max_y())) + 1);
        spread_players(
            source,
            Vec2Argument::get_vec2(c, "center"),
            *c.get_argument::<f32>("spreadDistance"),
            *c.get_argument::<f32>("maxRange"),
            max_height,
            BoolArgumentType::get_bool(c, "respectTeams"),
            EntityArgument::get_entities(c, "targets")?,
        )
    };
    ctx.dispatcher.register(
        literal("spreadplayers").requires(has_permission(LEVEL_GAMEMASTERS)).then(
            argument("center", Vec2Argument::vec2()).then(
                argument("spreadDistance", float_arg_min(0.0)).then(
                    argument("maxRange", float_arg_min(1.0))
                        .then(
                            argument("respectTeams", BoolArgumentType::bool())
                                .then(argument("targets", EntityArgument::entities()).executes(move |c| spread(c, None))),
                        )
                        .then(
                            literal("under").then(
                                argument("maxHeight", integer()).then(
                                    argument("respectTeams", BoolArgumentType::bool()).then(
                                        argument("targets", EntityArgument::entities())
                                            .executes(move |c| spread(c, Some(IntegerArgumentType::get_integer(c, "maxHeight")))),
                                    ),
                                ),
                            ),
                        ),
                ),
            ),
        ),
    );
});

/// `SpreadPlayersCommand.Position`.
#[derive(Clone, Copy, Default)]
struct Position {
    x: f64,
    z: f64,
}

impl Position {
    fn dist(&self, target: &Position) -> f64 {
        let dx = self.x - target.x;
        let dz = self.z - target.z;
        (dx * dx + dz * dz).sqrt()
    }

    fn get_length(&self) -> f64 {
        (self.x * self.x + self.z * self.z).sqrt()
    }

    fn normalize(&mut self) {
        let dist = self.get_length();
        self.x /= dist;
        self.z /= dist;
    }

    fn move_away(&mut self, pos: &Position) {
        self.x -= pos.x;
        self.z -= pos.z;
    }

    fn clamp(&mut self, min_x: f64, min_z: f64, max_x: f64, max_z: f64) -> bool {
        let mut changed = false;
        if self.x < min_x {
            self.x = min_x;
            changed = true;
        } else if self.x > max_x {
            self.x = max_x;
            changed = true;
        }
        if self.z < min_z {
            self.z = min_z;
            changed = true;
        } else if self.z > max_z {
            self.z = max_z;
            changed = true;
        }
        changed
    }

    fn get_spawn_y(&self, level: &mut ServerLevel, max_height: i32) -> i32 {
        // `new BlockPos.MutableBlockPos(x, maxHeight + 1, z)`
        let mut pos = BlockPos::containing(self.x, f64::from(max_height + 1), self.z);
        // vanilla loads the chunk synchronously on the first read
        level.chunk_blocking_mut(pos.x() >> 4, pos.z() >> 4);
        let mut air_2_above = level.get_block_state(pos).is_air();
        pos = pos.relative(Direction::Down);
        let mut air_1_above = level.get_block_state(pos).is_air();
        while pos.y() > level.get_min_y() {
            pos = pos.relative(Direction::Down);
            let current_is_air = level.get_block_state(pos).is_air();
            if !current_is_air && air_1_above && air_2_above {
                return pos.y() + 1;
            }
            air_2_above = air_1_above;
            air_1_above = current_is_air;
        }
        max_height + 1
    }

    fn is_safe(&self, level: &mut ServerLevel, max_height: i32) -> bool {
        let spawn_y = self.get_spawn_y(level, max_height);
        let pos = BlockPos::containing(self.x, f64::from(spawn_y - 1), self.z);
        let state = level.get_block_state(pos);
        pos.y() < max_height && !state.liquid() && state.block().is_in_tag("entities_can_teleport_to")
    }

    fn randomize(&mut self, random: &mut dyn RandomSource, min_x: f64, min_z: f64, max_x: f64, max_z: f64) {
        self.x = mth::next_double(random, min_x, max_x);
        self.z = mth::next_double(random, min_z, max_z);
    }
}

/// The facts of one target `spreadPlayers` consults: `entity instanceof Player ? entity.getTeam() : null` (by team name).
fn team_key(source: &CommandSourceStack, entity: &EntityRef) -> Option<String> {
    source.with_server(|server| {
        let e = host::get_entity(server, entity)?;
        if e.is_player() { host::entity_team_name(server, e) } else { None }
    })
}

fn spread_players(
    source: &CommandSourceStack,
    center: Vec2,
    spread_distance: f32,
    max_range: f32,
    max_height: i32,
    respect_teams: bool,
    entities: Vec<EntityRef>,
) -> Result<i32, CommandSyntaxException> {
    let dimension = source.get_level().clone();
    let min_y = source.with_server(|server| server.level_mut(&dimension).map_or(0, |l| l.get_min_y()));
    if max_height < min_y {
        return Err(ERROR_INVALID_MAX_HEIGHT.create(max_height, min_y));
    }
    let mut random = create_thread_local_instance_new();
    let min_x = f64::from(center.x - max_range);
    let min_z = f64::from(center.y - max_range);
    let max_x = f64::from(center.x + max_range);
    let max_z = f64::from(center.y + max_range);
    let teams: Vec<Option<String>> = entities.iter().map(|e| team_key(source, e)).collect();
    let count = if respect_teams {
        let mut distinct: Vec<&Option<String>> = Vec::new();
        for t in &teams {
            if !distinct.contains(&t) {
                distinct.push(t);
            }
        }
        distinct.len()
    } else {
        entities.len()
    };
    let mut positions: Vec<Position> = (0..count)
        .map(|_| {
            let mut position = Position::default();
            position.randomize(&mut random, min_x, min_z, max_x, max_z);
            position
        })
        .collect();
    source.with_server(|server| {
        let level = server.level_mut(&dimension).expect("a command source runs in a loaded level");
        spread_positions(center, f64::from(spread_distance), level, &mut random, (min_x, min_z, max_x, max_z), max_height, &mut positions, respect_teams)
    })?;
    let distance = set_player_positions(source, &entities, &teams, &dimension, &positions, max_height, respect_teams);
    source.send_success(
        || {
            translatable!(
                if respect_teams { "commands.spreadplayers.success.teams" } else { "commands.spreadplayers.success.entities" },
                positions.len() as i32,
                center.x,
                center.y,
                format_fixed(distance, 2)
            )
        },
        true,
    );
    Ok(positions.len() as i32)
}

#[allow(clippy::too_many_arguments)]
fn spread_positions(
    center: Vec2,
    spread_dist: f64,
    level: &mut ServerLevel,
    random: &mut dyn RandomSource,
    (min_x, min_z, max_x, max_z): (f64, f64, f64, f64),
    max_height: i32,
    positions: &mut [Position],
    respect_teams: bool,
) -> Result<(), CommandSyntaxException> {
    let mut has_collisions = true;
    let mut min_distance = f64::from(f32::MAX);
    let mut iteration = 0;
    while iteration < MAX_ITERATION_COUNT && has_collisions {
        has_collisions = false;
        min_distance = f64::from(f32::MAX);
        for i in 0..positions.len() {
            let position = positions[i];
            let mut neighbour_count = 0;
            let mut average_neighbour_pos = Position::default();
            for (j, neighbour) in positions.iter().enumerate() {
                if i != j {
                    let dist = position.dist(neighbour);
                    min_distance = dist.min(min_distance);
                    if dist < spread_dist {
                        neighbour_count += 1;
                        average_neighbour_pos.x += neighbour.x - position.x;
                        average_neighbour_pos.z += neighbour.z - position.z;
                    }
                }
            }
            let position = &mut positions[i];
            if neighbour_count > 0 {
                average_neighbour_pos.x /= f64::from(neighbour_count);
                average_neighbour_pos.z /= f64::from(neighbour_count);
                let length = average_neighbour_pos.get_length();
                if length > 0.0 {
                    average_neighbour_pos.normalize();
                    position.move_away(&average_neighbour_pos);
                } else {
                    position.randomize(random, min_x, min_z, max_x, max_z);
                }
                has_collisions = true;
            }
            if position.clamp(min_x, min_z, max_x, max_z) {
                has_collisions = true;
            }
        }
        if !has_collisions {
            for position in positions.iter_mut() {
                if !position.is_safe(level, max_height) {
                    position.randomize(random, min_x, min_z, max_x, max_z);
                    has_collisions = true;
                }
            }
        }
        iteration += 1;
    }
    if min_distance == f64::from(f32::MAX) {
        min_distance = 0.0;
    }
    if iteration >= MAX_ITERATION_COUNT {
        let error = if respect_teams { &ERROR_FAILED_TO_SPREAD_TEAMS } else { &ERROR_FAILED_TO_SPREAD_ENTITIES };
        return Err(error.create(positions.len() as i32, center.x, center.y, format_fixed(min_distance, 2)));
    }
    Ok(())
}

fn set_player_positions(
    source: &CommandSourceStack,
    entities: &[EntityRef],
    teams: &[Option<String>],
    dimension: &mc_core::ident::Identifier,
    positions: &[Position],
    max_height: i32,
    respect_teams: bool,
) -> f64 {
    let mut avg_distance = 0.0;
    let mut position_index = 0;
    // `Map<Team, Position>` keyed by team (null for non-players and team-less players)
    let mut team_positions: HashMap<Option<String>, usize> = HashMap::new();
    for (entity, team) in entities.iter().zip(teams) {
        let index = if respect_teams {
            *team_positions.entry(team.clone()).or_insert_with(|| {
                position_index += 1;
                position_index - 1
            })
        } else {
            position_index += 1;
            position_index - 1
        };
        let position = positions[index];
        let (x, z) = (f64::from(mth::floor_f64(position.x)) + 0.5, f64::from(mth::floor_f64(position.z)) + 0.5);
        source.with_server(|server| {
            let y = server.level_mut(dimension).map_or(0, |level| position.get_spawn_y(level, max_height));
            let (y_rot, x_rot) = host::get_entity(server, entity).map_or((0.0, 0.0), |e| (e.base().y_rot(), e.base().x_rot()));
            host::teleport_entity(server, entity, dimension, mc_core::phys::Vec3::new(x, f64::from(y), z), Relatives::NONE, y_rot, x_rot);
        });
        let mut closest = f64::MAX;
        for (test_index, test_position) in positions.iter().enumerate() {
            if index != test_index {
                closest = position.dist(test_position).min(closest);
            }
        }
        avg_distance += closest;
    }
    if entities.len() < 2 { 0.0 } else { avg_distance / entities.len() as f64 }
}
