//! `VersionCommand`.
use mc_brigadier::ArgumentBuilder;
use mc_game_core::commands::{
    CommandSelection, LEVEL_ALL, LEVEL_GAMEMASTERS, has_permission, literal,
};
use mc_game_core::server::packs::{DATA_PACK_FORMAT, RESOURCE_PACK_FORMAT};
use mc_game_core::{register_command, translatable};
use mc_text::Component;
use mc_world::level_storage::{
    MAIN_SERIES, WORLD_VERSION, WORLD_VERSION_NAME, WORLD_VERSION_STABLE,
};

/// `build_time` of the vanilla server jar's `version.json`.
const BUILD_TIME: &str = "2026-09-15T11:20:48+00:00";

register_command!("VersionCommand", |ctx| {
    let check_permissions = ctx.selection != CommandSelection::Integrated;
    ctx.dispatcher.register(
        literal("version")
            .requires(has_permission(if check_permissions {
                LEVEL_GAMEMASTERS
            } else {
                LEVEL_ALL
            }))
            .executes(|c| {
                let source = c.get_source();
                source.send_system_message(&translatable!("commands.version.header"));
                dump_version(|line| source.send_system_message(&line));
                Ok(1)
            }),
    );
});

/// `Date.toString()` of the build time in the JVM default zone (`EEE MMM dd HH:mm:ss zzz yyyy`): `UTC` for offset 0,
/// else the `GMT+hh:mm` form the JVM prints for zones without a short name.
fn build_time_string() -> String {
    let instant =
        chrono::DateTime::parse_from_rfc3339(BUILD_TIME).expect("version.json build_time");
    let local = instant.with_timezone(&chrono::Local);
    let offset = chrono::Offset::fix(local.offset()).local_minus_utc();
    let zone = if offset == 0 {
        "UTC".to_owned()
    } else {
        format!(
            "GMT{}{:02}:{:02}",
            if offset < 0 { '-' } else { '+' },
            offset.abs() / 3600,
            offset.abs() / 60 % 60
        )
    };
    format!(
        "{} {zone} {}",
        local.format("%a %b %d %H:%M:%S"),
        local.format("%Y")
    )
}

/// `VersionCommand.dumpVersion`.
pub fn dump_version(mut output: impl FnMut(Component)) {
    let protocol = mc_protocol::PROTOCOL_VERSION;
    output(translatable!(
        "commands.version.id",
        mc_game_core::server::packs::GAME_VERSION
    ));
    output(translatable!("commands.version.name", WORLD_VERSION_NAME));
    output(translatable!("commands.version.data", WORLD_VERSION));
    output(translatable!("commands.version.series", MAIN_SERIES));
    output(translatable!(
        "commands.version.protocol",
        protocol,
        format!("0x{protocol:x}")
    ));
    output(translatable!(
        "commands.version.build_time",
        Component::literal(build_time_string())
    ));
    output(translatable!(
        "commands.version.pack.resource",
        RESOURCE_PACK_FORMAT.to_string()
    ));
    output(translatable!(
        "commands.version.pack.data",
        DATA_PACK_FORMAT.to_string()
    ));
    output(translatable!(if WORLD_VERSION_STABLE {
        "commands.version.stable.yes"
    } else {
        "commands.version.stable.no"
    }));
}
