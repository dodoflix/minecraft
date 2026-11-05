//! `ItemPredicateArgument`: `stick`, `#tag`, `*`, `stick[count=(min..max)|!damage,custom_data~{..}]`.

use super::component_predicate_parser::{parse_for_commands, parse_for_suggestions, Context};
use super::item_parser::{ERROR_MALFORMED_COMPONENT, ERROR_UNKNOWN_COMPONENT, ERROR_UNKNOWN_ITEM};
use crate::commands::message::{dynamic_translatable, message};
use crate::commands::{CommandBuildContext, CommandSourceStack, RegistryView};
use crate::translatable;
use mc_brigadier::arguments::value;
use mc_brigadier::{
    ArgumentType, CommandContext, CommandSyntaxException, Dynamic2CommandExceptionType, DynamicCommandExceptionType, ImmutableStringReader,
    ParsedValue, StringReader, Suggestions, SuggestionsBuilder,
};
use mc_core::ident::Identifier;
use mc_data::{DataComponentPredicateType, DataComponentType, Item};
use mc_nbt::Tag;
use mc_protocol::components::registry::try_def;
use mc_protocol::components::values::predicates::{Ints, PredicateType};
use mc_protocol::components::ItemStack;
use mc_protocol::packets::play::ArgumentTypeTemplate;
use std::sync::{Arc, LazyLock};

static ERROR_UNKNOWN_TAG: LazyLock<DynamicCommandExceptionType<String>> = LazyLock::new(|| dynamic_translatable("arguments.item.tag.unknown"));
static ERROR_UNKNOWN_PREDICATE: LazyLock<DynamicCommandExceptionType<String>> =
    LazyLock::new(|| dynamic_translatable("arguments.item.predicate.unknown"));
static ERROR_MALFORMED_PREDICATE: LazyLock<Dynamic2CommandExceptionType<String, String>> = LazyLock::new(|| {
    Dynamic2CommandExceptionType::new(|t: String, m: String| message(translatable!("arguments.item.predicate.malformed", t, m)))
});

const COUNT_ID: &str = "minecraft:count";

type Pred = Arc<dyn Fn(&ItemStack) -> bool + Send + Sync>;

/// `ItemPredicateArgument.Result`: `Util.allOf(predicates)`.
#[derive(Clone)]
pub struct ItemPredicateResult(Pred);

impl ItemPredicateResult {
    pub fn test(&self, stack: &ItemStack) -> bool {
        (self.0)(stack)
    }
}

/// `ItemPredicateArgument.ComponentWrapper`: the `count` pseudo component or a persistent component type.
#[derive(Clone)]
enum ComponentWrapper {
    Count,
    Type(DataComponentType),
}

/// `ItemPredicateArgument.PredicateWrapper`.
#[derive(Clone)]
enum PredicateWrapper {
    Count,
    Type(DataComponentPredicateType),
    /// `createComponentExistencePredicate`.
    Existence(DataComponentType),
}

fn count_predicate(tag: &Tag) -> Result<Pred, String> {
    let range = mc_dfu::parse_nbt(&Ints::codec(), tag).into_result()?;
    Ok(Arc::new(move |s: &ItemStack| range.matches(s.count())))
}

impl ComponentWrapper {
    fn id(&self) -> String {
        match self {
            ComponentWrapper::Count => COUNT_ID.to_owned(),
            ComponentWrapper::Type(t) => t.name().to_owned(),
        }
    }

    /// `presenceChecker`.
    fn presence(&self) -> Pred {
        match *self {
            ComponentWrapper::Count => Arc::new(|_| true),
            ComponentWrapper::Type(t) => Arc::new(move |s: &ItemStack| s.has(t)),
        }
    }

    /// `decode(reader, value)`.
    fn decode(&self, reader: &dyn ImmutableStringReader, tag: &Tag) -> Result<Pred, CommandSyntaxException> {
        let result = match *self {
            ComponentWrapper::Count => count_predicate(tag),
            ComponentWrapper::Type(t) => {
                let def = try_def(t).expect("checked at lookup");
                mc_dfu::parse_nbt(def.codec_or_throw(), tag).into_result().map(|expected| -> Pred {
                    Arc::new(move |s: &ItemStack| s.get_value(t) == Some(&expected))
                })
            }
        };
        result.map_err(|msg| ERROR_MALFORMED_COMPONENT.create_with_context(reader, self.id(), msg))
    }
}

impl PredicateWrapper {
    fn id(&self) -> String {
        match self {
            PredicateWrapper::Count => COUNT_ID.to_owned(),
            PredicateWrapper::Type(t) => t.name().to_owned(),
            PredicateWrapper::Existence(t) => t.name().to_owned(),
        }
    }

    fn decode(&self, reader: &dyn ImmutableStringReader, tag: &Tag) -> Result<Pred, CommandSyntaxException> {
        let result = match *self {
            PredicateWrapper::Count => count_predicate(tag),
            PredicateWrapper::Type(t) => mc_dfu::parse_nbt(&PredicateType::Concrete(t).value_codec(), tag)
                .into_result()
                .map(|p| -> Pred { Arc::new(move |s: &ItemStack| p.matches(s)) }),
            PredicateWrapper::Existence(t) => {
                mc_dfu::parse_nbt(&mc_dfu::Codec::unit(()), tag).into_result().map(|()| -> Pred { Arc::new(move |s: &ItemStack| s.has(t)) })
            }
        };
        result.map_err(|msg| ERROR_MALFORMED_PREDICATE.create_with_context(reader, self.id(), msg))
    }
}

/// `ItemPredicateArgument.Context`.
struct ItemContext {
    items: Arc<dyn RegistryView>,
}

fn pseudo_id() -> Identifier {
    Identifier::parse(COUNT_ID).expect("valid id")
}

impl Context for ItemContext {
    type T = Pred;
    type C = ComponentWrapper;
    type P = PredicateWrapper;

    fn for_element_type(&self, reader: &dyn ImmutableStringReader, id: &Identifier) -> Result<Pred, CommandSyntaxException> {
        let item = if self.items.contains(id) { Item::by_name(&id.to_string()) } else { None };
        let item = item.ok_or_else(|| ERROR_UNKNOWN_ITEM.create_with_context(reader, id.to_string()))?;
        Ok(Arc::new(move |s: &ItemStack| s.is(item)))
    }

    fn list_element_types(&self) -> Vec<Identifier> {
        self.items.element_ids()
    }

    fn for_tag_type(&self, reader: &dyn ImmutableStringReader, id: &Identifier) -> Result<Pred, CommandSyntaxException> {
        let elements = self.items.tag_elements(id).ok_or_else(|| ERROR_UNKNOWN_TAG.create_with_context(reader, id.to_string()))?;
        let items: Vec<Item> = elements.iter().filter_map(|e| Item::by_name(&e.to_string())).collect();
        Ok(Arc::new(move |s: &ItemStack| items.contains(&s.item())))
    }

    fn list_tag_types(&self) -> Vec<Identifier> {
        self.items.tag_ids()
    }

    fn lookup_component_type(&self, reader: &dyn ImmutableStringReader, id: &Identifier) -> Result<ComponentWrapper, CommandSyntaxException> {
        if *id == pseudo_id() {
            return Ok(ComponentWrapper::Count);
        }
        let unknown = || ERROR_UNKNOWN_COMPONENT.create_with_context(reader, id.to_string());
        let t = DataComponentType::by_name(&id.to_string()).ok_or_else(unknown)?;
        // `ComponentWrapper.create`: a component without codec is unknown
        if try_def(t).is_none_or(|d| d.is_transient()) {
            return Err(unknown());
        }
        Ok(ComponentWrapper::Type(t))
    }

    fn list_component_types(&self) -> Vec<Identifier> {
        std::iter::once(pseudo_id())
            .chain(DataComponentType::iter().filter(|t| try_def(*t).is_some_and(|d| !d.is_transient())).map(|t| t.identifier()))
            .collect()
    }

    fn create_component_test(&self, reader: &dyn ImmutableStringReader, component_type: &ComponentWrapper, value: &Tag) -> Result<Pred, CommandSyntaxException> {
        component_type.decode(reader, value)
    }

    fn create_component_presence_test(&self, _reader: &dyn ImmutableStringReader, component_type: &ComponentWrapper) -> Pred {
        component_type.presence()
    }

    fn lookup_predicate_type(&self, reader: &dyn ImmutableStringReader, id: &Identifier) -> Result<PredicateWrapper, CommandSyntaxException> {
        if *id == pseudo_id() {
            return Ok(PredicateWrapper::Count);
        }
        let name = id.to_string();
        if let Some(t) = DataComponentPredicateType::by_name(&name) {
            return Ok(PredicateWrapper::Type(t));
        }
        if let Some(t) = DataComponentType::by_name(&name) {
            return Ok(PredicateWrapper::Existence(t));
        }
        Err(ERROR_UNKNOWN_PREDICATE.create_with_context(reader, name))
    }

    fn list_predicate_types(&self) -> Vec<Identifier> {
        std::iter::once(pseudo_id()).chain(DataComponentPredicateType::iter().map(|t| t.identifier())).collect()
    }

    fn create_predicate_test(&self, reader: &dyn ImmutableStringReader, predicate_type: &PredicateWrapper, value: &Tag) -> Result<Pred, CommandSyntaxException> {
        predicate_type.decode(reader, value)
    }

    fn negate(&self, value: Pred) -> Pred {
        Arc::new(move |s: &ItemStack| !value(s))
    }

    fn any_of(&self, alternatives: Vec<Pred>) -> Pred {
        Arc::new(move |s: &ItemStack| alternatives.iter().any(|p| p(s)))
    }
}

/// `ItemPredicateArgument`.
#[derive(Clone)]
pub struct ItemPredicateArgument {
    items: Arc<dyn RegistryView>,
}

impl ItemPredicateArgument {
    pub fn item_predicate(context: &CommandBuildContext) -> Self {
        Self { items: context.lookup("minecraft:item").expect("item registry") }
    }

    pub fn get_item_predicate(context: &CommandContext<CommandSourceStack>, name: &str) -> ItemPredicateResult {
        context.get_argument::<ItemPredicateResult>(name).clone()
    }

    fn context(&self) -> ItemContext {
        ItemContext { items: self.items.clone() }
    }
}

impl ArgumentType<CommandSourceStack> for ItemPredicateArgument {
    fn parse(&self, reader: &mut StringReader) -> Result<ParsedValue, CommandSyntaxException> {
        let predicates = parse_for_commands(&self.context(), reader)?;
        // `Util.allOf(predicates)::test`
        let all: Pred = Arc::new(move |s: &ItemStack| predicates.iter().all(|p| p(s)));
        Ok(value(ItemPredicateResult(all)))
    }

    fn list_suggestions(&self, _context: &CommandContext<CommandSourceStack>, builder: SuggestionsBuilder) -> Suggestions {
        parse_for_suggestions(&self.context(), builder)
    }

    fn get_examples(&self) -> Vec<String> {
        ["stick", "minecraft:stick", "#stick", "#stick{foo:'bar'}"].map(String::from).to_vec()
    }

    fn describe(&self) -> String {
        "ItemPredicateArgument".to_owned()
    }
}

crate::register_argument_type!(ItemPredicateArgument, |_a| ArgumentTypeTemplate::ItemPredicate);
