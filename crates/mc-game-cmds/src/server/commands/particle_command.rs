//! `ParticleCommand`.
use mc_brigadier::arguments::{FloatArgumentType, IntegerArgumentType, float_arg_min, integer_min};
use mc_brigadier::{
    ArgumentBuilder, CommandContext, CommandSyntaxException, SimpleCommandExceptionType,
};
use mc_core::phys::Vec3;
use mc_game_core::commands::arguments::coordinates::Vec3Argument;
use mc_game_core::commands::arguments::selector::EntityRef;
use mc_game_core::commands::arguments::{EntityArgument, ParticleArgument};
use mc_game_core::commands::message::simple_translatable;
use mc_game_core::commands::{
    CommandSourceStack, LEVEL_GAMEMASTERS, argument, has_permission, host, literal,
};
use mc_game_core::world::entity::ServerPlayer;
use mc_game_core::{register_command, translatable};
use mc_protocol::packets::play::{
    ClientboundLevelParticlesPacket, ParticleOptions, RandomizationType,
};
use std::sync::LazyLock;

static ERROR_FAILED: LazyLock<SimpleCommandExceptionType> =
    LazyLock::new(|| simple_translatable("commands.particle.failed"));

/// `c.getSource().getServer().getPlayerList().getPlayers()`.
fn all_players(c: &CommandContext<CommandSourceStack>) -> Vec<EntityRef> {
    c.get_source().with_server(|server| {
        server
            .player_list
            .get_players()
            .into_iter()
            .map(|p| EntityRef {
                dimension: p.dimension(),
                id: p.id,
            })
            .collect()
    })
}

register_command!("ParticleCommand", |ctx| {
    /// 0: `name`, 1: `name pos`, 2: `name pos delta speed count`.
    fn run(
        c: &CommandContext<CommandSourceStack>,
        shape: u8,
        force: bool,
        viewers: bool,
    ) -> Result<i32, CommandSyntaxException> {
        let (pos, delta, speed, count) = match shape {
            2 => (
                Vec3Argument::get_vec3(c, "pos"),
                Vec3Argument::get_vec3(c, "delta"),
                FloatArgumentType::get_float(c, "speed"),
                IntegerArgumentType::get_integer(c, "count"),
            ),
            1 => (Vec3Argument::get_vec3(c, "pos"), Vec3::ZERO, 0.0, 0),
            _ => (c.get_source().get_position(), Vec3::ZERO, 0.0, 0),
        };
        let players = if viewers {
            EntityArgument::get_players(c, "viewers")?
        } else {
            all_players(c)
        };
        send_particles(
            c.get_source(),
            ParticleArgument::get_particle(c, "name"),
            pos,
            delta,
            speed,
            count,
            force,
            &players,
        )
    }
    let viewers = |force: bool| {
        argument("viewers", EntityArgument::players()).executes(move |c| run(c, 2, force, true))
    };
    ctx.dispatcher.register(
        literal("particle")
            .requires(has_permission(LEVEL_GAMEMASTERS))
            .then(
                argument("name", ParticleArgument::particle())
                    .executes(|c| run(c, 0, false, false))
                    .then(
                        argument("pos", Vec3Argument::vec3())
                            .executes(|c| run(c, 1, false, false))
                            .then(
                                argument("delta", Vec3Argument::new(false)).then(
                                    argument("speed", float_arg_min(0.0)).then(
                                        argument("count", integer_min(0))
                                            .executes(|c| run(c, 2, false, false))
                                            .then(
                                                literal("force")
                                                    .executes(|c| run(c, 2, true, false))
                                                    .then(viewers(true)),
                                            )
                                            .then(
                                                literal("normal")
                                                    .executes(|c| run(c, 2, false, false))
                                                    .then(viewers(false)),
                                            ),
                                    ),
                                ),
                            ),
                    ),
            ),
    );
});

/// `sendParticles`: the players of the source's level close enough to `pos` get the particle packet.
#[allow(clippy::too_many_arguments)]
fn send_particles(
    source: &CommandSourceStack,
    particle: ParticleOptions,
    pos: Vec3,
    delta: Vec3,
    speed: f32,
    count: i32,
    force: bool,
    players: &[EntityRef],
) -> Result<i32, CommandSyntaxException> {
    let particle_name = particle.particle_type().name();
    let result = source.with_server(|server| {
        let mut result = 0;
        for player in players {
            // `ServerLevel.sendParticles(player, ...)`: the player must be in the source's level.
            if player.dimension != *source.get_level() {
                continue;
            }
            let Some(entity) = host::get_entity(server, player) else {
                continue;
            };
            let Some(target) = entity.as_any().downcast_ref::<ServerPlayer>() else {
                continue;
            };
            if !entity
                .base()
                .block_position()
                .closer_to_center_than(&pos, if force { 512.0 } else { 32.0 })
            {
                continue;
            }
            target.send(&ClientboundLevelParticlesPacket {
                particle: particle.clone(),
                override_limiter: force,
                always_show: false,
                x: pos.x,
                y: pos.y,
                z: pos.z,
                x_dist: delta.x as f32,
                y_dist: delta.y as f32,
                z_dist: delta.z as f32,
                x_max_speed: speed,
                y_max_speed: speed,
                z_max_speed: speed,
                count,
                randomization_type: RandomizationType::Default,
            });
            result += 1;
        }
        result
    });
    if result == 0 {
        return Err(ERROR_FAILED.create());
    }
    source.send_success(
        || translatable!("commands.particle.success", particle_name),
        true,
    );
    Ok(result)
}
