//! `BlockStateParser`: `stone[facing=north]{nbt}` / `#tag[prop=value]{nbt}`.

use crate::commands::RegistryView;
use crate::commands::arguments::nbt_arguments::parse_compound_as_argument;
use crate::commands::arguments::read_identifier;
use crate::commands::message::message;
use crate::commands::shared_suggestion_provider::{
    suggest_resource, suggest_resource_into, suggest_resource_prefixed,
    suggest_resource_prefixed_into,
};
use crate::translatable;
use mc_brigadier::{
    CommandSyntaxException, Dynamic2CommandExceptionType, Dynamic3CommandExceptionType,
    DynamicCommandExceptionType, ImmutableStringReader, SimpleCommandExceptionType, StringReader,
    Suggestions, SuggestionsBuilder,
};
use mc_core::ident::Identifier;
use mc_data::block::{Block, BlockState};
use mc_data::props::PropertyDef;
use mc_nbt::CompoundTag;
use std::collections::BTreeMap;
use std::sync::LazyLock;

pub static ERROR_NO_TAGS_ALLOWED: LazyLock<SimpleCommandExceptionType> = LazyLock::new(|| {
    SimpleCommandExceptionType::new(message(translatable!("argument.block.tag.disallowed")))
});
pub static ERROR_UNKNOWN_BLOCK: LazyLock<DynamicCommandExceptionType<String>> =
    LazyLock::new(|| {
        DynamicCommandExceptionType::new(|id: String| {
            message(translatable!("argument.block.id.invalid", id))
        })
    });
pub static ERROR_UNKNOWN_PROPERTY: LazyLock<Dynamic2CommandExceptionType<String, String>> =
    LazyLock::new(|| {
        Dynamic2CommandExceptionType::new(|block: String, property: String| {
            message(translatable!(
                "argument.block.property.unknown",
                block,
                property
            ))
        })
    });
pub static ERROR_DUPLICATE_PROPERTY: LazyLock<Dynamic2CommandExceptionType<String, String>> =
    LazyLock::new(|| {
        Dynamic2CommandExceptionType::new(|block: String, property: String| {
            message(translatable!(
                "argument.block.property.duplicate",
                property,
                block
            ))
        })
    });
pub static ERROR_INVALID_VALUE: LazyLock<Dynamic3CommandExceptionType<String, String, String>> =
    LazyLock::new(|| {
        Dynamic3CommandExceptionType::new(|block: String, property: String, value: String| {
            message(translatable!(
                "argument.block.property.invalid",
                block,
                value,
                property
            ))
        })
    });
pub static ERROR_EXPECTED_VALUE: LazyLock<Dynamic2CommandExceptionType<String, String>> =
    LazyLock::new(|| {
        Dynamic2CommandExceptionType::new(|block: String, property: String| {
            message(translatable!(
                "argument.block.property.novalue",
                property,
                block
            ))
        })
    });
pub static ERROR_EXPECTED_END_OF_PROPERTIES: LazyLock<SimpleCommandExceptionType> =
    LazyLock::new(|| {
        SimpleCommandExceptionType::new(message(translatable!("argument.block.property.unclosed")))
    });
pub static ERROR_UNKNOWN_TAG: LazyLock<DynamicCommandExceptionType<String>> = LazyLock::new(|| {
    DynamicCommandExceptionType::new(|tag: String| {
        message(translatable!("arguments.block.tag.unknown", tag))
    })
});

/// A property value: index into `PropertyDef::values`.
pub type PropertyValue = (&'static PropertyDef, usize);

/// `BlockStateParser.BlockResult`.
#[derive(Clone, Debug)]
pub struct BlockResult {
    pub block_state: BlockState,
    /// The properties given explicitly, in the order they were written.
    pub properties: Vec<PropertyValue>,
    pub nbt: Option<CompoundTag>,
}

/// `BlockStateParser.TagResult`.
#[derive(Clone, Debug)]
pub struct TagResult {
    pub tag: Identifier,
    pub tag_blocks: Vec<Block>,
    pub vague_properties: BTreeMap<String, String>,
    pub nbt: Option<CompoundTag>,
}

/// What `this.suggestions` points to.
#[derive(Clone, Debug)]
enum Suggest {
    Nothing,
    BlockIdOrTag,
    Item,
    Tag,
    OpenPropertiesOrNbt,
    OpenVaguePropertiesOrNbt,
    OpenNbt,
    PropertyNameOrEnd,
    VaguePropertyNameOrEnd,
    PropertyName,
    VaguePropertyName,
    Equals,
    PropertyValue(&'static PropertyDef),
    VaguePropertyValue(String),
    NextPropertyOrEnd,
}

pub struct BlockStateParser<'a> {
    blocks: &'a dyn RegistryView,
    reader: &'a mut StringReader,
    for_testing: bool,
    allow_nbt: bool,
    properties: Vec<PropertyValue>,
    vague_properties: BTreeMap<String, String>,
    id: Identifier,
    block: Option<Block>,
    state: Option<BlockState>,
    nbt: Option<CompoundTag>,
    tag: Option<(Identifier, Vec<Block>)>,
    suggestions: Suggest,
}

fn has_block_entity(block: Block) -> bool {
    block
        .data()
        .has(mc_data::block::block_flags::HAS_BLOCK_ENTITY)
}

fn property_def(block: Block, name: &str) -> Option<&'static PropertyDef> {
    block.properties().find(|p| p.name == name)
}

/// `Property.getValue(String)`.
pub fn parse_property_value(property: &'static PropertyDef, raw: &str) -> Option<usize> {
    use mc_data::props::PropertyKind;
    match property.kind {
        PropertyKind::Integer => {
            // `Ints.tryParse`: optional leading '-' or '+', ASCII digits only.
            let digits = raw.strip_prefix(['-', '+']).unwrap_or(raw);
            if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
                return None;
            }
            let v: i32 = raw.parse().ok()?;
            property.value_index(&v.to_string())
        }
        _ => property.value_index(raw),
    }
}

impl<'a> BlockStateParser<'a> {
    fn new(
        blocks: &'a dyn RegistryView,
        reader: &'a mut StringReader,
        for_testing: bool,
        allow_nbt: bool,
    ) -> Self {
        Self {
            blocks,
            reader,
            for_testing,
            allow_nbt,
            properties: Vec::new(),
            vague_properties: BTreeMap::new(),
            id: Identifier::with_default_namespace("")
                .unwrap_or_else(|_| Identifier::parse("minecraft:air").expect("valid")),
            block: None,
            state: None,
            nbt: None,
            tag: None,
            suggestions: Suggest::Nothing,
        }
    }

    /// `parseForBlock`.
    pub fn parse_for_block(
        blocks: &dyn RegistryView,
        reader: &mut StringReader,
        allow_nbt: bool,
    ) -> Result<BlockResult, CommandSyntaxException> {
        let cursor = reader.get_cursor();
        let mut parser = BlockStateParser::new(blocks, reader, false, allow_nbt);
        match parser.parse() {
            Ok(()) => Ok(BlockResult {
                block_state: parser.state.expect("parsed a block"),
                properties: parser.properties,
                nbt: parser.nbt,
            }),
            Err(e) => {
                reader.set_cursor(cursor);
                Err(e)
            }
        }
    }

    /// `parseForTesting`: `Ok(Ok(block))` or `Ok(Err(tag))` (vanilla's `Either`).
    pub fn parse_for_testing(
        blocks: &dyn RegistryView,
        reader: &mut StringReader,
        allow_nbt: bool,
    ) -> Result<Result<BlockResult, TagResult>, CommandSyntaxException> {
        let cursor = reader.get_cursor();
        let mut parser = BlockStateParser::new(blocks, reader, true, allow_nbt);
        match parser.parse() {
            Ok(()) => Ok(match parser.tag {
                Some((tag, tag_blocks)) => Err(TagResult {
                    tag,
                    tag_blocks,
                    vague_properties: parser.vague_properties,
                    nbt: parser.nbt,
                }),
                None => Ok(BlockResult {
                    block_state: parser.state.expect("parsed a block"),
                    properties: parser.properties,
                    nbt: parser.nbt,
                }),
            }),
            Err(e) => {
                reader.set_cursor(cursor);
                Err(e)
            }
        }
    }

    /// `fillSuggestions`.
    pub fn fill_suggestions(
        blocks: &dyn RegistryView,
        builder: SuggestionsBuilder,
        for_testing: bool,
        allow_nbt: bool,
    ) -> Suggestions {
        let mut reader = StringReader::new(builder.get_input());
        reader.set_cursor(builder.get_start());
        let mut parser = BlockStateParser::new(blocks, &mut reader, for_testing, allow_nbt);
        let _ = parser.parse();
        let suggestions = parser.suggestions.clone();
        let cursor = parser.reader.get_cursor();
        parser.apply(&suggestions, builder.create_offset(cursor))
    }

    fn parse(&mut self) -> Result<(), CommandSyntaxException> {
        self.suggestions = if self.for_testing {
            Suggest::BlockIdOrTag
        } else {
            Suggest::Item
        };
        if self.reader.can_read() && self.reader.peek() == '#' {
            self.read_tag()?;
            self.suggestions = Suggest::OpenVaguePropertiesOrNbt;
            if self.reader.can_read() && self.reader.peek() == '[' {
                self.read_vague_properties()?;
                self.suggestions = Suggest::OpenNbt;
            }
        } else {
            self.read_block()?;
            self.suggestions = Suggest::OpenPropertiesOrNbt;
            if self.reader.can_read() && self.reader.peek() == '[' {
                self.read_properties()?;
                self.suggestions = Suggest::OpenNbt;
            }
        }
        if self.allow_nbt && self.reader.can_read() && self.reader.peek() == '{' {
            self.suggestions = Suggest::Nothing;
            self.read_nbt()?;
        }
        Ok(())
    }

    fn tag_blocks(&self) -> &[Block] {
        self.tag.as_ref().map_or(&[], |(_, b)| b.as_slice())
    }

    fn has_block_entity(&self) -> bool {
        if let Some(state) = self.state {
            has_block_entity(state.block())
        } else {
            self.tag_blocks().iter().any(|b| has_block_entity(*b))
        }
    }

    fn apply(&self, suggest: &Suggest, mut builder: SuggestionsBuilder) -> Suggestions {
        match suggest {
            Suggest::Nothing => builder.build(),
            Suggest::BlockIdOrTag => {
                // `suggestTag(builder)` and `suggestItem(builder)` add to the same builder.
                suggest_resource_prefixed_into(self.blocks.tag_ids(), &mut builder, "#");
                suggest_resource_into(self.blocks.element_ids(), &mut builder);
                builder.build()
            }
            Suggest::Item => suggest_resource(self.blocks.element_ids(), builder),
            Suggest::Tag => suggest_resource_prefixed(self.blocks.tag_ids(), builder, "#"),
            Suggest::OpenPropertiesOrNbt => {
                if builder.get_remaining().is_empty() {
                    let block = self.state.expect("block read").block();
                    if block.properties().len() > 0 {
                        builder.suggest("[");
                    }
                    if has_block_entity(block) {
                        builder.suggest("{");
                    }
                }
                builder.build()
            }
            Suggest::OpenVaguePropertiesOrNbt => {
                if builder.get_remaining().is_empty() && self.tag.is_some() {
                    let has_properties = self.tag_blocks().iter().any(|b| b.properties().len() > 0);
                    let has_entity = self.tag_blocks().iter().any(|b| has_block_entity(*b));
                    if has_properties {
                        builder.suggest("[");
                    }
                    if has_entity {
                        builder.suggest("{");
                    }
                }
                builder.build()
            }
            Suggest::OpenNbt => {
                if builder.get_remaining().is_empty() && self.has_block_entity() {
                    builder.suggest("{");
                }
                builder.build()
            }
            Suggest::PropertyNameOrEnd => {
                if builder.get_remaining().is_empty() {
                    builder.suggest("]");
                }
                self.apply(&Suggest::PropertyName, builder)
            }
            Suggest::VaguePropertyNameOrEnd => {
                if builder.get_remaining().is_empty() {
                    builder.suggest("]");
                }
                self.apply(&Suggest::VaguePropertyName, builder)
            }
            Suggest::PropertyName => {
                let prefix = builder.get_remaining().to_lowercase();
                let block = self.state.expect("block read").block();
                for property in block.properties() {
                    if !self.properties.iter().any(|(p, _)| p.name == property.name)
                        && property.name.starts_with(&prefix)
                    {
                        builder.suggest(format!("{}=", property.name));
                    }
                }
                builder.build()
            }
            Suggest::VaguePropertyName => {
                let prefix = builder.get_remaining().to_lowercase();
                for block in self.tag_blocks() {
                    for property in block.properties() {
                        if !self.vague_properties.contains_key(property.name)
                            && property.name.starts_with(&prefix)
                        {
                            builder.suggest(format!("{}=", property.name));
                        }
                    }
                }
                builder.build()
            }
            Suggest::Equals => {
                if builder.get_remaining().is_empty() {
                    builder.suggest("=");
                }
                builder.build()
            }
            Suggest::PropertyValue(property) => {
                add_property_suggestions(&mut builder, property);
                builder.build()
            }
            Suggest::VaguePropertyValue(key) => {
                let mut has_more_properties = false;
                for block in self.tag_blocks() {
                    if let Some(property) = property_def(*block, key) {
                        add_property_suggestions(&mut builder, property);
                    }
                    if !has_more_properties
                        && block
                            .properties()
                            .any(|p| !self.vague_properties.contains_key(p.name))
                    {
                        has_more_properties = true;
                    }
                }
                if has_more_properties {
                    builder.suggest(",");
                }
                builder.suggest("]");
                builder.build()
            }
            Suggest::NextPropertyOrEnd => {
                if builder.get_remaining().is_empty() {
                    builder.suggest("]");
                }
                let block = self.state.expect("block read").block();
                if builder.get_remaining().is_empty()
                    && self.properties.len() < block.properties().len()
                {
                    builder.suggest(",");
                }
                builder.build()
            }
        }
    }

    fn read_block(&mut self) -> Result<(), CommandSyntaxException> {
        let start = self.reader.get_cursor();
        self.id = read_identifier(self.reader)?;
        let block = if self.blocks.contains(&self.id) {
            Block::by_name(&self.id.to_string())
        } else {
            None
        };
        let Some(block) = block else {
            self.reader.set_cursor(start);
            return Err(ERROR_UNKNOWN_BLOCK.create_with_context(self.reader, self.id.to_string()));
        };
        self.block = Some(block);
        self.state = Some(block.default_state());
        Ok(())
    }

    fn read_tag(&mut self) -> Result<(), CommandSyntaxException> {
        if !self.for_testing {
            return Err(ERROR_NO_TAGS_ALLOWED.create_with_context(self.reader));
        }
        let start = self.reader.get_cursor();
        self.reader.expect('#')?;
        self.suggestions = Suggest::Tag;
        let id = read_identifier(self.reader)?;
        match self.blocks.tag_elements(&id) {
            Some(members) => {
                let blocks = members
                    .iter()
                    .filter_map(|m| Block::by_name(&m.to_string()))
                    .collect();
                self.tag = Some((id, blocks));
                Ok(())
            }
            None => {
                self.reader.set_cursor(start);
                Err(ERROR_UNKNOWN_TAG.create_with_context(self.reader, id.to_string()))
            }
        }
    }

    fn read_properties(&mut self) -> Result<(), CommandSyntaxException> {
        let block = self.block.expect("block read");
        self.reader.skip();
        self.suggestions = Suggest::PropertyNameOrEnd;
        self.reader.skip_whitespace();
        while self.reader.can_read() && self.reader.peek() != ']' {
            self.reader.skip_whitespace();
            let key_start = self.reader.get_cursor();
            let key = self.reader.read_string()?;
            let Some(property) = property_def(block, &key) else {
                self.reader.set_cursor(key_start);
                return Err(ERROR_UNKNOWN_PROPERTY.create_with_context(
                    self.reader,
                    self.id.to_string(),
                    key,
                ));
            };
            if self.properties.iter().any(|(p, _)| p.name == property.name) {
                self.reader.set_cursor(key_start);
                return Err(ERROR_DUPLICATE_PROPERTY.create_with_context(
                    self.reader,
                    self.id.to_string(),
                    key,
                ));
            }
            self.reader.skip_whitespace();
            self.suggestions = Suggest::Equals;
            if !self.reader.can_read() || self.reader.peek() != '=' {
                return Err(ERROR_EXPECTED_VALUE.create_with_context(
                    self.reader,
                    self.id.to_string(),
                    key,
                ));
            }
            self.reader.skip();
            self.reader.skip_whitespace();
            self.suggestions = Suggest::PropertyValue(property);
            let start = self.reader.get_cursor();
            let raw = self.reader.read_string()?;
            self.set_value(property, &raw, start)?;
            self.suggestions = Suggest::NextPropertyOrEnd;
            self.reader.skip_whitespace();
            if self.reader.can_read() {
                if self.reader.peek() != ',' {
                    if self.reader.peek() != ']' {
                        return Err(
                            ERROR_EXPECTED_END_OF_PROPERTIES.create_with_context(self.reader)
                        );
                    }
                    break;
                }
                self.reader.skip();
                self.suggestions = Suggest::PropertyName;
            }
        }
        if self.reader.can_read() {
            self.reader.skip();
            Ok(())
        } else {
            Err(ERROR_EXPECTED_END_OF_PROPERTIES.create_with_context(self.reader))
        }
    }

    fn read_vague_properties(&mut self) -> Result<(), CommandSyntaxException> {
        self.reader.skip();
        self.suggestions = Suggest::VaguePropertyNameOrEnd;
        let mut value_start: Option<usize> = None;
        self.reader.skip_whitespace();
        while self.reader.can_read() && self.reader.peek() != ']' {
            self.reader.skip_whitespace();
            let key_start = self.reader.get_cursor();
            let key = self.reader.read_string()?;
            if self.vague_properties.contains_key(&key) {
                self.reader.set_cursor(key_start);
                return Err(ERROR_DUPLICATE_PROPERTY.create_with_context(
                    self.reader,
                    self.id.to_string(),
                    key,
                ));
            }
            self.reader.skip_whitespace();
            if !self.reader.can_read() || self.reader.peek() != '=' {
                self.reader.set_cursor(key_start);
                return Err(ERROR_EXPECTED_VALUE.create_with_context(
                    self.reader,
                    self.id.to_string(),
                    key,
                ));
            }
            self.reader.skip();
            self.reader.skip_whitespace();
            self.suggestions = Suggest::VaguePropertyValue(key.clone());
            value_start = Some(self.reader.get_cursor());
            let value = self.reader.read_string()?;
            self.vague_properties.insert(key, value);
            self.reader.skip_whitespace();
            if self.reader.can_read() {
                value_start = None;
                if self.reader.peek() != ',' {
                    if self.reader.peek() != ']' {
                        return Err(
                            ERROR_EXPECTED_END_OF_PROPERTIES.create_with_context(self.reader)
                        );
                    }
                    break;
                }
                self.reader.skip();
                self.suggestions = Suggest::VaguePropertyName;
            }
        }
        if self.reader.can_read() {
            self.reader.skip();
            Ok(())
        } else {
            if let Some(start) = value_start {
                self.reader.set_cursor(start);
            }
            Err(ERROR_EXPECTED_END_OF_PROPERTIES.create_with_context(self.reader))
        }
    }

    fn read_nbt(&mut self) -> Result<(), CommandSyntaxException> {
        self.nbt = Some(parse_compound_as_argument(self.reader)?);
        Ok(())
    }

    fn set_value(
        &mut self,
        property: &'static PropertyDef,
        raw: &str,
        start: usize,
    ) -> Result<(), CommandSyntaxException> {
        match parse_property_value(property, raw) {
            Some(index) => {
                let state = self.state.expect("block read");
                self.state = Some(
                    state
                        .with_property_str(property.name, property.values[index])
                        .expect("valid property value"),
                );
                self.properties.push((property, index));
                Ok(())
            }
            None => {
                self.reader.set_cursor(start);
                Err(ERROR_INVALID_VALUE.create_with_context(
                    self.reader,
                    self.id.to_string(),
                    property.name.to_owned(),
                    raw.to_owned(),
                ))
            }
        }
    }
}

/// `addSuggestions(builder, property)`: integer values as ints, others by name.
fn add_property_suggestions(builder: &mut SuggestionsBuilder, property: &PropertyDef) {
    for value in property.values {
        match (property.kind, value.parse::<i32>()) {
            (mc_data::props::PropertyKind::Integer, Ok(v)) => {
                builder.suggest_int(v);
            }
            _ => {
                builder.suggest(*value);
            }
        }
    }
}

/// `BlockStateParser.serialize(state)`.
pub fn serialize(state: BlockState) -> String {
    let mut result = state.block().name().to_owned();
    let values: Vec<_> = state.values().collect();
    if !values.is_empty() {
        result.push('[');
        for (i, (property, index)) in values.iter().enumerate() {
            if i > 0 {
                result.push(',');
            }
            result.push_str(property.name);
            result.push('=');
            result.push_str(property.values[*index]);
        }
        result.push(']');
    }
    result
}
