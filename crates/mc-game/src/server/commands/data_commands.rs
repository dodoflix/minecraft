//! `DataCommands`: `/data get|merge|remove|modify` over entity, block entity and command storage NBT.
use super::arg_provider::ArgProvider;
use super::data::{DataAccessor, block_data_accessor, entity_data_accessor, storage_data_accessor};
use super::loot_context_sources::add_context_sources;
use crate::commands::arguments::nbt_path_argument::{ERROR_DATA_TOO_DEEP, NbtPath};
use crate::commands::arguments::resource_arguments::{ContextFloatProviderArgument, ContextIntProviderArgument};
use crate::commands::arguments::{CompoundTagArgument, NbtPathArgument, NbtTagArgument};
use crate::commands::message::{dynamic_translatable, message, simple_translatable};
use crate::commands::{CommandBuildContext, CommandSourceStack, LEVEL_GAMEMASTERS, argument, has_permission, literal};
use crate::{register_command, translatable};
use mc_brigadier::arguments::{DoubleArgumentType, IntegerArgumentType, double_arg, integer};
use mc_brigadier::utf16::{len16, substring};
use mc_brigadier::{
    ArgumentBuilder, CommandContext, CommandSyntaxException, Dynamic2CommandExceptionType, DynamicCommandExceptionType, LiteralArgumentBuilder,
    NodeBuilder, SimpleCommandExceptionType,
};
use mc_core::math::mth;
use mc_nbt::{CompoundTag, Tag};
use std::sync::{Arc, LazyLock};

static ERROR_MERGE_UNCHANGED: LazyLock<SimpleCommandExceptionType> = LazyLock::new(|| simple_translatable("commands.data.merge.failed"));
static ERROR_GET_NOT_NUMBER: LazyLock<DynamicCommandExceptionType<String>> = LazyLock::new(|| dynamic_translatable("commands.data.get.invalid"));
static ERROR_GET_NON_EXISTENT: LazyLock<DynamicCommandExceptionType<String>> = LazyLock::new(|| dynamic_translatable("commands.data.get.unknown"));
static ERROR_MULTIPLE_TAGS: LazyLock<SimpleCommandExceptionType> = LazyLock::new(|| simple_translatable("commands.data.get.multiple"));
static ERROR_EXPECTED_OBJECT: LazyLock<DynamicCommandExceptionType<String>> =
    LazyLock::new(|| dynamic_translatable("commands.data.modify.expected_object"));
static ERROR_EXPECTED_VALUE: LazyLock<DynamicCommandExceptionType<String>> = LazyLock::new(|| dynamic_translatable("commands.data.modify.expected_value"));
static ERROR_INVALID_SUBSTRING: LazyLock<Dynamic2CommandExceptionType<i32, i32>> = LazyLock::new(|| {
    Dynamic2CommandExceptionType::new(|start: i32, end: i32| message(translatable!("commands.data.modify.invalid_substring", start, end)))
});

pub type Accessor = Box<dyn DataAccessor>;

/// `DataCommands.ALL_PROVIDERS`.
pub const ALL_PROVIDERS: [super::arg_provider::Factory<Accessor>; 3] =
    [entity_data_accessor::provider, block_data_accessor::provider, storage_data_accessor::provider];

/// `DataCommands.TARGET_PROVIDERS`.
pub static TARGET_PROVIDERS: LazyLock<Vec<ArgProvider<Accessor>>> = LazyLock::new(|| ArgProvider::build_list("target", &ALL_PROVIDERS));

/// `DataCommands.SOURCE_PROVIDERS`.
pub static SOURCE_PROVIDERS: LazyLock<Vec<ArgProvider<Accessor>>> = LazyLock::new(|| ArgProvider::build_list("source", &ALL_PROVIDERS));

/// `DataCommands.DataManipulator.modify(context, targetData, targetPath, source)`.
type Manipulator =
    Arc<dyn Fn(&CommandContext<CommandSourceStack>, &mut CompoundTag, &NbtPath, &[Tag]) -> Result<i32, CommandSyntaxException> + Send + Sync>;

/// `DataCommands.DataManipulatorDecorator`: a fresh `from`/`string`/`compute`/`value` node for the manipulator.
type Decorator = Box<dyn Fn(Manipulator) -> NodeBuilder<CommandSourceStack>>;

register_command!("DataCommands", |ctx| {
    let mut root = literal("data").requires(has_permission(LEVEL_GAMEMASTERS));
    for target_provider in TARGET_PROVIDERS.iter() {
        let tp = target_provider.clone();
        root = root
            .then(target_provider.wrap(literal("merge"), |p| {
                p.then(argument("nbt", CompoundTagArgument::compound_tag()).executes(move |c| {
                    merge_data(c.get_source(), &*tp.access(c)?, CompoundTagArgument::get_compound_tag(c, "nbt"))
                }))
            }))
            .then(target_provider.wrap(literal("get"), {
                let (tp_get, tp_path, tp_scale) = (target_provider.clone(), target_provider.clone(), target_provider.clone());
                |p| {
                    p.executes(move |c| get_data(c.get_source(), &*tp_get.access(c)?)).then(
                        argument("path", NbtPathArgument::nbt_path())
                            .executes(move |c| get_data_at(c.get_source(), &*tp_path.access(c)?, &NbtPathArgument::get_path(c, "path")))
                            .then(argument("scale", double_arg()).executes(move |c| {
                                get_numeric(
                                    c.get_source(),
                                    &*tp_scale.access(c)?,
                                    &NbtPathArgument::get_path(c, "path"),
                                    DoubleArgumentType::get_double(c, "scale"),
                                )
                            })),
                    )
                }
            }))
            .then({
                let tp = target_provider.clone();
                target_provider.wrap(literal("remove"), |p| {
                    p.then(
                        argument("path", NbtPathArgument::nbt_path())
                            .executes(move |c| remove_data(c.get_source(), &*tp.access(c)?, &NbtPathArgument::get_path(c, "path"))),
                    )
                })
            })
            .then(decorate_modification(ctx.build_context));
    }
    ctx.dispatcher.register(root);
});

/// `DataCommands.getAsText(tag)`.
fn get_as_text(tag: &Tag) -> Result<String, CommandSyntaxException> {
    match tag {
        Tag::String(s) => Ok(s.clone()),
        t if t.is_numeric() => Ok(t.to_string()),
        t => Err(ERROR_EXPECTED_VALUE.create(t.to_string())),
    }
}

/// `DataCommands.stringifyTagList(source, stringProcessor)`.
fn stringify_tag_list(source: Vec<Tag>, processor: &dyn Fn(&str) -> Result<String, CommandSyntaxException>) -> Result<Vec<Tag>, CommandSyntaxException> {
    source.iter().map(|tag| Ok(Tag::String(processor(&get_as_text(tag)?)?))).collect()
}

/// `DataCommands.decorateModification(buildContext, nodeSupplier)`.
fn decorate_modification(build_context: &CommandBuildContext) -> LiteralArgumentBuilder<CommandSourceStack> {
    let mut modify = literal("modify");
    for target_provider in TARGET_PROVIDERS.iter() {
        modify = target_provider.wrap(modify, |t| {
            let mut target_path_node = argument("targetPath", NbtPathArgument::nbt_path());
            let rests = decorators(target_provider, build_context);
            let modes: [(&str, bool, Manipulator); 5] = [
                ("insert", true, manipulator_of(|c, target, path, source| with_root(target, |root| insert(path, IntegerArgumentType::get_integer(c, "index"), root, source)))),
                ("prepend", false, manipulator_of(|_, target, path, source| with_root(target, |root| insert(path, 0, root, source)))),
                ("append", false, manipulator_of(|_, target, path, source| with_root(target, |root| insert(path, -1, root, source)))),
                (
                    "set",
                    false,
                    manipulator_of(|_, target, path, source| {
                        let last = source.last().expect("a modification source has at least one tag");
                        with_root(target, |root| path.set(root, last))
                    }),
                ),
                ("merge", false, manipulator_of(|_, target, path, source| merge_into(target, path, source))),
            ];
            for (name, has_index, manipulator) in modes {
                let mut mode_node = literal(name);
                if has_index {
                    let mut index_node = argument("index", integer());
                    for rest in &rests {
                        index_node = index_node.then(rest(manipulator.clone()));
                    }
                    mode_node = mode_node.then(index_node);
                } else {
                    for rest in &rests {
                        mode_node = mode_node.then(rest(manipulator.clone()));
                    }
                }
                target_path_node = target_path_node.then(mode_node);
            }
            t.then(target_path_node)
        });
    }
    modify
}

/// Coerces a closure to a [`Manipulator`] (fixes its higher-ranked signature).
fn manipulator_of(
    f: impl Fn(&CommandContext<CommandSourceStack>, &mut CompoundTag, &NbtPath, &[Tag]) -> Result<i32, CommandSyntaxException> + Send + Sync + 'static,
) -> Manipulator {
    Arc::new(f)
}

/// The `Decorator`s of `decorateModification`, in the order vanilla calls `nodeSupplier.accept`: `from` and `string` per
/// source provider, `compute`, `value`.
fn decorators(target_provider: &ArgProvider<Accessor>, build_context: &CommandBuildContext) -> Vec<Decorator> {
    let mut rests: Vec<Decorator> = Vec::new();
    for source_provider in SOURCE_PROVIDERS.iter() {
        let (tp, sp) = (target_provider.clone(), source_provider.clone());
        rests.push(Box::new(move |manipulator| {
            let (tp, sp) = (tp.clone(), sp.clone());
            let node = sp.clone().wrap(literal("from"), |s| {
                let (tp_path, sp_path, m_path) = (tp.clone(), sp.clone(), manipulator.clone());
                s.executes(move |c| manipulate_data(c, &tp, &manipulator, get_singleton_source(c, &sp)?)).then(
                    argument("sourcePath", NbtPathArgument::nbt_path())
                        .executes(move |c| manipulate_data(c, &tp_path, &m_path, resolve_source_path(c, &sp_path)?)),
                )
            });
            NodeBuilder::Literal(node)
        }));
        let (tp, sp) = (target_provider.clone(), source_provider.clone());
        rests.push(Box::new(move |manipulator| {
            let (tp, sp) = (tp.clone(), sp.clone());
            let node = sp.clone().wrap(literal("string"), |s| {
                let (tp1, sp1, m1) = (tp.clone(), sp.clone(), manipulator.clone());
                let (tp2, sp2, m2) = (tp.clone(), sp.clone(), manipulator.clone());
                let (tp3, sp3, m3) = (tp.clone(), sp.clone(), manipulator.clone());
                s.executes(move |c| manipulate_data(c, &tp, &manipulator, stringify_tag_list(get_singleton_source(c, &sp)?, &|s| Ok(s.to_owned()))?))
                    .then(
                        argument("sourcePath", NbtPathArgument::nbt_path())
                            .executes(move |c| manipulate_data(c, &tp1, &m1, stringify_tag_list(resolve_source_path(c, &sp1)?, &|s| Ok(s.to_owned()))?))
                            .then(
                                argument("start", integer())
                                    .executes(move |c| {
                                        let start = IntegerArgumentType::get_integer(c, "start");
                                        manipulate_data(c, &tp2, &m2, stringify_tag_list(resolve_source_path(c, &sp2)?, &|s| substring_from_index(s, start))?)
                                    })
                                    .then(argument("end", integer()).executes(move |c| {
                                        let start = IntegerArgumentType::get_integer(c, "start");
                                        let end = IntegerArgumentType::get_integer(c, "end");
                                        manipulate_data(c, &tp3, &m3, stringify_tag_list(resolve_source_path(c, &sp3)?, &|s| substring_range(s, start, end))?)
                                    })),
                            ),
                    )
            });
            NodeBuilder::Literal(node)
        }));
    }
    let build_context = build_context.clone();
    rests.push(Box::new(move |_manipulator| {
        let build_context = build_context.clone();
        // SEAM(loot): the providers evaluate against a `LootContext` (`getFloatProvider(c, "provider").value().getFloat(lootContext)`
        // -> `manipulate_data(.., [FloatTag])`, likewise `getInt` -> `[IntTag]`); no context can be created yet.
        NodeBuilder::Literal(add_context_sources(literal("compute"), &|decorator, output| {
            let d_float = decorator.clone();
            output(NodeBuilder::Literal(literal("float").then(
                argument("provider", ContextFloatProviderArgument::float_provider(&build_context)).executes(move |c| match d_float(c)? {}),
            )));
            output(NodeBuilder::Literal(literal("integer").then(
                argument("provider", ContextIntProviderArgument::int_provider(&build_context)).executes(move |c| match decorator(c)? {}),
            )));
        }))
    }));
    let tp = target_provider.clone();
    rests.push(Box::new(move |manipulator| {
        let tp = tp.clone();
        NodeBuilder::Literal(literal("value").then(argument("value", NbtTagArgument::nbt_tag()).executes(move |c| {
            manipulate_data(c, &tp, &manipulator, vec![NbtTagArgument::get_nbt_tag(c, "value")])
        })))
    }));
    rests
}

/// Runs `f` on `target` as a root `Tag` (the path API works on `Tag`), keeping `target` valid afterwards.
fn with_root<R>(target: &mut CompoundTag, f: impl FnOnce(&mut Tag) -> R) -> R {
    let mut root = Tag::Compound(std::mem::take(target));
    let result = f(&mut root);
    if let Tag::Compound(compound) = root {
        *target = compound;
    }
    result
}

fn insert(path: &NbtPath, index: i32, root: &mut Tag, source: &[Tag]) -> Result<i32, CommandSyntaxException> {
    let Tag::Compound(target) = root else { unreachable!("with_root wraps a compound") };
    path.insert(index, target, source)
}

/// The `merge` modification mode.
fn merge_into(target: &mut CompoundTag, path: &NbtPath, source: &[Tag]) -> Result<i32, CommandSyntaxException> {
    let mut combined_sources = CompoundTag::new();
    for source_tag in source {
        if NbtPath::is_too_deep(source_tag, 0) {
            return Err(ERROR_DATA_TOO_DEEP.create());
        }
        let Tag::Compound(tag) = source_tag else { return Err(ERROR_EXPECTED_OBJECT.create(source_tag.to_string())) };
        combined_sources.merge(tag);
    }
    let mut changed_count = 0;
    with_root(target, |root| {
        path.get_or_create_mut(root, &|| Tag::Compound(CompoundTag::new()), &mut |target_tag| {
            let Tag::Compound(target_object) = target_tag else { return Err(ERROR_EXPECTED_OBJECT.create(target_tag.to_string())) };
            let original_target = target_object.copy();
            target_object.merge(&combined_sources);
            changed_count += i32::from(original_target != *target_object);
            Ok(())
        })
    })?;
    Ok(changed_count)
}

/// `DataCommands.validatedSubstring(input, start, end)` (UTF-16 indices like `String.substring`).
fn validated_substring(input: &str, start: i32, end: i32) -> Result<String, CommandSyntaxException> {
    if start >= 0 && i64::from(end) <= len16(input) as i64 && start <= end {
        Ok(substring(input, start as usize, end as usize))
    } else {
        Err(ERROR_INVALID_SUBSTRING.create(start, end))
    }
}

/// `DataCommands.getOffset(index, length)`.
fn get_offset(index: i32, length: i32) -> i32 {
    if index >= 0 { index } else { length.wrapping_add(index) }
}

/// `DataCommands.substring(input, start, end)`.
fn substring_range(input: &str, start: i32, end: i32) -> Result<String, CommandSyntaxException> {
    let length = len16(input) as i32;
    validated_substring(input, get_offset(start, length), get_offset(end, length))
}

/// `DataCommands.substring(input, start)`.
fn substring_from_index(input: &str, start: i32) -> Result<String, CommandSyntaxException> {
    let length = len16(input) as i32;
    validated_substring(input, get_offset(start, length), length)
}

/// `DataCommands.getSingletonSource(context, sourceProvider)`.
fn get_singleton_source(context: &CommandContext<CommandSourceStack>, source_provider: &ArgProvider<Accessor>) -> Result<Vec<Tag>, CommandSyntaxException> {
    let source = source_provider.access(context)?;
    Ok(vec![Tag::Compound(source.get_data(context.get_source())?)])
}

/// `DataCommands.resolveSourcePath(context, sourceProvider)`.
fn resolve_source_path(context: &CommandContext<CommandSourceStack>, source_provider: &ArgProvider<Accessor>) -> Result<Vec<Tag>, CommandSyntaxException> {
    let source = source_provider.access(context)?;
    let source_path = NbtPathArgument::get_path(context, "sourcePath");
    source_path.get(&Tag::Compound(source.get_data(context.get_source())?))
}

/// `DataCommands.manipulateData(context, targetProvider, manipulator, source)`.
fn manipulate_data(
    context: &CommandContext<CommandSourceStack>,
    target_provider: &ArgProvider<Accessor>,
    manipulator: &Manipulator,
    source: Vec<Tag>,
) -> Result<i32, CommandSyntaxException> {
    let stack = context.get_source();
    let target = target_provider.access(context)?;
    let target_path = NbtPathArgument::get_path(context, "targetPath");
    let mut target_data = target.get_data(stack)?;
    let result = manipulator(context, &mut target_data, &target_path, &source)?;
    if result == 0 {
        Err(ERROR_MERGE_UNCHANGED.create())
    } else {
        target.set_data(stack, target_data)?;
        stack.send_success(|| target.get_modified_success(stack), true);
        Ok(result)
    }
}

/// `DataCommands.removeData(source, accessor, path)`.
fn remove_data(source: &CommandSourceStack, accessor: &dyn DataAccessor, path: &NbtPath) -> Result<i32, CommandSyntaxException> {
    let mut result = accessor.get_data(source)?;
    let count = with_root(&mut result, |root| path.remove(root));
    if count == 0 {
        Err(ERROR_MERGE_UNCHANGED.create())
    } else {
        accessor.set_data(source, result)?;
        source.send_success(|| accessor.get_modified_success(source), true);
        Ok(count)
    }
}

/// `DataCommands.getSingleTag(path, accessor)`.
pub fn get_single_tag(path: &NbtPath, accessor: &dyn DataAccessor, source: &CommandSourceStack) -> Result<Tag, CommandSyntaxException> {
    let mut tags = path.get(&Tag::Compound(accessor.get_data(source)?))?.into_iter();
    let result = tags.next().expect("NbtPath.get fails instead of returning nothing");
    if tags.next().is_some() { Err(ERROR_MULTIPLE_TAGS.create()) } else { Ok(result) }
}

/// `DataCommands.getData(source, accessor, path)`.
fn get_data_at(source: &CommandSourceStack, accessor: &dyn DataAccessor, path: &NbtPath) -> Result<i32, CommandSyntaxException> {
    let tag = get_single_tag(path, accessor, source)?;
    let result = match &tag {
        Tag::List(l) => l.len() as i32,
        Tag::ByteArray(a) => a.len() as i32,
        Tag::IntArray(a) => a.len() as i32,
        Tag::LongArray(a) => a.len() as i32,
        Tag::Compound(c) => c.len() as i32,
        Tag::String(s) => len16(s) as i32,
        Tag::End => return Err(ERROR_GET_NON_EXISTENT.create(path.to_string())),
        numeric => mth::floor_f64(numeric.as_number().expect("numeric tag").double_value()),
    };
    source.send_success(|| accessor.get_print_success(source, &tag), false);
    Ok(result)
}

/// `DataCommands.getNumeric(source, accessor, path, scale)`.
fn get_numeric(source: &CommandSourceStack, accessor: &dyn DataAccessor, path: &NbtPath, scale: f64) -> Result<i32, CommandSyntaxException> {
    match get_single_tag(path, accessor, source)?.as_number() {
        Some(number) => {
            let result = mth::floor_f64(number.double_value() * scale);
            source.send_success(|| accessor.get_print_success_for_path(source, path, scale, result), false);
            Ok(result)
        }
        None => Err(ERROR_GET_NOT_NUMBER.create(path.to_string())),
    }
}

/// `DataCommands.getData(source, accessor)`.
fn get_data(source: &CommandSourceStack, accessor: &dyn DataAccessor) -> Result<i32, CommandSyntaxException> {
    let data = Tag::Compound(accessor.get_data(source)?);
    source.send_success(|| accessor.get_print_success(source, &data), false);
    Ok(1)
}

/// `DataCommands.mergeData(source, accessor, nbt)`.
fn merge_data(source: &CommandSourceStack, accessor: &dyn DataAccessor, nbt: CompoundTag) -> Result<i32, CommandSyntaxException> {
    let old = accessor.get_data(source)?;
    if NbtPath::is_too_deep(&Tag::Compound(nbt.clone()), 0) {
        return Err(ERROR_DATA_TOO_DEEP.create());
    }
    let mut result = old.copy();
    result.merge(&nbt);
    if old == result {
        return Err(ERROR_MERGE_UNCHANGED.create());
    }
    accessor.set_data(source, result)?;
    source.send_success(|| accessor.get_modified_success(source), true);
    Ok(1)
}
