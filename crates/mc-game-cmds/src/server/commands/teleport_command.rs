//! `TeleportCommand` (`teleport`, `tp`) with `LookAt`.
use super::look_at::LookAt;
use mc_brigadier::ArgumentBuilder;
use mc_brigadier::{CommandContext, CommandSyntaxException, SimpleCommandExceptionType};
use mc_core::math::mth;
use mc_core::phys::Vec3;
use mc_core::pos::BlockPos;
use mc_game_core::commands::arguments::EntityArgument;
use mc_game_core::commands::arguments::coordinates::block_pos_argument::is_in_spawnable_bounds;
use mc_game_core::commands::arguments::coordinates::{Coordinates, RotationArgument, Vec3Argument};
use mc_game_core::commands::arguments::entity_anchor_argument::Anchor;
use mc_game_core::commands::arguments::entity_anchor_argument::EntityAnchorArgument;
use mc_game_core::commands::arguments::selector::EntityRef;
use mc_game_core::commands::message::simple_translatable;
use mc_game_core::commands::{
    CommandSourceStack, LEVEL_GAMEMASTERS, argument, has_permission, host, literal,
};
use mc_game_core::world::level::DimensionKey;
use mc_game_core::{register_command, translatable};
use mc_protocol::packets::play::{Relative, Relatives};
use mc_text::Component;
use std::sync::LazyLock;

static INVALID_POSITION: LazyLock<SimpleCommandExceptionType> =
    LazyLock::new(|| simple_translatable("commands.teleport.invalidPosition"));

register_command!("TeleportCommand", |ctx| {
    fn here(
        c: &CommandContext<CommandSourceStack>,
    ) -> Result<Vec<EntityRef>, CommandSyntaxException> {
        Ok(vec![host::source_entity_ref(c.get_source())?])
    }
    fn to_pos(
        c: &CommandContext<CommandSourceStack>,
        rotation: bool,
        look_at: Option<LookAt>,
    ) -> Result<i32, CommandSyntaxException> {
        let rot = if rotation {
            Some(RotationArgument::get_rotation(c, "rotation"))
        } else {
            None
        };
        teleport_to_pos(
            c.get_source(),
            EntityArgument::get_entities(c, "targets")?,
            c.get_source().get_level().clone(),
            Vec3Argument::get_coordinates(c, "location"),
            rot,
            look_at,
        )
    }
    let teleport = ctx.dispatcher.register(
        literal("teleport")
            .requires(has_permission(LEVEL_GAMEMASTERS))
            .then(argument("location", Vec3Argument::vec3()).executes(|c| {
                teleport_to_pos(
                    c.get_source(),
                    here(c)?,
                    c.get_source().get_level().clone(),
                    Vec3Argument::get_coordinates(c, "location"),
                    None,
                    None,
                )
            }))
            .then(
                argument("destination", EntityArgument::entity()).executes(|c| {
                    teleport_to_entity(
                        c.get_source(),
                        here(c)?,
                        EntityArgument::get_entity(c, "destination")?,
                    )
                }),
            )
            .then(
                argument("targets", EntityArgument::entities())
                    .then(
                        argument("location", Vec3Argument::vec3())
                            .executes(|c| to_pos(c, false, None))
                            .then(
                                argument("rotation", RotationArgument::rotation())
                                    .executes(|c| to_pos(c, true, None)),
                            )
                            .then(
                                literal("facing")
                                    .then(
                                        literal("entity").then(
                                            argument("facingEntity", EntityArgument::entity())
                                                .executes(|c| {
                                                    to_pos(
                                                        c,
                                                        false,
                                                        Some(LookAt::Entity(
                                                            EntityArgument::get_entity(
                                                                c,
                                                                "facingEntity",
                                                            )?,
                                                            Anchor::Feet,
                                                        )),
                                                    )
                                                })
                                                .then(
                                                    argument(
                                                        "facingAnchor",
                                                        EntityAnchorArgument::anchor(),
                                                    )
                                                    .executes(|c| {
                                                        to_pos(
                                                            c,
                                                            false,
                                                            Some(LookAt::Entity(
                                                                EntityArgument::get_entity(
                                                                    c,
                                                                    "facingEntity",
                                                                )?,
                                                                EntityAnchorArgument::get_anchor(
                                                                    c,
                                                                    "facingAnchor",
                                                                ),
                                                            )),
                                                        )
                                                    }),
                                                ),
                                        ),
                                    )
                                    .then(
                                        argument("facingLocation", Vec3Argument::vec3()).executes(
                                            |c| {
                                                to_pos(
                                                    c,
                                                    false,
                                                    Some(LookAt::Position(Vec3Argument::get_vec3(
                                                        c,
                                                        "facingLocation",
                                                    ))),
                                                )
                                            },
                                        ),
                                    ),
                            ),
                    )
                    .then(
                        argument("destination", EntityArgument::entity()).executes(|c| {
                            teleport_to_entity(
                                c.get_source(),
                                EntityArgument::get_entities(c, "targets")?,
                                EntityArgument::get_entity(c, "destination")?,
                            )
                        }),
                    ),
            ),
    );
    ctx.dispatcher.register(
        literal("tp")
            .requires(has_permission(LEVEL_GAMEMASTERS))
            .redirect(teleport),
    );
});

/// `CommandResponseTracker` over `entities` (every element has value 1): `(count, only element)`.
fn track(entities: &[EntityRef]) -> (i32, Option<&EntityRef>) {
    (
        entities.len() as i32,
        if entities.len() == 1 {
            entities.first()
        } else {
            None
        },
    )
}

fn display_name(source: &CommandSourceStack, entity: &EntityRef) -> Component {
    source.with_server(|server| {
        host::get_entity(server, entity)
            .map_or_else(Component::empty, |e| host::entity_display_name(server, e))
    })
}

fn teleport_to_entity(
    source: &CommandSourceStack,
    entities: Vec<EntityRef>,
    destination: EntityRef,
) -> Result<i32, CommandSyntaxException> {
    let Some((pos, y_rot, x_rot)) = source.with_server(|server| {
        host::get_entity(server, &destination)
            .map(|d| (d.base().position(), d.base().y_rot(), d.base().x_rot()))
    }) else {
        return Ok(0);
    };
    let mut tracked = Vec::new();
    for entity in entities {
        perform_teleport(
            source,
            &entity,
            &destination.dimension,
            pos.x,
            pos.y,
            pos.z,
            Relatives::NONE,
            y_rot,
            x_rot,
            None,
        )?;
        tracked.push(entity);
    }
    let (count, only) = track(&tracked);
    let destination_name = display_name(source, &destination);
    let message = match only {
        Some(entity) => translatable!(
            "commands.teleport.success.entity.single",
            display_name(source, entity),
            destination_name
        ),
        None => translatable!(
            "commands.teleport.success.entity.multiple",
            count,
            destination_name
        ),
    };
    source.send_success(|| message, true);
    Ok(count)
}

fn teleport_to_pos(
    source: &CommandSourceStack,
    entities: Vec<EntityRef>,
    level: DimensionKey,
    destination: Coordinates,
    rotation: Option<Coordinates>,
    look_at: Option<LookAt>,
) -> Result<i32, CommandSyntaxException> {
    let pos = destination.get_position(source);
    let rot = rotation.as_ref().map(|r| r.get_rotation(source));
    let mut tracked = Vec::new();
    for entity in entities {
        let relatives = get_relatives(&destination, rotation.as_ref(), entity.dimension == level);
        let (current_y, current_x) = source.with_server(|server| {
            host::get_entity(server, &entity)
                .map_or((0.0, 0.0), |e| (e.base().y_rot(), e.base().x_rot()))
        });
        match rot {
            None => perform_teleport(
                source,
                &entity,
                &level,
                pos.x,
                pos.y,
                pos.z,
                relatives,
                current_y,
                current_x,
                look_at.as_ref(),
            )?,
            Some(rot) => perform_teleport(
                source,
                &entity,
                &level,
                pos.x,
                pos.y,
                pos.z,
                relatives,
                rot.y,
                rot.x,
                look_at.as_ref(),
            )?,
        }
        tracked.push(entity);
    }
    let (count, only) = track(&tracked);
    let (x, y, z) = (
        format_double(pos.x),
        format_double(pos.y),
        format_double(pos.z),
    );
    let message = match only {
        Some(entity) => translatable!(
            "commands.teleport.success.location.single",
            display_name(source, entity),
            x,
            y,
            z
        ),
        None => translatable!(
            "commands.teleport.success.location.multiple",
            count,
            x,
            y,
            z
        ),
    };
    source.send_success(|| message, true);
    Ok(count)
}

fn get_relatives(
    destination: &Coordinates,
    rotation: Option<&Coordinates>,
    same_dimension: bool,
) -> Relatives {
    let (x, y, z) = (
        destination.is_x_relative(),
        destination.is_y_relative(),
        destination.is_z_relative(),
    );
    let mut set = Relatives::NONE;
    for (relative, on) in [
        (Relative::DeltaX, x),
        (Relative::DeltaY, y),
        (Relative::DeltaZ, z),
    ] {
        if on {
            set.insert(relative);
        }
    }
    if same_dimension {
        for (relative, on) in [(Relative::X, x), (Relative::Y, y), (Relative::Z, z)] {
            if on {
                set.insert(relative);
            }
        }
    }
    match rotation {
        None => set.union(Relatives::ROTATION),
        Some(rotation) => {
            if rotation.is_y_relative() {
                set.insert(Relative::YRot);
            }
            if rotation.is_x_relative() {
                set.insert(Relative::XRot);
            }
            set
        }
    }
}

/// `String.format(Locale.ROOT, "%f", value)`: six decimals, HALF_UP on the shortest decimal digits of the double.
fn format_double(value: f64) -> String {
    if value.is_nan() {
        return "NaN".to_owned();
    }
    if value.is_infinite() {
        return if value < 0.0 { "-Infinity" } else { "Infinity" }.to_owned();
    }
    let negative = value.is_sign_negative();
    let sci = format!("{:e}", value.abs());
    let (mantissa, exponent) = sci.split_once('e').unwrap_or((&sci, "0"));
    let exponent: i32 = exponent.parse().unwrap_or(0);
    let digits: Vec<u8> = mantissa
        .bytes()
        .filter(u8::is_ascii_digit)
        .map(|b| b - b'0')
        .collect();
    // value = 0.d1d2d3... * 10^(exponent + 1); scaled integer of `value * 10^6` rounded half up.
    let decimal_point = exponent + 1;
    let wanted = decimal_point + 6; // number of digits kept
    let mut kept: Vec<u8> = Vec::new();
    let mut round_up = false;
    if wanted >= 0 {
        for (i, d) in digits.iter().enumerate() {
            if (i as i32) < wanted {
                kept.push(*d);
            } else if i as i32 == wanted {
                round_up = *d >= 5;
            }
        }
        while (kept.len() as i32) < wanted {
            kept.push(0);
        }
    } else {
        // everything is below the printed precision
        kept.clear();
    }
    if round_up {
        let mut i = kept.len();
        loop {
            if i == 0 {
                kept.insert(0, 1);
                break;
            }
            i -= 1;
            if kept[i] == 9 {
                kept[i] = 0;
            } else {
                kept[i] += 1;
                break;
            }
        }
    }
    let mut text: String = kept.iter().map(|d| char::from(b'0' + d)).collect();
    if text.len() < 7 {
        text = format!("{}{text}", "0".repeat(7 - text.len()));
    }
    let split = text.len() - 6;
    let out = format!("{}.{}", &text[..split], &text[split..]);
    if negative { format!("-{out}") } else { out }
}

fn perform_teleport(
    source: &CommandSourceStack,
    victim: &EntityRef,
    level: &DimensionKey,
    x: f64,
    y: f64,
    z: f64,
    relatives: Relatives,
    y_rot: f32,
    x_rot: f32,
    look_at: Option<&LookAt>,
) -> Result<(), CommandSyntaxException> {
    if !is_in_spawnable_bounds(BlockPos::containing(x, y, z)) {
        return Err(INVALID_POSITION.create());
    }
    let (current, current_y_rot, current_x_rot) = source.with_server(|server| {
        host::get_entity(server, victim).map_or((Vec3::ZERO, 0.0, 0.0), |e| {
            (e.base().position(), e.base().y_rot(), e.base().x_rot())
        })
    });
    let relative_or_absolute = |relative: Relative, value: f64, current: f64| {
        if relatives.contains(relative) {
            value - current
        } else {
            value
        }
    };
    let (rx, ry, rz) = (
        relative_or_absolute(Relative::X, x, current.x),
        relative_or_absolute(Relative::Y, y, current.y),
        relative_or_absolute(Relative::Z, z, current.z),
    );
    let new_y_rot = mth::wrap_degrees_f32(if relatives.contains(Relative::YRot) {
        y_rot - current_y_rot
    } else {
        y_rot
    });
    let new_x_rot = mth::wrap_degrees_f32(if relatives.contains(Relative::XRot) {
        x_rot - current_x_rot
    } else {
        x_rot
    });
    let teleported = source.with_server(|server| {
        host::teleport_entity(
            server,
            victim,
            level,
            Vec3::new(rx, ry, rz),
            relatives,
            new_y_rot,
            new_x_rot,
        )
    });
    if let Some(victim) = teleported {
        if let Some(look_at) = look_at {
            look_at.perform(source, &victim);
        }
        source.with_server(|server| host::after_teleport(server, &victim));
    }
    Ok(())
}
