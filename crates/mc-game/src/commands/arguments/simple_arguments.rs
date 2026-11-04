//! Small self-contained argument types: `AngleArgument`, `HexColorArgument`, `IdentifierArgument`, `TimeArgument`,
//! `UuidArgument`, `DimensionArgument`, `GameModeArgument`, `SwingAnimationArgument`, `TeamColorArgument`,
//! `HeightmapTypeArgument`, `TemplateMirrorArgument`, `TemplateRotationArgument`.

use super::coordinates::{ERROR_EXPECTED_DOUBLE, WorldCoordinate};
use super::string_representable_argument::string_representable_argument;
use crate::commands::message::{dynamic_translatable, message, simple_translatable};
use crate::commands::shared_suggestion_provider::{suggest, suggest_resource};
use crate::commands::CommandSourceStack;
use mc_brigadier::arguments::value;
use mc_brigadier::{
    ArgumentType, CommandContext, CommandSyntaxException, Dynamic2CommandExceptionType, DynamicCommandExceptionType,
    ImmutableStringReader, ParsedValue, SimpleCommandExceptionType, StringReader, Suggestions, SuggestionsBuilder,
};
use mc_core::ident::{Identifier, is_allowed_in_identifier};
use mc_core::math::mth;
use mc_core::mirror::Mirror;
use mc_core::rotation::Rotation;
use mc_world::heightmap::HeightmapType;
use mc_world::level_storage::GameType;
use std::sync::LazyLock;
use uuid::Uuid;

// ---- AngleArgument ----

static ERROR_ANGLE_NOT_COMPLETE: LazyLock<SimpleCommandExceptionType> = LazyLock::new(|| simple_translatable("argument.angle.incomplete"));
static ERROR_INVALID_ANGLE: LazyLock<SimpleCommandExceptionType> = LazyLock::new(|| simple_translatable("argument.angle.invalid"));

/// `AngleArgument.SingleAngle`.
#[derive(Clone, Copy, Debug)]
pub struct SingleAngle {
    angle: f32,
    is_relative: bool,
}

impl SingleAngle {
    pub fn get_angle(&self, sender: &CommandSourceStack) -> f32 {
        mth::wrap_degrees_f32(if self.is_relative { self.angle + sender.get_rotation().y } else { self.angle })
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct AngleArgument;

impl AngleArgument {
    pub fn angle() -> Self {
        Self
    }

    pub fn get_angle(context: &CommandContext<CommandSourceStack>, name: &str) -> f32 {
        context.get_argument::<SingleAngle>(name).get_angle(context.get_source())
    }
}

impl ArgumentType<CommandSourceStack> for AngleArgument {
    fn parse(&self, reader: &mut StringReader) -> Result<ParsedValue, CommandSyntaxException> {
        if !reader.can_read() {
            return Err(ERROR_ANGLE_NOT_COMPLETE.create_with_context(reader));
        }
        let is_relative = WorldCoordinate::is_relative(reader);
        let angle = if reader.can_read() && reader.peek() != ' ' { reader.read_float()? } else { 0.0 };
        if angle.is_nan() || angle.is_infinite() {
            Err(ERROR_INVALID_ANGLE.create_with_context(reader))
        } else {
            Ok(value(SingleAngle { angle, is_relative }))
        }
    }

    fn get_examples(&self) -> Vec<String> {
        ["0", "~", "~-5"].map(String::from).to_vec()
    }

    fn describe(&self) -> String {
        "AngleArgument".to_owned()
    }
}

// ---- HexColorArgument ----

pub static ERROR_INVALID_HEX: LazyLock<DynamicCommandExceptionType<String>> = LazyLock::new(|| dynamic_translatable("argument.hexcolor.invalid"));

/// `Integer.parseInt(CharSequence, begin, end, 16)`; a malformed digit is a `NumberFormatException`
/// (a runtime exception: brigadier reports it as "Could not parse command").
fn parse_hex(s: &str, begin: usize, end: usize) -> i32 {
    let part = &s[begin..end];
    match i32::from_str_radix(part, 16) {
        Ok(v) => v,
        Err(_) => {
            let index = part.find(|c: char| !c.is_ascii_hexdigit()).unwrap_or(0);
            panic!("Error at index {index} in: \"{part}\"")
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct HexColorArgument;

impl HexColorArgument {
    pub fn hex_color() -> Self {
        Self
    }

    pub fn get_hex_color(context: &CommandContext<CommandSourceStack>, name: &str) -> i32 {
        *context.get_argument::<i32>(name)
    }
}

/// `ARGB.color(red, green, blue)`.
fn argb_color(r: i32, g: i32, b: i32) -> i32 {
    (255u32 << 24 | (r as u32 & 0xFF) << 16 | (g as u32 & 0xFF) << 8 | (b as u32 & 0xFF)) as i32
}

impl ArgumentType<CommandSourceStack> for HexColorArgument {
    fn parse(&self, reader: &mut StringReader) -> Result<ParsedValue, CommandSyntaxException> {
        let color_string = reader.read_unquoted_string();
        let color = match color_string.chars().count() {
            3 if color_string.is_ascii() => {
                let d = |i: usize| parse_hex(&color_string, i, i + 1) * 17;
                argb_color(d(0), d(1), d(2))
            }
            6 if color_string.is_ascii() => {
                argb_color(parse_hex(&color_string, 0, 2), parse_hex(&color_string, 2, 4), parse_hex(&color_string, 4, 6))
            }
            _ => return Err(ERROR_INVALID_HEX.create_with_context(reader, color_string)),
        };
        Ok(value(color))
    }

    fn list_suggestions(&self, _context: &CommandContext<CommandSourceStack>, builder: SuggestionsBuilder) -> Suggestions {
        suggest(["F00", "FF0000"], builder)
    }

    fn get_examples(&self) -> Vec<String> {
        ["F00", "FF0000"].map(String::from).to_vec()
    }

    fn describe(&self) -> String {
        "HexColorArgument".to_owned()
    }
}

// ---- IdentifierArgument (+ Identifier.read) ----

pub static ERROR_INVALID_ID: LazyLock<SimpleCommandExceptionType> = LazyLock::new(|| simple_translatable("argument.id.invalid"));

/// `Identifier.readGreedy`.
fn read_greedy(reader: &mut StringReader) -> String {
    let start = reader.get_cursor();
    while reader.can_read() && is_allowed_in_identifier(reader.peek()) {
        reader.skip();
    }
    mc_brigadier::utf16::substring(reader.get_string(), start, reader.get_cursor())
}

/// `Identifier.read(StringReader)`.
pub fn read_identifier(reader: &mut StringReader) -> Result<Identifier, CommandSyntaxException> {
    let start = reader.get_cursor();
    let raw = read_greedy(reader);
    Identifier::parse(&raw).map_err(|_| {
        reader.set_cursor(start);
        ERROR_INVALID_ID.create_with_context(reader)
    })
}

/// `Identifier.readNonEmpty(StringReader)`.
pub fn read_identifier_non_empty(reader: &mut StringReader) -> Result<Identifier, CommandSyntaxException> {
    let start = reader.get_cursor();
    let raw = read_greedy(reader);
    if raw.is_empty() {
        return Err(ERROR_INVALID_ID.create_with_context(reader));
    }
    Identifier::parse(&raw).map_err(|_| {
        reader.set_cursor(start);
        ERROR_INVALID_ID.create_with_context(reader)
    })
}

#[derive(Clone, Copy, Debug, Default)]
pub struct IdentifierArgument;

impl IdentifierArgument {
    pub fn id() -> Self {
        Self
    }

    pub fn get_id(context: &CommandContext<CommandSourceStack>, name: &str) -> Identifier {
        context.get_argument::<Identifier>(name).clone()
    }
}

impl ArgumentType<CommandSourceStack> for IdentifierArgument {
    fn parse(&self, reader: &mut StringReader) -> Result<ParsedValue, CommandSyntaxException> {
        read_identifier(reader).map(value)
    }

    fn get_examples(&self) -> Vec<String> {
        ["foo", "foo:bar", "012"].map(String::from).to_vec()
    }

    fn describe(&self) -> String {
        "IdentifierArgument".to_owned()
    }
}

// ---- DimensionArgument ----

static ERROR_INVALID_DIMENSION: LazyLock<DynamicCommandExceptionType<String>> = LazyLock::new(|| dynamic_translatable("argument.dimension.invalid"));

#[derive(Clone, Copy, Debug, Default)]
pub struct DimensionArgument;

impl DimensionArgument {
    pub fn dimension() -> Self {
        Self
    }

    /// `getDimension`: the key of an existing level.
    pub fn get_dimension(context: &CommandContext<CommandSourceStack>, name: &str) -> Result<Identifier, CommandSyntaxException> {
        let location = context.get_argument::<Identifier>(name).clone();
        let exists = context.get_source().with_server(|server| server.level(&location).is_some());
        if exists { Ok(location) } else { Err(ERROR_INVALID_DIMENSION.create(location.to_string())) }
    }
}

impl ArgumentType<CommandSourceStack> for DimensionArgument {
    fn parse(&self, reader: &mut StringReader) -> Result<ParsedValue, CommandSyntaxException> {
        read_identifier(reader).map(value)
    }

    fn list_suggestions(&self, context: &CommandContext<CommandSourceStack>, builder: SuggestionsBuilder) -> Suggestions {
        suggest_resource(context.get_source().levels(), builder)
    }

    fn get_examples(&self) -> Vec<String> {
        ["minecraft:overworld", "minecraft:the_nether"].map(String::from).to_vec()
    }

    fn describe(&self) -> String {
        "DimensionArgument".to_owned()
    }
}

// ---- GameModeArgument ----

static ERROR_INVALID_GAMEMODE: LazyLock<DynamicCommandExceptionType<String>> = LazyLock::new(|| dynamic_translatable("argument.gamemode.invalid"));

const GAME_TYPES: [GameType; 4] = [GameType::Survival, GameType::Creative, GameType::Adventure, GameType::Spectator];

#[derive(Clone, Copy, Debug, Default)]
pub struct GameModeArgument;

impl GameModeArgument {
    pub fn game_mode() -> Self {
        Self
    }

    pub fn get_game_mode(context: &CommandContext<CommandSourceStack>, name: &str) -> GameType {
        *context.get_argument::<GameType>(name)
    }
}

impl ArgumentType<CommandSourceStack> for GameModeArgument {
    fn parse(&self, reader: &mut StringReader) -> Result<ParsedValue, CommandSyntaxException> {
        let s = reader.read_unquoted_string();
        match GAME_TYPES.into_iter().find(|g| g.get_name() == s) {
            Some(g) => Ok(value(g)),
            None => Err(ERROR_INVALID_GAMEMODE.create_with_context(reader, s)),
        }
    }

    fn list_suggestions(&self, _context: &CommandContext<CommandSourceStack>, builder: SuggestionsBuilder) -> Suggestions {
        suggest(GAME_TYPES.map(GameType::get_name), builder)
    }

    fn get_examples(&self) -> Vec<String> {
        ["survival", "creative"].map(String::from).to_vec()
    }

    fn describe(&self) -> String {
        "GameModeArgument".to_owned()
    }
}

// ---- SwingAnimationArgument ----

/// `SwingAnimationType`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SwingAnimationType {
    None,
    Whack,
    Stab,
}

impl SwingAnimationType {
    pub const VALUES: [SwingAnimationType; 3] = [Self::None, Self::Whack, Self::Stab];

    pub fn get_serialized_name(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Whack => "whack",
            Self::Stab => "stab",
        }
    }

    pub fn by_name(name: &str) -> Option<Self> {
        Self::VALUES.into_iter().find(|t| t.get_serialized_name() == name)
    }
}

static ERROR_INVALID_SWING: LazyLock<DynamicCommandExceptionType<String>> = LazyLock::new(|| dynamic_translatable("argument.swing_animation.invalid"));

#[derive(Clone, Copy, Debug, Default)]
pub struct SwingAnimationArgument;

impl SwingAnimationArgument {
    pub fn swing_animation_type() -> Self {
        Self
    }

    pub fn get_swing_animation_type(context: &CommandContext<CommandSourceStack>, name: &str) -> SwingAnimationType {
        *context.get_argument::<SwingAnimationType>(name)
    }
}

impl ArgumentType<CommandSourceStack> for SwingAnimationArgument {
    fn parse(&self, reader: &mut StringReader) -> Result<ParsedValue, CommandSyntaxException> {
        let s = reader.read_unquoted_string();
        match SwingAnimationType::by_name(&s) {
            Some(t) => Ok(value(t)),
            None => Err(ERROR_INVALID_SWING.create_with_context(reader, s)),
        }
    }

    fn list_suggestions(&self, _context: &CommandContext<CommandSourceStack>, builder: SuggestionsBuilder) -> Suggestions {
        suggest(SwingAnimationType::VALUES.map(SwingAnimationType::get_serialized_name), builder)
    }

    fn get_examples(&self) -> Vec<String> {
        ["whack", "stab"].map(String::from).to_vec()
    }

    fn describe(&self) -> String {
        "SwingAnimationArgument".to_owned()
    }
}

// ---- TeamColorArgument ----

/// `TeamColor` (the 16 colors a team can have).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TeamColor {
    Black,
    DarkBlue,
    DarkGreen,
    DarkAqua,
    DarkRed,
    DarkPurple,
    Gold,
    Gray,
    DarkGray,
    Blue,
    Green,
    Aqua,
    Red,
    LightPurple,
    Yellow,
    White,
}

impl TeamColor {
    pub const VALUES: [TeamColor; 16] = [
        Self::Black,
        Self::DarkBlue,
        Self::DarkGreen,
        Self::DarkAqua,
        Self::DarkRed,
        Self::DarkPurple,
        Self::Gold,
        Self::Gray,
        Self::DarkGray,
        Self::Blue,
        Self::Green,
        Self::Aqua,
        Self::Red,
        Self::LightPurple,
        Self::Yellow,
        Self::White,
    ];

    pub fn id(self) -> i32 {
        self as i32
    }

    pub fn get_serialized_name(self) -> &'static str {
        match self {
            Self::Black => "black",
            Self::DarkBlue => "dark_blue",
            Self::DarkGreen => "dark_green",
            Self::DarkAqua => "dark_aqua",
            Self::DarkRed => "dark_red",
            Self::DarkPurple => "dark_purple",
            Self::Gold => "gold",
            Self::Gray => "gray",
            Self::DarkGray => "dark_gray",
            Self::Blue => "blue",
            Self::Green => "green",
            Self::Aqua => "aqua",
            Self::Red => "red",
            Self::LightPurple => "light_purple",
            Self::Yellow => "yellow",
            Self::White => "white",
        }
    }

    pub fn by_name(name: &str) -> Option<TeamColor> {
        Self::VALUES.into_iter().find(|c| c.get_serialized_name() == name)
    }
}

pub static ERROR_INVALID_TEAM_COLOR: LazyLock<DynamicCommandExceptionType<String>> = LazyLock::new(|| dynamic_translatable("argument.color.invalid"));

#[derive(Clone, Copy, Debug, Default)]
pub struct TeamColorArgument;

impl TeamColorArgument {
    pub fn team_color() -> Self {
        Self
    }

    pub fn get_team_color(context: &CommandContext<CommandSourceStack>, name: &str) -> TeamColor {
        *context.get_argument::<TeamColor>(name)
    }
}

impl ArgumentType<CommandSourceStack> for TeamColorArgument {
    fn parse(&self, reader: &mut StringReader) -> Result<ParsedValue, CommandSyntaxException> {
        let id = reader.read_unquoted_string();
        match TeamColor::by_name(&id) {
            Some(c) => Ok(value(c)),
            None => Err(ERROR_INVALID_TEAM_COLOR.create_with_context(reader, id)),
        }
    }

    fn list_suggestions(&self, _context: &CommandContext<CommandSourceStack>, builder: SuggestionsBuilder) -> Suggestions {
        suggest(TeamColor::VALUES.map(TeamColor::get_serialized_name), builder)
    }

    fn get_examples(&self) -> Vec<String> {
        ["red", "green"].map(String::from).to_vec()
    }

    fn describe(&self) -> String {
        "TeamColorArgument".to_owned()
    }
}

// ---- StringRepresentable enum arguments ----

string_representable_argument!(
    /// `HeightmapTypeArgument`: the heightmap types that are kept after world generation, by lowercase key.
    HeightmapTypeArgument,
    HeightmapType,
    || HeightmapType::ALL.into_iter().filter(|t| t.keep_after_worldgen()).collect::<Vec<_>>(),
    |t: &HeightmapType| t.serialization_key().to_lowercase()
);
string_representable_argument!(
    TemplateMirrorArgument,
    Mirror,
    || Mirror::ALL.to_vec(),
    |m: &Mirror| m.get_serialized_name().to_owned()
);
string_representable_argument!(
    TemplateRotationArgument,
    Rotation,
    || Rotation::ALL.to_vec(),
    |r: &Rotation| r.get_serialized_name().to_owned()
);

// ---- TimeArgument ----

static ERROR_INVALID_UNIT: LazyLock<SimpleCommandExceptionType> = LazyLock::new(|| simple_translatable("argument.time.invalid_unit"));
static ERROR_TICK_COUNT_TOO_LOW: LazyLock<Dynamic2CommandExceptionType<i32, i32>> = LazyLock::new(|| {
    Dynamic2CommandExceptionType::new(|value: i32, limit: i32| message(crate::translatable!("argument.time.tick_count_too_low", limit, value)))
});

/// `TimeArgument.UNITS`.
const TIME_UNITS: [(&str, i32); 4] = [("d", 24000), ("s", 20), ("t", 1), ("", 1)];

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct TimeArgument {
    pub minimum: i32,
}

impl TimeArgument {
    pub fn time() -> Self {
        Self { minimum: 0 }
    }

    pub fn time_min(minimum: i32) -> Self {
        Self { minimum }
    }

    pub fn get_time(context: &CommandContext<CommandSourceStack>, name: &str) -> i32 {
        *context.get_argument::<i32>(name)
    }
}

impl ArgumentType<CommandSourceStack> for TimeArgument {
    fn parse(&self, reader: &mut StringReader) -> Result<ParsedValue, CommandSyntaxException> {
        let v = reader.read_float()?;
        let unit = reader.read_unquoted_string();
        let factor = TIME_UNITS.iter().find(|(u, _)| *u == unit).map_or(0, |(_, f)| *f);
        if factor == 0 {
            return Err(ERROR_INVALID_UNIT.create_with_context(reader));
        }
        let ticks = mc_core::math::java::round_f32(v * factor as f32);
        if ticks < self.minimum {
            Err(ERROR_TICK_COUNT_TOO_LOW.create_with_context(reader, ticks, self.minimum))
        } else {
            Ok(value(ticks))
        }
    }

    fn list_suggestions(&self, _context: &CommandContext<CommandSourceStack>, builder: SuggestionsBuilder) -> Suggestions {
        let mut reader = StringReader::new(builder.get_remaining());
        if reader.read_float().is_err() {
            return builder.build();
        }
        let offset = builder.get_start() + reader.get_cursor();
        suggest(TIME_UNITS.map(|(u, _)| u), builder.create_offset(offset))
    }

    fn get_examples(&self) -> Vec<String> {
        ["0d", "0s", "0t", "0"].map(String::from).to_vec()
    }

    fn equals(&self, other: &dyn ArgumentType<CommandSourceStack>) -> bool {
        (other as &dyn std::any::Any).downcast_ref::<TimeArgument>().is_some_and(|o| o == self)
    }

    fn describe(&self) -> String {
        "TimeArgument".to_owned()
    }
}

// ---- UuidArgument ----

static ERROR_INVALID_UUID: LazyLock<SimpleCommandExceptionType> = LazyLock::new(|| simple_translatable("argument.uuid.invalid"));

#[derive(Clone, Copy, Debug, Default)]
pub struct UuidArgument;

impl UuidArgument {
    pub fn uuid() -> Self {
        Self
    }

    pub fn get_uuid(context: &CommandContext<CommandSourceStack>, name: &str) -> Uuid {
        *context.get_argument::<Uuid>(name)
    }
}

impl ArgumentType<CommandSourceStack> for UuidArgument {
    fn parse(&self, reader: &mut StringReader) -> Result<ParsedValue, CommandSyntaxException> {
        let remaining = reader.get_remaining();
        let end = remaining.find(|c: char| !(c == '-' || c.is_ascii_hexdigit())).unwrap_or(remaining.len());
        let maybe_uuid = &remaining[..end];
        if !maybe_uuid.is_empty()
            && let Some(uuid) = mc_core::uuid_util::uuid_from_string(maybe_uuid).ok()
        {
            reader.set_cursor(reader.get_cursor() + mc_brigadier::utf16::len16(maybe_uuid));
            return Ok(value(uuid));
        }
        Err(ERROR_INVALID_UUID.create_with_context(reader))
    }

    fn get_examples(&self) -> Vec<String> {
        vec!["dd12be42-52a9-4a91-a8a1-11c01849e498".to_owned()]
    }

    fn describe(&self) -> String {
        "UuidArgument".to_owned()
    }
}

#[allow(dead_code)]
fn _unused() {
    let _ = ERROR_EXPECTED_DOUBLE.create();
}

crate::register_argument_type!(AngleArgument, |_a| mc_protocol::packets::play::ArgumentTypeTemplate::Angle);
crate::register_argument_type!(HexColorArgument, |_a| mc_protocol::packets::play::ArgumentTypeTemplate::HexColor);
crate::register_argument_type!(IdentifierArgument, |_a| mc_protocol::packets::play::ArgumentTypeTemplate::ResourceLocation);
crate::register_argument_type!(DimensionArgument, |_a| mc_protocol::packets::play::ArgumentTypeTemplate::Dimension);
crate::register_argument_type!(GameModeArgument, |_a| mc_protocol::packets::play::ArgumentTypeTemplate::Gamemode);
crate::register_argument_type!(SwingAnimationArgument, |_a| mc_protocol::packets::play::ArgumentTypeTemplate::SwingAnimation);
crate::register_argument_type!(TeamColorArgument, |_a| mc_protocol::packets::play::ArgumentTypeTemplate::TeamColor);
crate::register_argument_type!(HeightmapTypeArgument, |_a| mc_protocol::packets::play::ArgumentTypeTemplate::Heightmap);
crate::register_argument_type!(TemplateMirrorArgument, |_a| mc_protocol::packets::play::ArgumentTypeTemplate::TemplateMirror);
crate::register_argument_type!(TemplateRotationArgument, |_a| mc_protocol::packets::play::ArgumentTypeTemplate::TemplateRotation);
crate::register_argument_type!(TimeArgument, |a| mc_protocol::packets::play::ArgumentTypeTemplate::Time { min: a.minimum });
crate::register_argument_type!(UuidArgument, |_a| mc_protocol::packets::play::ArgumentTypeTemplate::Uuid);
