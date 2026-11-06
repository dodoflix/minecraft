//! Registry-element arguments: `ResourceArgument`, `ResourceKeyArgument`, `ResourceOrTagArgument`,
//! `ResourceOrTagKeyArgument`, `ResourceSelectorArgument`, `ResourceOrIdArgument` (+ its subclasses).
//!
//! Vanilla's results are typed `Holder`s/`ResourceKey<T>`; here they are registry-qualified ids ([`ResourceKey`],
//! [`TagKey`]) and the getters check the registry like `ResourceKey.cast`. The element values are read from the
//! registries by whoever consumes the key.

use super::{read_identifier, read_identifier_non_empty};
use crate::commands::message::message;
use crate::commands::shared_suggestion_provider::ElementSuggestionType;
use crate::commands::{CommandBuildContext, CommandSourceStack, RegistryView};
use crate::translatable;
use mc_brigadier::arguments::value;
use mc_brigadier::{
    ArgumentType, CommandContext, CommandSyntaxException, Dynamic2CommandExceptionType,
    Dynamic3CommandExceptionType, DynamicCommandExceptionType, ImmutableStringReader, ParsedValue,
    StringReader, Suggestions, SuggestionsBuilder,
};
use mc_core::ident::{Identifier, is_allowed_in_identifier};
use mc_nbt::Tag;
use mc_protocol::packets::play::ArgumentTypeTemplate;
use std::sync::{Arc, LazyLock};

type KeyFilter = Arc<dyn Fn(&Identifier) -> bool + Send + Sync>;

/// `ResourceKey<T>`: a registry key and the element id in it.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct ResourceKey {
    pub registry: Identifier,
    pub id: Identifier,
}

impl ResourceKey {
    pub fn create(registry: &Identifier, id: Identifier) -> Self {
        Self {
            registry: registry.clone(),
            id,
        }
    }

    /// `isFor(registryKey)`.
    pub fn is_for(&self, registry: &Identifier) -> bool {
        self.registry == *registry
    }
}

/// `TagKey<T>`.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct TagKey {
    pub registry: Identifier,
    pub location: Identifier,
}

pub fn registry_id(registry_key: &str) -> Identifier {
    Identifier::with_default_namespace(registry_key).expect("valid registry key")
}

pub static ERROR_UNKNOWN_RESOURCE: LazyLock<Dynamic2CommandExceptionType<String, String>> =
    LazyLock::new(|| {
        Dynamic2CommandExceptionType::new(|id: String, registry: String| {
            message(translatable!("argument.resource.not_found", id, registry))
        })
    });
pub static ERROR_INVALID_RESOURCE_TYPE: LazyLock<
    Dynamic3CommandExceptionType<String, String, String>,
> = LazyLock::new(|| {
    Dynamic3CommandExceptionType::new(|id: String, actual: String, expected: String| {
        message(translatable!(
            "argument.resource.invalid_type",
            id,
            actual,
            expected
        ))
    })
});
static ERROR_UNKNOWN_TAG: LazyLock<Dynamic2CommandExceptionType<String, String>> =
    LazyLock::new(|| {
        Dynamic2CommandExceptionType::new(|id: String, registry: String| {
            message(translatable!(
                "argument.resource_tag.not_found",
                id,
                registry
            ))
        })
    });
static ERROR_INVALID_TAG_TYPE: LazyLock<Dynamic3CommandExceptionType<String, String, String>> =
    LazyLock::new(|| {
        Dynamic3CommandExceptionType::new(|id: String, actual: String, expected: String| {
            message(translatable!(
                "argument.resource_tag.invalid_type",
                id,
                actual,
                expected
            ))
        })
    });
static ERROR_NOT_SUMMONABLE_ENTITY: LazyLock<DynamicCommandExceptionType<String>> =
    LazyLock::new(|| {
        DynamicCommandExceptionType::new(|v: String| {
            message(translatable!("entity.not_summonable", v))
        })
    });
pub static ERROR_NO_MATCHES: LazyLock<Dynamic2CommandExceptionType<String, String>> =
    LazyLock::new(|| {
        Dynamic2CommandExceptionType::new(|selector: String, registry: String| {
            message(translatable!(
                "argument.resource_selector.not_found",
                selector,
                registry
            ))
        })
    });
pub static ERROR_FAILED_TO_PARSE: LazyLock<DynamicCommandExceptionType<String>> =
    LazyLock::new(|| {
        DynamicCommandExceptionType::new(|e: String| {
            message(translatable!("argument.resource_or_id.failed_to_parse", e))
        })
    });
pub static ERROR_NO_SUCH_ELEMENT: LazyLock<Dynamic2CommandExceptionType<String, String>> =
    LazyLock::new(|| {
        Dynamic2CommandExceptionType::new(|id: String, registry: String| {
            message(translatable!(
                "argument.resource_or_id.no_such_element",
                id,
                registry
            ))
        })
    });

fn lookup(context: &CommandBuildContext, registry: &Identifier) -> Arc<dyn RegistryView> {
    context
        .lookup(&registry.to_string())
        .unwrap_or_else(|| panic!("Missing registry: {registry}"))
}

macro_rules! registry_argument {
    ($name:ident) => {
        impl PartialEq for $name {
            fn eq(&self, other: &Self) -> bool {
                self.registry_key == other.registry_key
            }
        }
    };
}

// ---- ResourceArgument ----

#[derive(Clone)]
pub struct ResourceArgument {
    registry_key: Identifier,
    registry_lookup: Arc<dyn RegistryView>,
}

registry_argument!(ResourceArgument);

impl ResourceArgument {
    pub fn resource(context: &CommandBuildContext, registry: &str) -> Self {
        let registry_key = registry_id(registry);
        Self {
            registry_lookup: lookup(context, &registry_key),
            registry_key,
        }
    }

    pub fn registry_key(&self) -> &Identifier {
        &self.registry_key
    }

    /// `getResource(context, name, registryKey)`.
    pub fn get_resource(
        context: &CommandContext<CommandSourceStack>,
        name: &str,
        registry: &str,
    ) -> Result<ResourceKey, CommandSyntaxException> {
        let argument = context.get_argument::<ResourceKey>(name);
        let registry = registry_id(registry);
        if argument.is_for(&registry) {
            Ok(argument.clone())
        } else {
            Err(ERROR_INVALID_RESOURCE_TYPE.create(
                argument.id.to_string(),
                argument.registry.to_string(),
                registry.to_string(),
            ))
        }
    }

    /// `getSummonableEntityType`.
    pub fn get_summonable_entity_type(
        context: &CommandContext<CommandSourceStack>,
        name: &str,
    ) -> Result<mc_data::EntityType, CommandSyntaxException> {
        let key = Self::get_resource(context, name, "entity_type")?;
        match mc_data::EntityType::by_name(&key.id.to_string()) {
            Some(t) if t.data().summonable => Ok(t),
            _ => Err(ERROR_NOT_SUMMONABLE_ENTITY.create(key.id.to_string())),
        }
    }

    /// `getAttribute`.
    pub fn get_attribute(
        context: &CommandContext<CommandSourceStack>,
        name: &str,
    ) -> Result<mc_data::Attribute, CommandSyntaxException> {
        let key = Self::get_resource(context, name, "attribute")?;
        mc_data::Attribute::by_name(&key.id.to_string()).ok_or_else(|| {
            ERROR_UNKNOWN_RESOURCE.create(key.id.to_string(), key.registry.to_string())
        })
    }

    /// `getMobEffect`.
    pub fn get_mob_effect(
        context: &CommandContext<CommandSourceStack>,
        name: &str,
    ) -> Result<mc_data::MobEffect, CommandSyntaxException> {
        let key = Self::get_resource(context, name, "mob_effect")?;
        mc_data::MobEffect::by_name(&key.id.to_string()).ok_or_else(|| {
            ERROR_UNKNOWN_RESOURCE.create(key.id.to_string(), key.registry.to_string())
        })
    }

    /// `getEnchantment`: the registry key (enchantments are data-driven).
    pub fn get_enchantment(
        context: &CommandContext<CommandSourceStack>,
        name: &str,
    ) -> Result<ResourceKey, CommandSyntaxException> {
        Self::get_resource(context, name, "enchantment")
    }
}

impl ArgumentType<CommandSourceStack> for ResourceArgument {
    fn parse(&self, reader: &mut StringReader) -> Result<ParsedValue, CommandSyntaxException> {
        let id = read_identifier(reader)?;
        if self.registry_lookup.contains(&id) {
            Ok(value(ResourceKey::create(&self.registry_key, id)))
        } else {
            Err(ERROR_UNKNOWN_RESOURCE.create_with_context(
                reader,
                id.to_string(),
                self.registry_key.to_string(),
            ))
        }
    }

    fn list_suggestions(
        &self,
        context: &CommandContext<CommandSourceStack>,
        builder: SuggestionsBuilder,
    ) -> Suggestions {
        context.get_source().suggest_registry_elements(
            &self.registry_key.to_string(),
            ElementSuggestionType::Elements,
            builder,
            None,
        )
    }

    fn get_examples(&self) -> Vec<String> {
        ["foo", "foo:bar", "012"].map(String::from).to_vec()
    }

    fn describe(&self) -> String {
        "ResourceArgument".to_owned()
    }
}

// ---- ResourceKeyArgument ----

#[derive(Clone)]
pub struct ResourceKeyArgument {
    registry_key: Identifier,
    filter: Option<KeyFilter>,
}

registry_argument!(ResourceKeyArgument);

static ERROR_INVALID_STRUCTURE: LazyLock<DynamicCommandExceptionType<String>> =
    LazyLock::new(|| {
        DynamicCommandExceptionType::new(|v: String| {
            message(translatable!("commands.place.structure.invalid", v))
        })
    });
static ERROR_INVALID_TEMPLATE_POOL: LazyLock<DynamicCommandExceptionType<String>> =
    LazyLock::new(|| {
        DynamicCommandExceptionType::new(|v: String| {
            message(translatable!("commands.place.jigsaw.invalid", v))
        })
    });
pub static ERROR_INVALID_RECIPE: LazyLock<DynamicCommandExceptionType<String>> =
    LazyLock::new(|| {
        DynamicCommandExceptionType::new(|v: String| message(translatable!("recipe.notFound", v)))
    });
pub static ERROR_INVALID_ADVANCEMENT: LazyLock<DynamicCommandExceptionType<String>> =
    LazyLock::new(|| {
        DynamicCommandExceptionType::new(|v: String| {
            message(translatable!("advancement.advancementNotFound", v))
        })
    });

impl ResourceKeyArgument {
    pub fn key(registry: &str) -> Self {
        Self {
            registry_key: registry_id(registry),
            filter: None,
        }
    }

    /// `key(registry, filter)`: the filter only restricts the suggestions.
    pub fn key_with_filter(
        registry: &str,
        filter: impl Fn(&Identifier) -> bool + Send + Sync + 'static,
    ) -> Self {
        Self {
            registry_key: registry_id(registry),
            filter: Some(Arc::new(filter)),
        }
    }

    /// `getRegistryKey(context, name, registryKey, exceptionType)`.
    pub fn get_registry_key(
        context: &CommandContext<CommandSourceStack>,
        name: &str,
        registry: &str,
        exception_type: &DynamicCommandExceptionType<String>,
    ) -> Result<ResourceKey, CommandSyntaxException> {
        let argument = context.get_argument::<ResourceKey>(name);
        if argument.is_for(&registry_id(registry)) {
            Ok(argument.clone())
        } else {
            Err(exception_type.create(argument.id.to_string()))
        }
    }

    /// `getAdvancement`.
    pub fn get_advancement(
        context: &CommandContext<CommandSourceStack>,
        name: &str,
    ) -> Result<crate::commands::host::AdvancementRef, CommandSyntaxException> {
        let key = Self::get_registry_key(context, name, "advancement", &ERROR_INVALID_ADVANCEMENT)?;
        context
            .get_source()
            .with_server(|server| crate::commands::host::advancement_get(server, &key.id))
            .ok_or_else(|| ERROR_INVALID_ADVANCEMENT.create(key.id.to_string()))
    }

    /// `getRecipe`: the id of an existing learnable (`!isSpecial`) recipe.
    pub fn get_recipe(
        context: &CommandContext<CommandSourceStack>,
        name: &str,
    ) -> Result<Identifier, CommandSyntaxException> {
        let key = Self::get_registry_key(context, name, "recipe", &ERROR_INVALID_RECIPE)?;
        if context
            .get_source()
            .with_server(|server| crate::commands::host::learnable_recipe_exists(server, &key.id))
        {
            Ok(key.id)
        } else {
            Err(ERROR_INVALID_RECIPE.create(key.id.to_string()))
        }
    }

    /// `getStructure`: the key of an existing structure.
    pub fn get_structure(
        context: &CommandContext<CommandSourceStack>,
        name: &str,
    ) -> Result<ResourceKey, CommandSyntaxException> {
        Self::resolve_key(
            context,
            name,
            "worldgen/structure",
            &ERROR_INVALID_STRUCTURE,
        )
    }

    /// `getStructureTemplatePool`.
    pub fn get_structure_template_pool(
        context: &CommandContext<CommandSourceStack>,
        name: &str,
    ) -> Result<ResourceKey, CommandSyntaxException> {
        Self::resolve_key(
            context,
            name,
            "worldgen/template_pool",
            &ERROR_INVALID_TEMPLATE_POOL,
        )
    }

    fn resolve_key(
        context: &CommandContext<CommandSourceStack>,
        name: &str,
        registry: &str,
        exception: &DynamicCommandExceptionType<String>,
    ) -> Result<ResourceKey, CommandSyntaxException> {
        let key = Self::get_registry_key(context, name, registry, exception)?;
        let exists = context
            .get_source()
            .with_server(|server| {
                crate::commands::host::registry_view(server, &registry_id(registry).to_string())
            })
            .is_some_and(|view| view.contains(&key.id));
        if exists {
            Ok(key)
        } else {
            Err(exception.create(key.id.to_string()))
        }
    }
}

impl ArgumentType<CommandSourceStack> for ResourceKeyArgument {
    fn parse(&self, reader: &mut StringReader) -> Result<ParsedValue, CommandSyntaxException> {
        read_identifier(reader).map(|id| value(ResourceKey::create(&self.registry_key, id)))
    }

    fn list_suggestions(
        &self,
        context: &CommandContext<CommandSourceStack>,
        builder: SuggestionsBuilder,
    ) -> Suggestions {
        let filter = self
            .filter
            .as_deref()
            .map(|f| f as &dyn Fn(&Identifier) -> bool);
        context.get_source().suggest_registry_elements(
            &self.registry_key.to_string(),
            ElementSuggestionType::Elements,
            builder,
            filter,
        )
    }

    fn get_examples(&self) -> Vec<String> {
        ["foo", "foo:bar", "012"].map(String::from).to_vec()
    }

    fn describe(&self) -> String {
        "ResourceKeyArgument".to_owned()
    }
}

// ---- ResourceOrTagArgument ----

/// `ResourceOrTagArgument.Result`.
#[derive(Clone, PartialEq, Debug)]
pub enum ResourceOrTag {
    Resource(ResourceKey),
    Tag {
        key: TagKey,
        members: Vec<Identifier>,
    },
}

impl ResourceOrTag {
    /// `test(holder)` for the element `id` of the argument's registry.
    pub fn test(&self, id: &Identifier) -> bool {
        match self {
            ResourceOrTag::Resource(key) => key.id == *id,
            ResourceOrTag::Tag { members, .. } => members.contains(id),
        }
    }

    /// `asPrintable()`.
    pub fn as_printable(&self) -> String {
        match self {
            ResourceOrTag::Resource(key) => key.id.to_string(),
            ResourceOrTag::Tag { key, .. } => format!("#{}", key.location),
        }
    }
}

#[derive(Clone)]
pub struct ResourceOrTagArgument {
    registry_key: Identifier,
    registry_lookup: Arc<dyn RegistryView>,
}

registry_argument!(ResourceOrTagArgument);

impl ResourceOrTagArgument {
    pub fn resource_or_tag(context: &CommandBuildContext, registry: &str) -> Self {
        let registry_key = registry_id(registry);
        Self {
            registry_lookup: lookup(context, &registry_key),
            registry_key,
        }
    }

    /// `getResourceOrTag`.
    pub fn get_resource_or_tag(
        context: &CommandContext<CommandSourceStack>,
        name: &str,
        registry: &str,
    ) -> Result<ResourceOrTag, CommandSyntaxException> {
        let argument = context.get_argument::<ResourceOrTag>(name);
        let registry = registry_id(registry);
        match argument {
            ResourceOrTag::Resource(key) if key.is_for(&registry) => Ok(argument.clone()),
            ResourceOrTag::Tag { key, .. } if key.registry == registry => Ok(argument.clone()),
            ResourceOrTag::Resource(key) => Err(ERROR_INVALID_RESOURCE_TYPE.create(
                key.id.to_string(),
                key.registry.to_string(),
                registry.to_string(),
            )),
            ResourceOrTag::Tag { key, .. } => Err(ERROR_INVALID_TAG_TYPE.create(
                key.location.to_string(),
                key.registry.to_string(),
                registry.to_string(),
            )),
        }
    }
}

impl ArgumentType<CommandSourceStack> for ResourceOrTagArgument {
    fn parse(&self, reader: &mut StringReader) -> Result<ParsedValue, CommandSyntaxException> {
        if reader.can_read() && reader.peek() == '#' {
            let cursor = reader.get_cursor();
            let result = (|| {
                reader.skip();
                let tag_id = read_identifier(reader)?;
                match self.registry_lookup.tag_elements(&tag_id) {
                    Some(members) => Ok(ResourceOrTag::Tag {
                        key: TagKey {
                            registry: self.registry_key.clone(),
                            location: tag_id,
                        },
                        members,
                    }),
                    None => Err(ERROR_UNKNOWN_TAG.create_with_context(
                        reader,
                        tag_id.to_string(),
                        self.registry_key.to_string(),
                    )),
                }
            })();
            return result.map(value).inspect_err(|_| reader.set_cursor(cursor));
        }
        let id = read_identifier(reader)?;
        if self.registry_lookup.contains(&id) {
            Ok(value(ResourceOrTag::Resource(ResourceKey::create(
                &self.registry_key,
                id,
            ))))
        } else {
            Err(ERROR_UNKNOWN_RESOURCE.create_with_context(
                reader,
                id.to_string(),
                self.registry_key.to_string(),
            ))
        }
    }

    fn list_suggestions(
        &self,
        context: &CommandContext<CommandSourceStack>,
        builder: SuggestionsBuilder,
    ) -> Suggestions {
        context.get_source().suggest_registry_elements(
            &self.registry_key.to_string(),
            ElementSuggestionType::All,
            builder,
            None,
        )
    }

    fn get_examples(&self) -> Vec<String> {
        [
            "foo",
            "foo:bar",
            "012",
            "#skeletons",
            "#minecraft:skeletons",
        ]
        .map(String::from)
        .to_vec()
    }

    fn describe(&self) -> String {
        "ResourceOrTagArgument".to_owned()
    }
}

// ---- ResourceOrTagKeyArgument ----

/// `ResourceOrTagKeyArgument.Result`.
#[derive(Clone, PartialEq, Debug)]
pub enum ResourceOrTagKey {
    Resource(ResourceKey),
    Tag(TagKey),
}

impl ResourceOrTagKey {
    /// `asPrintable()`.
    pub fn as_printable(&self) -> String {
        match self {
            ResourceOrTagKey::Resource(key) => key.id.to_string(),
            ResourceOrTagKey::Tag(key) => format!("#{}", key.location),
        }
    }

    fn registry(&self) -> &Identifier {
        match self {
            ResourceOrTagKey::Resource(key) => &key.registry,
            ResourceOrTagKey::Tag(key) => &key.registry,
        }
    }
}

#[derive(Clone)]
pub struct ResourceOrTagKeyArgument {
    registry_key: Identifier,
}

registry_argument!(ResourceOrTagKeyArgument);

impl ResourceOrTagKeyArgument {
    pub fn resource_or_tag_key(registry: &str) -> Self {
        Self {
            registry_key: registry_id(registry),
        }
    }

    /// `getResourceOrTagKey`: `exception_type` is given the printable form of a mismatching argument.
    pub fn get_resource_or_tag_key(
        context: &CommandContext<CommandSourceStack>,
        name: &str,
        registry: &str,
        exception_type: &DynamicCommandExceptionType<String>,
    ) -> Result<ResourceOrTagKey, CommandSyntaxException> {
        let argument = context.get_argument::<ResourceOrTagKey>(name);
        if *argument.registry() == registry_id(registry) {
            Ok(argument.clone())
        } else {
            Err(exception_type.create(argument.as_printable()))
        }
    }
}

impl ArgumentType<CommandSourceStack> for ResourceOrTagKeyArgument {
    fn parse(&self, reader: &mut StringReader) -> Result<ParsedValue, CommandSyntaxException> {
        if reader.can_read() && reader.peek() == '#' {
            let cursor = reader.get_cursor();
            let result = (|| {
                reader.skip();
                let tag_id = read_identifier(reader)?;
                Ok(ResourceOrTagKey::Tag(TagKey {
                    registry: self.registry_key.clone(),
                    location: tag_id,
                }))
            })();
            return result
                .map(value)
                .inspect_err(|_: &CommandSyntaxException| reader.set_cursor(cursor));
        }
        let id = read_identifier(reader)?;
        Ok(value(ResourceOrTagKey::Resource(ResourceKey::create(
            &self.registry_key,
            id,
        ))))
    }

    fn list_suggestions(
        &self,
        context: &CommandContext<CommandSourceStack>,
        builder: SuggestionsBuilder,
    ) -> Suggestions {
        context.get_source().suggest_registry_elements(
            &self.registry_key.to_string(),
            ElementSuggestionType::All,
            builder,
            None,
        )
    }

    fn get_examples(&self) -> Vec<String> {
        [
            "foo",
            "foo:bar",
            "012",
            "#skeletons",
            "#minecraft:skeletons",
        ]
        .map(String::from)
        .to_vec()
    }

    fn describe(&self) -> String {
        "ResourceOrTagKeyArgument".to_owned()
    }
}

// ---- ResourceSelectorArgument ----

#[derive(Clone)]
pub struct ResourceSelectorArgument {
    registry_key: Identifier,
    registry_lookup: Arc<dyn RegistryView>,
}

registry_argument!(ResourceSelectorArgument);

/// `FilenameUtils.wildcardMatch(name, pattern)` (case sensitive), including commons-io's quirks (a `?` after `*` does not
/// backtrack: `*?` only matches names whose `*` part is empty).
fn wildcard_match(name: &str, pattern: &str) -> bool {
    let name: Vec<char> = name.chars().collect();
    // `splitOnTokens`
    let mut tokens: Vec<Vec<char>> = Vec::new();
    let mut text: Vec<char> = Vec::new();
    for c in pattern.chars() {
        if c == '?' || c == '*' {
            if !text.is_empty() {
                tokens.push(std::mem::take(&mut text));
            }
            if c == '?' {
                tokens.push(vec!['?']);
            } else if tokens.last().is_none_or(|t| t != &['*']) {
                tokens.push(vec!['*']);
            }
        } else {
            text.push(c);
        }
    }
    if !text.is_empty() {
        tokens.push(text);
    }
    let index_of = |from: usize, needle: &[char]| -> Option<usize> {
        if from > name.len() {
            return None;
        }
        (from..=name.len().saturating_sub(needle.len()))
            .find(|&i| i + needle.len() <= name.len() && name[i..i + needle.len()] == *needle)
    };
    let mut any_chars = false;
    let (mut text_idx, mut wcs_idx) = (0usize, 0usize);
    let mut backtrack: Vec<(usize, usize)> = Vec::new();
    loop {
        if let Some((w, t)) = backtrack.pop() {
            wcs_idx = w;
            text_idx = t;
            any_chars = true;
        }
        while wcs_idx < tokens.len() {
            let token = &tokens[wcs_idx];
            if token == &['?'] {
                text_idx += 1;
                if text_idx > name.len() {
                    break;
                }
                any_chars = false;
            } else if token == &['*'] {
                any_chars = true;
                if wcs_idx == tokens.len() - 1 {
                    text_idx = name.len();
                }
            } else {
                if any_chars {
                    let Some(found) = index_of(text_idx, token) else {
                        break;
                    };
                    text_idx = found;
                    if let Some(repeat) = index_of(text_idx + 1, token) {
                        backtrack.push((wcs_idx, repeat));
                    }
                } else if !(text_idx + token.len() <= name.len()
                    && name[text_idx..text_idx + token.len()] == **token)
                {
                    break;
                }
                text_idx += token.len();
                any_chars = false;
            }
            wcs_idx += 1;
        }
        if wcs_idx == tokens.len() && text_idx == name.len() {
            return true;
        }
        if backtrack.is_empty() {
            return false;
        }
    }
}

fn read_pattern(reader: &mut StringReader) -> String {
    let start = reader.get_cursor();
    while reader.can_read()
        && (is_allowed_in_identifier(reader.peek()) || reader.peek() == '*' || reader.peek() == '?')
    {
        reader.skip();
    }
    mc_brigadier::utf16::substring(reader.get_string(), start, reader.get_cursor())
}

fn ensure_namespaced(input: &str) -> String {
    if input.contains(':') {
        input.to_owned()
    } else {
        format!("minecraft:{input}")
    }
}

impl ResourceSelectorArgument {
    pub fn resource_selector(context: &CommandBuildContext, registry: &str) -> Self {
        let registry_key = registry_id(registry);
        Self {
            registry_lookup: lookup(context, &registry_key),
            registry_key,
        }
    }

    /// `getSelectedResources`.
    pub fn get_selected_resources(
        context: &CommandContext<CommandSourceStack>,
        name: &str,
    ) -> Vec<ResourceKey> {
        context.get_argument::<Vec<ResourceKey>>(name).clone()
    }
}

impl ArgumentType<CommandSourceStack> for ResourceSelectorArgument {
    fn parse(&self, reader: &mut StringReader) -> Result<ParsedValue, CommandSyntaxException> {
        let pattern = ensure_namespaced(&read_pattern(reader));
        let results: Vec<ResourceKey> = self
            .registry_lookup
            .element_ids()
            .into_iter()
            .filter(|id| wildcard_match(&id.to_string(), &pattern))
            .map(|id| ResourceKey::create(&self.registry_key, id))
            .collect();
        if results.is_empty() {
            Err(ERROR_NO_MATCHES.create_with_context(
                reader,
                pattern,
                self.registry_key.to_string(),
            ))
        } else {
            Ok(value(results))
        }
    }

    fn list_suggestions(
        &self,
        context: &CommandContext<CommandSourceStack>,
        builder: SuggestionsBuilder,
    ) -> Suggestions {
        context.get_source().suggest_registry_elements(
            &self.registry_key.to_string(),
            ElementSuggestionType::Elements,
            builder,
            None,
        )
    }

    fn get_examples(&self) -> Vec<String> {
        ["minecraft:*", "*:asset", "*"].map(String::from).to_vec()
    }

    fn describe(&self) -> String {
        "ResourceSelectorArgument".to_owned()
    }
}

// ---- ResourceOrIdArgument ----

/// `ResourceOrIdArgument`'s result: a reference to a registered element or an inline definition (SNBT, decoded by the
/// consumer with the element codec: loot tables, predicates, ...).
#[derive(Clone, PartialEq, Debug)]
pub enum ResourceOrId {
    Reference(ResourceKey),
    Inline(Tag),
}

/// The shared implementation; the subclasses differ only in registry and wire type.
#[derive(Clone)]
pub struct ResourceOrIdParser {
    registry_key: Identifier,
    element_lookup: Option<Arc<dyn RegistryView>>,
    context: CommandBuildContext,
}

/// `RegistryOps` reference checks of an inline definition against the command build context's registries.
struct BuildContextLookup<'a>(&'a CommandBuildContext);

impl BuildContextLookup<'_> {
    fn view(&self, registry: &str) -> Option<Arc<dyn RegistryView>> {
        let key = if registry.contains(':') {
            registry.to_owned()
        } else {
            format!("minecraft:{registry}")
        };
        self.0.lookup(&key)
    }
}

impl crate::registry::codec::RegistryLookup for BuildContextLookup<'_> {
    fn contains(&self, registry: &str, id: &Identifier) -> crate::registry::codec::Lookup {
        use crate::registry::codec::Lookup;
        match self.view(registry) {
            None => Lookup::NoRegistry,
            Some(view) if view.contains(id) => Lookup::Yes,
            Some(_) => Lookup::No,
        }
    }

    fn tag_exists(&self, registry: &str, tag: &Identifier) -> crate::registry::codec::Lookup {
        use crate::registry::codec::Lookup;
        match self.view(registry) {
            None => Lookup::NoRegistry,
            Some(view) if view.has_tag(tag) => Lookup::Yes,
            Some(_) => Lookup::No,
        }
    }
}

/// `codec.parse(createSerializationContext(ops), value)` of the registries whose elements are code-defined (loot data);
/// other registries are not checked while parsing.
fn check_inline(context: &CommandBuildContext, registry: &str, tag: &Tag) -> Result<(), String> {
    use crate::world::level::storage::loot::holder::with_decode_context;
    fn run<T: 'static>(codec: mc_dfu::Codec<T>, tag: &Tag) -> Result<(), String> {
        mc_dfu::parse_nbt(&codec, tag).into_result().map(|_| ())
    }
    let lookup = BuildContextLookup(context);
    let decode = crate::registry::codec::RegistryDecodeContext::new(&lookup);
    with_decode_context(&decode, || match registry {
        "minecraft:loot_table" => run(
            crate::world::level::storage::loot::table::loot_table_direct_codec(),
            tag,
        ),
        "minecraft:item_modifier" => run(
            crate::world::level::storage::loot::functions::function_codec(),
            tag,
        ),
        "minecraft:predicate" => run(
            crate::world::level::storage::loot::predicates::condition_codec(),
            tag,
        ),
        "minecraft:slot_source" => run(crate::world::item::slot::slot_source_codec(), tag),
        "minecraft:dialog" => mc_protocol::dialog::Dialog::from_inline_tag(tag).map(|_| ()),
        _ => Ok(()),
    })
}

impl ResourceOrIdParser {
    pub fn new(context: &CommandBuildContext, registry: &str) -> Self {
        let registry_key = registry_id(registry);
        Self {
            element_lookup: context.lookup(&registry_key.to_string()),
            registry_key,
            context: context.clone(),
        }
    }

    pub fn parse(&self, reader: &mut StringReader) -> Result<ResourceOrId, CommandSyntaxException> {
        // Grammar: alternative(id, inline SNBT value); the id rule skips whitespace and needs a non-empty identifier.
        reader.skip_whitespace();
        let cursor = reader.get_cursor();
        let contents = match read_identifier_non_empty(reader) {
            Ok(id) => ResourceOrId::Reference(ResourceKey::create(&self.registry_key, id)),
            Err(_) => {
                reader.set_cursor(cursor);
                ResourceOrId::Inline(super::nbt_arguments::parse_tag_as_argument(reader)?)
            }
        };
        match (&contents, &self.element_lookup) {
            (_, None) => Ok(contents),
            (ResourceOrId::Reference(key), Some(lookup)) => {
                if lookup.contains(&key.id) {
                    Ok(contents)
                } else {
                    Err(ERROR_NO_SUCH_ELEMENT.create_with_context(
                        reader,
                        key.id.to_string(),
                        key.registry.to_string(),
                    ))
                }
            }
            (ResourceOrId::Inline(tag), Some(_)) => {
                check_inline(&self.context, &self.registry_key.to_string(), tag)
                    .map_err(|msg| ERROR_FAILED_TO_PARSE.create_with_context(reader, msg))?;
                Ok(contents)
            }
        }
    }
}

macro_rules! resource_or_id_argument {
    ($name:ident, $getter:ident, $ctor:ident, $registry:literal, $template:expr) => {
        #[derive(Clone)]
        pub struct $name {
            parser: ResourceOrIdParser,
        }

        impl $name {
            pub fn $ctor(context: &CommandBuildContext) -> Self {
                Self {
                    parser: ResourceOrIdParser::new(context, $registry),
                }
            }

            pub fn $getter(
                context: &CommandContext<CommandSourceStack>,
                name: &str,
            ) -> ResourceOrId {
                context.get_argument::<ResourceOrId>(name).clone()
            }
        }

        impl ArgumentType<CommandSourceStack> for $name {
            fn parse(
                &self,
                reader: &mut StringReader,
            ) -> Result<ParsedValue, CommandSyntaxException> {
                self.parser.parse(reader).map(value)
            }

            fn list_suggestions(
                &self,
                context: &CommandContext<CommandSourceStack>,
                builder: SuggestionsBuilder,
            ) -> Suggestions {
                context.get_source().suggest_registry_elements(
                    &self.parser.registry_key.to_string(),
                    ElementSuggestionType::Elements,
                    builder,
                    None,
                )
            }

            fn get_examples(&self) -> Vec<String> {
                ["foo", "foo:bar", "012", "{}", "true"]
                    .map(String::from)
                    .to_vec()
            }

            fn describe(&self) -> String {
                stringify!($name).to_owned()
            }
        }

        crate::register_argument_type!($name, |_a| $template);
    };
}

resource_or_id_argument!(
    LootTableArgument,
    get_loot_table,
    loot_table,
    "loot_table",
    ArgumentTypeTemplate::LootTable
);
resource_or_id_argument!(
    LootModifierArgument,
    get_loot_modifier,
    loot_modifier,
    "item_modifier",
    ArgumentTypeTemplate::LootModifier
);
resource_or_id_argument!(
    LootPredicateArgument,
    get_loot_predicate,
    loot_predicate,
    "predicate",
    ArgumentTypeTemplate::LootPredicate
);
resource_or_id_argument!(
    ContextFloatProviderArgument,
    get_float_provider,
    float_provider,
    "context_float_provider",
    ArgumentTypeTemplate::ContextFloatProvider
);
resource_or_id_argument!(
    ContextIntProviderArgument,
    get_int_provider,
    int_provider,
    "context_int_provider",
    ArgumentTypeTemplate::ContextIntProvider
);
resource_or_id_argument!(
    DialogArgument,
    get_dialog,
    dialog,
    "dialog",
    ArgumentTypeTemplate::Dialog
);
resource_or_id_argument!(
    FeatureArgument,
    get_feature,
    feature,
    "worldgen/feature",
    ArgumentTypeTemplate::Feature
);

crate::register_argument_type!(ResourceArgument, |a| ArgumentTypeTemplate::Resource(
    a.registry_key.clone()
));
crate::register_argument_type!(ResourceKeyArgument, |a| ArgumentTypeTemplate::ResourceKey(
    a.registry_key.clone()
));
crate::register_argument_type!(ResourceOrTagArgument, |a| {
    ArgumentTypeTemplate::ResourceOrTag(a.registry_key.clone())
});
crate::register_argument_type!(ResourceOrTagKeyArgument, |a| {
    ArgumentTypeTemplate::ResourceOrTagKey(a.registry_key.clone())
});
crate::register_argument_type!(ResourceSelectorArgument, |a| {
    ArgumentTypeTemplate::ResourceSelector(a.registry_key.clone())
});
