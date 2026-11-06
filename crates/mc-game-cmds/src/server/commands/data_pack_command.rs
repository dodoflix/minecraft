//! `DataPackCommand`: `/datapack enable|disable|list|create`.
use super::reload_command::reload_packs;
use mc_brigadier::arguments::{StringArgumentType, escape_if_required};
use mc_brigadier::{
    ArgumentBuilder, CommandContext, CommandSyntaxException, Dynamic2CommandExceptionType,
    DynamicCommandExceptionType,
};
use mc_game_core::commands::arguments::ComponentArgument;
use mc_game_core::commands::component_utils::{ResolutionContext, resolve};
use mc_game_core::commands::message::dynamic_translatable;
use mc_game_core::commands::shared_suggestion_provider::suggest;
use mc_game_core::commands::{
    CommandSourceStack, LEVEL_GAMEMASTERS, LEVEL_OWNERS, argument, has_permission, literal,
};
use mc_game_core::registry::feature_flags::{FeatureFlagSetExt, print_missing_flags};
use mc_game_core::server::packs::{DATA_PACK_FORMAT, Pack, PackSource};
use mc_game_core::{register_command, translatable};
use mc_text::Component;
use std::sync::{Arc, LazyLock};

static ERROR_UNKNOWN_PACK: LazyLock<DynamicCommandExceptionType<String>> =
    LazyLock::new(|| dynamic_translatable("commands.datapack.unknown"));
static ERROR_PACK_ALREADY_ENABLED: LazyLock<DynamicCommandExceptionType<String>> =
    LazyLock::new(|| dynamic_translatable("commands.datapack.enable.failed"));
static ERROR_PACK_ALREADY_DISABLED: LazyLock<DynamicCommandExceptionType<String>> =
    LazyLock::new(|| dynamic_translatable("commands.datapack.disable.failed"));
static ERROR_CANNOT_DISABLE_FEATURE: LazyLock<DynamicCommandExceptionType<String>> =
    LazyLock::new(|| dynamic_translatable("commands.datapack.disable.failed.feature"));
static ERROR_PACK_FEATURES_NOT_ENABLED: LazyLock<Dynamic2CommandExceptionType<String, String>> =
    LazyLock::new(|| {
        Dynamic2CommandExceptionType::new(|id: String, flags: String| {
            mc_game_core::commands::message::message(translatable!(
                "commands.datapack.enable.failed.no_flags",
                id,
                flags
            ))
        })
    });
static ERROR_PACK_INVALID_NAME: LazyLock<DynamicCommandExceptionType<String>> =
    LazyLock::new(|| dynamic_translatable("commands.datapack.create.invalid_name"));
static ERROR_PACK_INVALID_FULL_NAME: LazyLock<DynamicCommandExceptionType<String>> =
    LazyLock::new(|| dynamic_translatable("commands.datapack.create.invalid_full_name"));
static ERROR_PACK_ALREADY_EXISTS: LazyLock<DynamicCommandExceptionType<String>> =
    LazyLock::new(|| dynamic_translatable("commands.datapack.create.already_exists"));
static ERROR_PACK_METADATA_ENCODE_FAILURE: LazyLock<Dynamic2CommandExceptionType<String, String>> =
    LazyLock::new(|| {
        Dynamic2CommandExceptionType::new(|id: String, error: String| {
            mc_game_core::commands::message::message(translatable!(
                "commands.datapack.create.metadata_encode_failure",
                id,
                error
            ))
        })
    });
static ERROR_PACK_IO_FAILURE: LazyLock<DynamicCommandExceptionType<String>> =
    LazyLock::new(|| dynamic_translatable("commands.datapack.create.io_failure"));

/// `Inserter`: where in the selected list a pack goes.
type Inserter<'a> =
    dyn Fn(&mut Vec<Arc<Pack>>, Arc<Pack>) -> Result<(), CommandSyntaxException> + 'a;

fn selected_packs(
    c: &CommandContext<CommandSourceStack>,
    builder: mc_brigadier::SuggestionsBuilder,
) -> Result<mc_brigadier::Suggestions, CommandSyntaxException> {
    let ids = c
        .get_source()
        .with_server(|server| server.pack_repository.selected_ids());
    Ok(suggest(
        ids.iter().map(|id| escape_if_required(id)),
        builder,
    ))
}

fn unselected_packs(
    c: &CommandContext<CommandSourceStack>,
    builder: mc_brigadier::SuggestionsBuilder,
) -> Result<mc_brigadier::Suggestions, CommandSyntaxException> {
    let enabled_features = c.get_source().enabled_features();
    let ids: Vec<String> = c.get_source().with_server(|server| {
        let selected = server.pack_repository.selected_ids();
        server
            .pack_repository
            .available_packs()
            .filter(|pack| pack.requested_features().is_subset_of(enabled_features))
            .map(|pack| pack.id().to_owned())
            .filter(|id| !selected.contains(id))
            .map(|id| escape_if_required(&id))
            .collect()
    });
    Ok(suggest(ids, builder))
}

register_command!("DataPackCommand", |ctx| {
    fn enable(
        c: &CommandContext<CommandSourceStack>,
        inserter: &Inserter<'_>,
    ) -> Result<i32, CommandSyntaxException> {
        enable_pack(c.get_source(), get_pack(c, "name", true)?, inserter)
    }
    let relative = |after: bool| {
        move |c: &CommandContext<CommandSourceStack>| {
            enable(c, &|selected: &mut Vec<Arc<Pack>>, pack| {
                let existing = get_pack(c, "existing", false)?;
                let index = selected
                    .iter()
                    .position(|p| *p == existing)
                    .expect("an enabled pack is selected");
                selected.insert(if after { index + 1 } else { index }, pack);
                Ok(())
            })
        }
    };
    ctx.dispatcher.register(
        literal("datapack")
            .requires(has_permission(LEVEL_GAMEMASTERS))
            .then(
                literal("enable").then(
                    argument("name", StringArgumentType::string())
                        .suggests(unselected_packs)
                        .executes(|c| {
                            enable(c, &|l: &mut Vec<Arc<Pack>>, p: Arc<Pack>| {
                                p.default_position().insert(
                                    l,
                                    p.clone(),
                                    |p| p.selection_config(),
                                    false,
                                );
                                Ok(())
                            })
                        })
                        .then(
                            literal("after").then(
                                argument("existing", StringArgumentType::string())
                                    .suggests(selected_packs)
                                    .executes(relative(true)),
                            ),
                        )
                        .then(
                            literal("before").then(
                                argument("existing", StringArgumentType::string())
                                    .suggests(selected_packs)
                                    .executes(relative(false)),
                            ),
                        )
                        .then(literal("last").executes(|c| {
                            enable(c, &|l: &mut Vec<Arc<Pack>>, p| {
                                l.push(p);
                                Ok(())
                            })
                        }))
                        .then(literal("first").executes(|c| {
                            enable(c, &|l: &mut Vec<Arc<Pack>>, p| {
                                l.insert(0, p);
                                Ok(())
                            })
                        })),
                ),
            )
            .then(
                literal("disable").then(
                    argument("name", StringArgumentType::string())
                        .suggests(selected_packs)
                        .executes(|c| {
                            Ok(disable_pack(c.get_source(), get_pack(c, "name", false)?))
                        }),
                ),
            )
            .then(
                literal("list")
                    .executes(|c| Ok(list_packs(c.get_source())))
                    .then(
                        literal("available").executes(|c| Ok(list_available_packs(c.get_source()))),
                    )
                    .then(literal("enabled").executes(|c| Ok(list_enabled_packs(c.get_source())))),
            )
            .then(
                literal("create")
                    .requires(has_permission(LEVEL_OWNERS))
                    .then(argument("id", StringArgumentType::string()).then(
                        argument("description", ComponentArgument::text_component()).executes(
                            |c| {
                                let description = resolve(
                                    &ResolutionContext::create(c.get_source()),
                                    &ComponentArgument::get_raw_component(c, "description"),
                                )?;
                                create_pack(
                                    c.get_source(),
                                    StringArgumentType::get_string(c, "id"),
                                    &description,
                                )
                            },
                        ),
                    )),
            ),
    );
});

/// `FileUtil.isValidPathSegment`.
fn is_valid_path_segment(segment: &str) -> bool {
    segment != ".."
        && segment != "."
        && !segment.is_empty()
        && segment
            .bytes()
            .all(|b| matches!(b, b'-' | b'.' | b'_' | b'a'..=b'z' | b'0'..=b'9'))
}

/// `FileUtil.isPathPartPortable`: not a reserved Windows file name (`RESERVED_WINDOWS_FILENAMES`, case-insensitive).
fn is_path_part_portable(name: &str) -> bool {
    if name.ends_with('.') {
        return false;
    }
    let lower = name.to_lowercase();
    let reserved = |base: &str| {
        lower == base
            || lower
                .strip_prefix(base)
                .is_some_and(|rest| rest.starts_with('.'))
    };
    let numbered = |prefix: &str| ('1'..='9').any(|d| reserved(&format!("{prefix}{d}")));
    !(["com", "clock$", "con", "prn", "aux", "nul"]
        .iter()
        .any(|b| reserved(b))
        || numbered("com")
        || numbered("lpt"))
}

/// `createPack`.
fn create_pack(
    source: &CommandSourceStack,
    id: &str,
    description: &Component,
) -> Result<i32, CommandSyntaxException> {
    let datapack_dir = source.with_server(|server| {
        server
            .storage_source
            .get_level_path(mc_world::level_storage::LevelResource::DATAPACK_DIR)
    });
    if !is_valid_path_segment(id) {
        return Err(ERROR_PACK_INVALID_NAME.create(id.to_owned()));
    }
    if !is_path_part_portable(id) {
        return Err(ERROR_PACK_INVALID_FULL_NAME.create(id.to_owned()));
    }
    let pack_dir = datapack_dir.join(id);
    if pack_dir.exists() {
        return Err(ERROR_PACK_ALREADY_EXISTS.create(id.to_owned()));
    }
    // `PackMetadataSection.SERVER_TYPE` encode: `min_format`/`max_format` are compact (`121` for `121.0` and `121.*`).
    let description_json = description
        .to_json()
        .map_err(|e| ERROR_PACK_METADATA_ENCODE_FAILURE.create(id.to_owned(), e.0))?;
    let format = DATA_PACK_FORMAT.major;
    let mcmeta = serde_json::json!({"pack": {"description": description_json, "min_format": format, "max_format": format}});
    let written = (|| -> std::io::Result<()> {
        std::fs::create_dir(&pack_dir)?;
        std::fs::create_dir(pack_dir.join("data"))?;
        std::fs::write(
            pack_dir.join("pack.mcmeta"),
            serde_json::to_string_pretty(&mcmeta).map_err(std::io::Error::other)?,
        )
    })();
    if let Err(e) = written {
        let absolute = std::path::absolute(&datapack_dir).unwrap_or(datapack_dir);
        log::warn!(
            "Failed to create pack at {}\n{}",
            absolute.display(),
            mc_game_core::server::packs::throwable("java.io.IOException", &e.to_string())
        );
        return Err(ERROR_PACK_IO_FAILURE.create(id.to_owned()));
    }
    source.send_success(
        || translatable!("commands.datapack.create.success", id.to_owned()),
        true,
    );
    Ok(1)
}

/// `enablePack`.
fn enable_pack(
    source: &CommandSourceStack,
    unopened: Arc<Pack>,
    inserter: &Inserter<'_>,
) -> Result<i32, CommandSyntaxException> {
    let mut selected: Vec<Arc<Pack>> =
        source.with_server(|server| server.pack_repository.selected_packs().to_vec());
    inserter(&mut selected, unopened.clone())?;
    source.send_success(
        || translatable!("commands.datapack.modify.enable", unopened.chat_link(true)),
        true,
    );
    reload_packs(
        &selected
            .iter()
            .map(|p| p.id().to_owned())
            .collect::<Vec<_>>(),
        source,
    );
    Ok(selected.len() as i32)
}

/// `disablePack`.
fn disable_pack(source: &CommandSourceStack, unopened: Arc<Pack>) -> i32 {
    let mut selected: Vec<Arc<Pack>> =
        source.with_server(|server| server.pack_repository.selected_packs().to_vec());
    selected.retain(|p| *p != unopened);
    source.send_success(
        || translatable!("commands.datapack.modify.disable", unopened.chat_link(true)),
        true,
    );
    reload_packs(
        &selected
            .iter()
            .map(|p| p.id().to_owned())
            .collect::<Vec<_>>(),
        source,
    );
    selected.len() as i32
}

fn list_packs(source: &CommandSourceStack) -> i32 {
    list_enabled_packs(source) + list_available_packs(source)
}

/// `listAvailablePacks`.
fn list_available_packs(source: &CommandSourceStack) -> i32 {
    let enabled_features = source.enabled_features();
    let unselected: Vec<Arc<Pack>> = source.with_server(|server| {
        server.pack_repository.reload();
        let selected = server.pack_repository.selected_packs();
        server
            .pack_repository
            .available_packs()
            .filter(|p| {
                !selected.contains(p) && p.requested_features().is_subset_of(enabled_features)
            })
            .cloned()
            .collect()
    });
    if unselected.is_empty() {
        source.send_success(
            || translatable!("commands.datapack.list.available.none"),
            false,
        );
    } else {
        source.send_success(
            || {
                translatable!(
                    "commands.datapack.list.available.success",
                    unselected.len() as i32,
                    mc_text::util::format_list(
                        unselected.iter(),
                        &mc_text::util::default_separator(),
                        |p| p.chat_link(false)
                    )
                )
            },
            false,
        );
    }
    unselected.len() as i32
}

/// `listEnabledPacks`.
fn list_enabled_packs(source: &CommandSourceStack) -> i32 {
    let selected: Vec<Arc<Pack>> = source.with_server(|server| {
        server.pack_repository.reload();
        server.pack_repository.selected_packs().to_vec()
    });
    if selected.is_empty() {
        source.send_success(
            || translatable!("commands.datapack.list.enabled.none"),
            false,
        );
    } else {
        source.send_success(
            || {
                translatable!(
                    "commands.datapack.list.enabled.success",
                    selected.len() as i32,
                    mc_text::util::format_list(
                        selected.iter(),
                        &mc_text::util::default_separator(),
                        |p| p.chat_link(true)
                    )
                )
            },
            false,
        );
    }
    selected.len() as i32
}

/// `getPack`.
fn get_pack(
    context: &CommandContext<CommandSourceStack>,
    name: &str,
    enabling: bool,
) -> Result<Arc<Pack>, CommandSyntaxException> {
    let id = StringArgumentType::get_string(context, name);
    let source = context.get_source();
    let (pack, enabled) = source.with_server(|server| {
        let pack = server.pack_repository.get_pack(id).cloned();
        let enabled = pack
            .as_ref()
            .is_some_and(|p| server.pack_repository.selected_packs().contains(p));
        (pack, enabled)
    });
    let Some(pack) = pack else {
        return Err(ERROR_UNKNOWN_PACK.create(id.to_owned()));
    };
    if enabling && enabled {
        return Err(ERROR_PACK_ALREADY_ENABLED.create(id.to_owned()));
    }
    if !enabling && !enabled {
        return Err(ERROR_PACK_ALREADY_DISABLED.create(id.to_owned()));
    }
    let available_features = source.enabled_features();
    let requested_features = pack.requested_features();
    if !enabling && !requested_features.is_empty() && pack.pack_source() == PackSource::Feature {
        return Err(ERROR_CANNOT_DISABLE_FEATURE.create(id.to_owned()));
    }
    if !requested_features.is_subset_of(available_features) {
        return Err(ERROR_PACK_FEATURES_NOT_ENABLED.create(
            id.to_owned(),
            print_missing_flags(available_features, requested_features),
        ));
    }
    Ok(pack)
}
