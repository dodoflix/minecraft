//! `ItemParser`: `item[component=value,!removed]`.

use super::item_input::ItemInput;
use crate::commands::arguments::nbt_arguments::parse_tag_as_argument;
use crate::commands::arguments::read_identifier;
use crate::commands::message::{dynamic_translatable, message, simple_translatable};
use crate::commands::shared_suggestion_provider::{filter_resources, suggest_resource};
use crate::commands::{CommandBuildContext, RegistryView};
use crate::translatable;
use mc_brigadier::{
    CommandSyntaxException, Dynamic2CommandExceptionType, DynamicCommandExceptionType,
    ImmutableStringReader, SimpleCommandExceptionType, StringReader, Suggestions,
    SuggestionsBuilder,
};
use mc_core::ident::Identifier;
use mc_data::{DataComponentType, Item};
use mc_protocol::components::DataComponentPatch;
use mc_protocol::components::registry::try_def;
use std::sync::{Arc, LazyLock};

pub(super) static ERROR_UNKNOWN_ITEM: LazyLock<DynamicCommandExceptionType<String>> =
    LazyLock::new(|| dynamic_translatable("argument.item.id.invalid"));
pub(super) static ERROR_UNKNOWN_COMPONENT: LazyLock<DynamicCommandExceptionType<String>> =
    LazyLock::new(|| dynamic_translatable("arguments.item.component.unknown"));
pub(super) static ERROR_MALFORMED_COMPONENT: LazyLock<
    Dynamic2CommandExceptionType<String, String>,
> = LazyLock::new(|| {
    Dynamic2CommandExceptionType::new(|t: String, m: String| {
        message(translatable!("arguments.item.component.malformed", t, m))
    })
});
static ERROR_EXPECTED_COMPONENT: LazyLock<SimpleCommandExceptionType> =
    LazyLock::new(|| simple_translatable("arguments.item.component.expected"));
static ERROR_REPEATED_COMPONENT: LazyLock<DynamicCommandExceptionType<String>> =
    LazyLock::new(|| dynamic_translatable("arguments.item.component.repeated"));

#[derive(Clone)]
enum Suggest {
    Nothing,
    Item,
    StartComponents,
    ComponentAssignmentOrRemoval,
    Component,
    Assignment,
    NextOrEndComponents,
}

#[derive(Clone)]
pub struct ItemParser {
    items: Arc<dyn RegistryView>,
}

/// What a parse produced.
pub trait Visitor {
    fn visit_item(&mut self, _item: Item) {}
    fn visit_component(
        &mut self,
        _type: DataComponentType,
        _value: mc_protocol::components::Value,
    ) {
    }
    fn visit_removed_component(&mut self, _type: DataComponentType) {}
    fn visit_suggestions(&mut self, _suggestions: &'static str) {}
}

struct State<'a, V> {
    parser: &'a ItemParser,
    reader: &'a mut StringReader,
    visitor: &'a mut V,
    suggestions: Suggest,
}

impl ItemParser {
    pub fn new(context: &CommandBuildContext) -> Self {
        Self {
            items: context.lookup("minecraft:item").expect("item registry"),
        }
    }

    /// `parse(reader)`.
    pub fn parse(&self, reader: &mut StringReader) -> Result<ItemInput, CommandSyntaxException> {
        struct Collect {
            item: Option<Item>,
            patch: Vec<(DataComponentType, Option<mc_protocol::components::Value>)>,
        }
        let mut collect = Collect {
            item: None,
            patch: Vec::new(),
        };
        let cursor = reader.get_cursor();
        let mut state = State {
            parser: self,
            reader,
            visitor: &mut NullVisitor,
            suggestions: Suggest::Nothing,
        };
        let result = state.parse_collect(&mut |event| match event {
            Event::Item(i) => collect.item = Some(i),
            Event::Component(t, v) => {
                collect.patch.retain(|(x, _)| *x != t);
                collect.patch.push((t, Some(v)));
            }
            Event::Removed(t) => {
                collect.patch.retain(|(x, _)| *x != t);
                collect.patch.push((t, None));
            }
        });
        if let Err(e) = result {
            reader.set_cursor(cursor);
            return Err(e);
        }
        let item = collect.item.expect("Parser gave no item");
        let mut builder = DataComponentPatch::builder();
        for (t, v) in collect.patch {
            builder = match v {
                Some(v) => builder.set_value(t, v),
                None => builder.remove(t),
            };
        }
        Ok(ItemInput {
            item,
            components: builder.build(),
        })
    }

    /// `fillSuggestions(builder)`.
    pub fn fill_suggestions(&self, builder: SuggestionsBuilder) -> Suggestions {
        let mut reader = StringReader::new(builder.get_input());
        reader.set_cursor(builder.get_start());
        let mut state = State {
            parser: self,
            reader: &mut reader,
            visitor: &mut NullVisitor,
            suggestions: Suggest::Nothing,
        };
        let _ = state.parse_collect(&mut |_| {});
        let suggest = state.suggestions.clone();
        let cursor = reader.get_cursor();
        self.resolve(&suggest, builder.create_offset(cursor))
    }

    fn resolve(&self, suggest: &Suggest, mut builder: SuggestionsBuilder) -> Suggestions {
        match suggest {
            Suggest::Nothing => builder.build(),
            Suggest::Item => suggest_resource(self.items.element_ids(), builder),
            Suggest::StartComponents => {
                if builder.get_remaining().is_empty() {
                    builder.suggest("[");
                }
                builder.build()
            }
            Suggest::NextOrEndComponents => {
                if builder.get_remaining().is_empty() {
                    builder.suggest(",");
                    builder.suggest("]");
                }
                builder.build()
            }
            Suggest::Assignment => {
                if builder.get_remaining().is_empty() {
                    builder.suggest("=");
                }
                builder.build()
            }
            Suggest::ComponentAssignmentOrRemoval => {
                builder.suggest("!");
                self.suggest_component(builder, "=")
            }
            Suggest::Component => self.suggest_component(builder, ""),
        }
    }

    fn suggest_component(&self, mut builder: SuggestionsBuilder, suffix: &str) -> Suggestions {
        let contents = builder.get_remaining().to_lowercase();
        let all: Vec<Identifier> = DataComponentType::iter()
            .filter(|t| try_def(*t).is_some_and(|d| !d.is_transient()))
            .map(|t| t.identifier())
            .collect();
        filter_resources(
            all,
            &contents,
            |id| id.clone(),
            |id| {
                builder.suggest(format!("{id}{suffix}"));
            },
        );
        builder.build()
    }
}

enum Event {
    Item(Item),
    Component(DataComponentType, mc_protocol::components::Value),
    Removed(DataComponentType),
}

struct NullVisitor;

impl Visitor for NullVisitor {}

impl<V> State<'_, V> {
    fn parse_collect(&mut self, sink: &mut dyn FnMut(Event)) -> Result<(), CommandSyntaxException> {
        self.suggestions = Suggest::Item;
        self.read_item(sink)?;
        self.suggestions = Suggest::StartComponents;
        if self.reader.can_read() && self.reader.peek() == '[' {
            self.suggestions = Suggest::Nothing;
            self.read_components(sink)?;
        }
        Ok(())
    }

    fn read_item(&mut self, sink: &mut dyn FnMut(Event)) -> Result<(), CommandSyntaxException> {
        let cursor = self.reader.get_cursor();
        let id = read_identifier(self.reader)?;
        let item = if self.parser.items.contains(&id) {
            Item::by_name(&id.to_string())
        } else {
            None
        };
        match item {
            Some(item) => {
                sink(Event::Item(item));
                Ok(())
            }
            None => {
                self.reader.set_cursor(cursor);
                Err(ERROR_UNKNOWN_ITEM.create_with_context(self.reader, id.to_string()))
            }
        }
    }

    fn read_components(
        &mut self,
        sink: &mut dyn FnMut(Event),
    ) -> Result<(), CommandSyntaxException> {
        self.reader.expect('[')?;
        self.suggestions = Suggest::ComponentAssignmentOrRemoval;
        let mut visited: Vec<DataComponentType> = Vec::new();
        while self.reader.can_read() && self.reader.peek() != ']' {
            self.reader.skip_whitespace();
            if self.reader.can_read() && self.reader.peek() == '!' {
                self.reader.skip();
                self.suggestions = Suggest::Component;
                let component_type = read_component_type(self.reader)?;
                if visited.contains(&component_type) {
                    return Err(ERROR_REPEATED_COMPONENT.create(component_type.name().to_owned()));
                }
                visited.push(component_type);
                sink(Event::Removed(component_type));
                self.suggestions = Suggest::Nothing;
                self.reader.skip_whitespace();
            } else {
                let component_type = read_component_type(self.reader)?;
                if visited.contains(&component_type) {
                    return Err(ERROR_REPEATED_COMPONENT.create(component_type.name().to_owned()));
                }
                visited.push(component_type);
                self.suggestions = Suggest::Assignment;
                self.reader.skip_whitespace();
                self.reader.expect('=')?;
                self.suggestions = Suggest::Nothing;
                self.reader.skip_whitespace();
                self.read_component(component_type, sink)?;
                self.reader.skip_whitespace();
            }
            self.suggestions = Suggest::NextOrEndComponents;
            if !self.reader.can_read() || self.reader.peek() != ',' {
                break;
            }
            self.reader.skip();
            self.reader.skip_whitespace();
            self.suggestions = Suggest::ComponentAssignmentOrRemoval;
            if !self.reader.can_read() {
                return Err(ERROR_EXPECTED_COMPONENT.create_with_context(self.reader));
            }
        }
        self.reader.expect(']')?;
        self.suggestions = Suggest::Nothing;
        Ok(())
    }

    fn read_component(
        &mut self,
        component_type: DataComponentType,
        sink: &mut dyn FnMut(Event),
    ) -> Result<(), CommandSyntaxException> {
        let cursor = self.reader.get_cursor();
        let tag = parse_tag_as_argument(self.reader)?;
        let malformed = |reader: &mut StringReader, msg: String| {
            reader.set_cursor(cursor);
            ERROR_MALFORMED_COMPONENT.create_with_context(
                reader,
                component_type.name().to_owned(),
                msg,
            )
        };
        let Some(def) = try_def(component_type) else {
            return Err(malformed(
                self.reader,
                format!("Component {} is not implemented", component_type.name()),
            ));
        };
        match mc_dfu::parse_nbt(def.codec_or_throw(), &tag).into_result() {
            Ok(value) => {
                sink(Event::Component(component_type, value));
                Ok(())
            }
            Err(msg) => Err(malformed(self.reader, msg)),
        }
    }
}

/// `ItemParser.State.readComponentType(reader)`.
pub fn read_component_type(
    reader: &mut StringReader,
) -> Result<DataComponentType, CommandSyntaxException> {
    if !reader.can_read() {
        return Err(ERROR_EXPECTED_COMPONENT.create_with_context(reader));
    }
    let cursor = reader.get_cursor();
    let id = read_identifier(reader)?;
    match DataComponentType::by_name(&id.to_string()) {
        Some(t) if !try_def(t).is_some_and(|d| d.is_transient()) => Ok(t),
        _ => {
            reader.set_cursor(cursor);
            Err(ERROR_UNKNOWN_COMPONENT.create_with_context(reader, id.to_string()))
        }
    }
}
