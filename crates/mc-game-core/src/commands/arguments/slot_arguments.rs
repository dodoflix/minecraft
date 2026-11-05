//! `SlotRanges`, `SlotArgument`, `SlotsArgument`, `SlotSourceArgument`.

use super::resource_arguments::{ResourceOrId, ResourceOrIdParser};
use crate::commands::message::dynamic_translatable;
use crate::commands::shared_suggestion_provider::suggest;
use crate::commands::{CommandBuildContext, CommandSourceStack};
use mc_brigadier::arguments::value;
use mc_brigadier::{
    ArgumentType, CommandContext, CommandSyntaxException, DynamicCommandExceptionType, ImmutableStringReader, ParsedValue, StringReader,
    Suggestions, SuggestionsBuilder,
};
use mc_protocol::packets::play::ArgumentTypeTemplate;
use std::sync::{Arc, LazyLock};

/// `SlotRange`: a named list of slot ids.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct SlotRange {
    name: String,
    slots: Vec<i32>,
}

impl SlotRange {
    pub fn slots(&self) -> &[i32] {
        &self.slots
    }

    pub fn size(&self) -> usize {
        self.slots.len()
    }

    pub fn get_serialized_name(&self) -> &str {
        &self.name
    }
}

static ERROR_UNKNOWN: LazyLock<DynamicCommandExceptionType<String>> = LazyLock::new(|| dynamic_translatable("slot.unknown"));
static ERROR_ONLY_SINGLE_SLOT_ALLOWED: LazyLock<DynamicCommandExceptionType<String>> = LazyLock::new(|| dynamic_translatable("slot.only_single_allowed"));

/// `SlotRanges.SLOTS` in registration order.
static SLOTS: LazyLock<Vec<SlotRange>> = LazyLock::new(|| {
    let mut v: Vec<SlotRange> = Vec::new();
    let single = |v: &mut Vec<SlotRange>, name: &str, id: i32| v.push(SlotRange { name: name.to_owned(), slots: vec![id] });
    let range = |v: &mut Vec<SlotRange>, prefix: &str, offset: i32, size: i32| {
        let mut all = Vec::new();
        for i in 0..size {
            v.push(SlotRange { name: format!("{prefix}{i}"), slots: vec![offset + i] });
            all.push(offset + i);
        }
        v.push(SlotRange { name: format!("{prefix}*"), slots: all });
    };
    let many = |v: &mut Vec<SlotRange>, name: &str, ids: &[i32]| v.push(SlotRange { name: name.to_owned(), slots: ids.to_vec() });
    single(&mut v, "contents", 0);
    range(&mut v, "container.", 0, 54);
    range(&mut v, "hotbar.", 0, 9);
    range(&mut v, "inventory.", 9, 27);
    range(&mut v, "enderchest.", 200, 27);
    range(&mut v, "mob.inventory.", 300, 8);
    range(&mut v, "horse.", 500, 15);
    // `EquipmentSlot.getIndex(base)` = base + index (MAINHAND 0, OFFHAND 1, FEET 0, LEGS 1, CHEST 2, HEAD 3, BODY 0, SADDLE 0).
    let (mainhand, offhand) = (98, 99);
    single(&mut v, "weapon", mainhand);
    single(&mut v, "weapon.mainhand", mainhand);
    single(&mut v, "weapon.offhand", offhand);
    many(&mut v, "weapon.*", &[mainhand, offhand]);
    let (head, chest, legs, feet, body) = (103, 102, 101, 100, 105);
    single(&mut v, "armor.head", head);
    single(&mut v, "armor.chest", chest);
    single(&mut v, "armor.legs", legs);
    single(&mut v, "armor.feet", feet);
    single(&mut v, "armor.body", body);
    many(&mut v, "armor.*", &[head, chest, legs, feet, body]);
    single(&mut v, "saddle", 106);
    single(&mut v, "horse.chest", 499);
    single(&mut v, "player.cursor", 499);
    range(&mut v, "player.crafting.", 500, 4);
    v
});

pub struct SlotRanges;

impl SlotRanges {
    pub const MOB_INVENTORY_SLOT_OFFSET: i32 = 300;
    pub const MOB_INVENTORY_SIZE: i32 = 8;

    pub fn name_to_ids(name: &str) -> Option<&'static SlotRange> {
        SLOTS.iter().find(|s| s.name == name)
    }

    pub fn all_names() -> impl Iterator<Item = &'static str> {
        SLOTS.iter().map(|s| s.name.as_str())
    }

    pub fn single_slot_names() -> impl Iterator<Item = &'static str> {
        SLOTS.iter().filter(|s| s.size() == 1).map(|s| s.name.as_str())
    }

    /// `ParserUtils.readWhile(reader, c -> c != ' ')`.
    fn read_name(reader: &mut StringReader) -> String {
        let start = reader.get_cursor();
        while reader.can_read() && reader.peek() != ' ' {
            reader.skip();
        }
        mc_brigadier::utf16::substring(reader.get_string(), start, reader.get_cursor())
    }

    pub fn try_read(reader: &mut StringReader) -> Option<&'static SlotRange> {
        Self::name_to_ids(&Self::read_name(reader))
    }

    pub fn read(reader: &mut StringReader) -> Result<&'static SlotRange, CommandSyntaxException> {
        let name = Self::read_name(reader);
        Self::name_to_ids(&name).ok_or_else(|| ERROR_UNKNOWN.create_with_context(reader, name))
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct SlotArgument;

impl SlotArgument {
    pub fn slot() -> Self {
        Self
    }

    pub fn get_slot(context: &CommandContext<CommandSourceStack>, name: &str) -> i32 {
        *context.get_argument::<i32>(name)
    }
}

impl ArgumentType<CommandSourceStack> for SlotArgument {
    fn parse(&self, reader: &mut StringReader) -> Result<ParsedValue, CommandSyntaxException> {
        let result = SlotRanges::read(reader)?;
        if result.size() != 1 {
            return Err(ERROR_ONLY_SINGLE_SLOT_ALLOWED.create_with_context(reader, result.get_serialized_name().to_owned()));
        }
        Ok(value(result.slots()[0]))
    }

    fn list_suggestions(&self, _context: &CommandContext<CommandSourceStack>, builder: SuggestionsBuilder) -> Suggestions {
        suggest(SlotRanges::single_slot_names(), builder)
    }

    fn get_examples(&self) -> Vec<String> {
        ["container.5", "weapon"].map(String::from).to_vec()
    }

    fn describe(&self) -> String {
        "SlotArgument".to_owned()
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct SlotsArgument;

impl SlotsArgument {
    pub const EXAMPLES: [&'static str; 3] = ["container.*", "container.5", "weapon"];

    pub fn slots() -> Self {
        Self
    }

    pub fn get_slots(context: &CommandContext<CommandSourceStack>, name: &str) -> SlotRange {
        context.get_argument::<SlotRange>(name).clone()
    }
}

impl ArgumentType<CommandSourceStack> for SlotsArgument {
    fn parse(&self, reader: &mut StringReader) -> Result<ParsedValue, CommandSyntaxException> {
        SlotRanges::read(reader).map(|r| value(r.clone()))
    }

    fn list_suggestions(&self, _context: &CommandContext<CommandSourceStack>, builder: SuggestionsBuilder) -> Suggestions {
        suggest(SlotRanges::all_names(), builder)
    }

    fn get_examples(&self) -> Vec<String> {
        Self::EXAMPLES.map(String::from).to_vec()
    }

    fn describe(&self) -> String {
        "SlotsArgument".to_owned()
    }
}

/// `SlotSourceArgument.Result`.
#[derive(Clone, PartialEq, Debug)]
pub enum SlotSourceResult {
    /// `LiteralResult`.
    Literal(SlotRange),
    /// `HolderResult`: a `slot_source` registry element or an inline definition.
    Holder(ResourceOrId),
}

impl SlotSourceResult {
    /// `name()`.
    pub fn name(&self) -> Option<String> {
        match self {
            SlotSourceResult::Literal(range) => Some(range.get_serialized_name().to_owned()),
            SlotSourceResult::Holder(ResourceOrId::Reference(key)) => Some(key.id.to_string()),
            SlotSourceResult::Holder(ResourceOrId::Inline(_)) => None,
        }
    }
}

#[derive(Clone)]
pub struct SlotSourceArgument {
    holder_argument: Arc<ResourceOrIdParser>,
}

impl SlotSourceArgument {
    pub fn slot_source(context: &CommandBuildContext) -> Self {
        Self { holder_argument: Arc::new(ResourceOrIdParser::new(context, "slot_source")) }
    }

    pub fn get_slot_source(context: &CommandContext<CommandSourceStack>, name: &str) -> SlotSourceResult {
        context.get_argument::<SlotSourceResult>(name).clone()
    }
}

impl ArgumentType<CommandSourceStack> for SlotSourceArgument {
    fn parse(&self, reader: &mut StringReader) -> Result<ParsedValue, CommandSyntaxException> {
        let start = reader.get_cursor();
        if let Some(range) = SlotRanges::try_read(reader) {
            return Ok(value(SlotSourceResult::Literal(range.clone())));
        }
        reader.set_cursor(start);
        self.holder_argument.parse(reader).map(|h| value(SlotSourceResult::Holder(h)))
    }

    fn list_suggestions(&self, context: &CommandContext<CommandSourceStack>, mut builder: SuggestionsBuilder) -> Suggestions {
        let sub = suggest(SlotRanges::all_names(), builder.restart());
        for s in sub.get_list() {
            builder.suggest(s.get_text());
        }
        let ids = context.get_source().suggest_registry_elements(
            "slot_source",
            crate::commands::shared_suggestion_provider::ElementSuggestionType::Elements,
            builder,
            None,
        );
        ids
    }

    fn get_examples(&self) -> Vec<String> {
        ["foo", "foo:bar", "012", "{}", "true", "container.*", "container.5", "weapon"].map(String::from).to_vec()
    }

    fn describe(&self) -> String {
        "SlotSourceArgument".to_owned()
    }
}

crate::register_argument_type!(SlotArgument, |_a| ArgumentTypeTemplate::ItemSlot);
crate::register_argument_type!(SlotsArgument, |_a| ArgumentTypeTemplate::ItemSlots);
crate::register_argument_type!(SlotSourceArgument, |_a| ArgumentTypeTemplate::SlotSource);
