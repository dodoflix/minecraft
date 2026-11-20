//! `net.minecraft.network.chat.Component` / `MutableComponent` / `ComponentContents` and the
//! concrete contents types.

use crate::chat_formatting::ChatFormatting;
use crate::language::Language;
use crate::profile::ResolvableProfile;
use crate::style::{FontDescription, Style};
use crate::value::Num;
use mc_core::ident::Identifier;
use mc_core::java_string::JavaString;
use std::fmt;

/// U+FFFC, what `ObjectContents` emits for styled visiting.
const OBJECT_PLACEHOLDER: &str = "\u{fffc}";

/// A chat component: contents + siblings + style. Unlike vanilla there is a single (owned,
/// mutable-by-value) type covering both `Component` and `MutableComponent`.
#[derive(Clone, Debug, PartialEq)]
pub struct Component {
    pub(crate) contents: ComponentContents,
    pub(crate) siblings: Vec<Component>,
    pub(crate) style: Style,
}

/// The `ComponentContents` implementations (plain text, translatable, keybind, score, selector, nbt, object).
#[derive(Clone, Debug, PartialEq)]
pub enum ComponentContents {
    /// `PlainTextContents` (the empty string is `PlainTextContents.EMPTY`).
    Literal(JavaString),
    Translatable(TranslatableContents),
    /// `KeybindContents` (the key name).
    Keybind(JavaString),
    Score(ScoreContents),
    Selector(SelectorContents),
    Nbt(NbtContents),
    Object(ObjectContents),
}

/// A `TranslatableContents` argument (`Object` in vanilla): a primitive or a component.
#[derive(Clone, Debug, PartialEq)]
pub enum TranslatableArg {
    String(JavaString),
    Bool(bool),
    Number(Num),
    Component(Component),
}

impl TranslatableArg {
    /// `TranslatableContents.isAllowedPrimitiveArgument` is statically guaranteed by this enum.
    /// `Object.toString()` of a primitive argument.
    fn primitive_string(&self) -> Option<JavaString> {
        match self {
            TranslatableArg::String(s) => Some(s.clone()),
            TranslatableArg::Bool(b) => Some(b.to_string().into()),
            TranslatableArg::Number(n) => Some(n.to_string().into()),
            TranslatableArg::Component(_) => None,
        }
    }
}

impl From<&str> for TranslatableArg {
    fn from(s: &str) -> Self {
        TranslatableArg::String(s.into())
    }
}
impl From<String> for TranslatableArg {
    fn from(s: String) -> Self {
        TranslatableArg::String(s.into())
    }
}
impl From<JavaString> for TranslatableArg {
    fn from(s: JavaString) -> Self {
        TranslatableArg::String(s)
    }
}
impl From<bool> for TranslatableArg {
    fn from(b: bool) -> Self {
        TranslatableArg::Bool(b)
    }
}
impl From<i32> for TranslatableArg {
    fn from(v: i32) -> Self {
        TranslatableArg::Number(Num::Int(v))
    }
}
impl From<i64> for TranslatableArg {
    fn from(v: i64) -> Self {
        TranslatableArg::Number(Num::Long(v))
    }
}
impl From<f32> for TranslatableArg {
    fn from(v: f32) -> Self {
        TranslatableArg::Number(Num::Float(v))
    }
}
impl From<f64> for TranslatableArg {
    fn from(v: f64) -> Self {
        TranslatableArg::Number(Num::Double(v))
    }
}
impl From<Component> for TranslatableArg {
    fn from(c: Component) -> Self {
        TranslatableArg::Component(c)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct TranslatableContents {
    pub key: String,
    pub fallback: Option<JavaString>,
    pub args: Vec<TranslatableArg>,
}

/// One piece of a decomposed translation template.
enum Part<'a> {
    Text(JavaString),
    Component(&'a Component),
}

impl TranslatableContents {
    pub fn new(
        key: impl Into<String>,
        fallback: Option<JavaString>,
        args: Vec<TranslatableArg>,
    ) -> Self {
        TranslatableContents {
            key: key.into(),
            fallback,
            args,
        }
    }

    /// `decompose()`: the template with `%s`/`%n$s`/`%%` substituted. Any malformed template makes
    /// vanilla fall back to the raw format string as a single part.
    fn decompose(&self, language: &Language) -> Vec<Part<'_>> {
        let from_language;
        let format: &JavaString = match language.get(&self.key) {
            Some(text) => {
                from_language = JavaString::from(text);
                &from_language
            }
            None => match &self.fallback {
                Some(fallback) => fallback,
                None => {
                    from_language = JavaString::from(self.key.as_str());
                    &from_language
                }
            },
        };
        self.decompose_template(format)
            .unwrap_or_else(|()| vec![Part::Text(format.clone())])
    }

    /// `decomposeTemplate`; `Err` stands for `TranslatableFormatException`/`IllegalArgumentException`.
    fn decompose_template(&self, format: &JavaString) -> Result<Vec<Part<'_>>, ()> {
        // The text between two byte offsets of the template; a lone surrogate is one unit and one character of the
        // text, so unit offsets can be counted on the text.
        let slice = |from: usize, to: usize| -> JavaString {
            let text = format.as_lossy_str();
            if format.has_lone_surrogate() {
                let unit = |byte: usize| text[..byte].encode_utf16().count();
                format.substring(unit(from), unit(to))
            } else {
                text[from..to].into()
            }
        };
        let template = format.as_lossy_str();
        let b = template.as_bytes();
        let mut parts = Vec::new();
        let mut replacement_index = 0usize;
        let mut current = 0;
        // Regex `%(?:(\d+)\$)?([A-Za-z%]|$)`: a '%' that does not start a match always ends up in a
        // literal prefix/tail, which vanilla rejects, so every '%' must start a valid directive.
        while let Some(rel) = template[current..].find('%') {
            let start = current + rel;
            if start > current {
                parts.push(Part::Text(slice(current, start)));
            }
            let mut j = start + 1;
            while j < b.len() && b[j].is_ascii_digit() {
                j += 1;
            }
            let (group, m) = if j > start + 1 && j < b.len() && b[j] == b'$' {
                (Some(&template[start + 1..j]), j + 1)
            } else {
                (None, start + 1)
            };
            let format_type = match b.get(m) {
                Some(&c) if c.is_ascii_alphabetic() || c == b'%' => c,
                _ => return Err(()),
            };
            let end = m + 1;
            if format_type == b'%' && &template[start..end] == "%%" {
                parts.push(Part::Text("%".into()));
            } else {
                if format_type != b's' {
                    return Err(());
                }
                let index = match group {
                    Some(g) => i64::from(g.parse::<i32>().map_err(|_| ())?) - 1,
                    None => {
                        replacement_index += 1;
                        replacement_index as i64 - 1
                    }
                };
                let arg = usize::try_from(index)
                    .ok()
                    .and_then(|i| self.args.get(i))
                    .ok_or(())?;
                parts.push(match arg {
                    TranslatableArg::Component(c) => Part::Component(c),
                    other => Part::Text(other.primitive_string().unwrap_or_default()),
                });
            }
            current = end;
        }
        if current < template.len() {
            parts.push(Part::Text(slice(current, template.len())));
        }
        Ok(parts)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum ScoreName {
    /// A valid entity selector (`CompilableString<EntitySelector>`), source text.
    Selector(String),
    /// A literal score holder name.
    Name(String),
}

#[derive(Clone, Debug, PartialEq)]
pub struct ScoreContents {
    pub name: ScoreName,
    pub objective: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SelectorContents {
    /// Entity selector source text.
    pub selector: String,
    pub separator: Option<Box<Component>>,
}

/// `DataSource` (`entity` / `block` / `storage`); the strings are the selector/coordinate source text.
#[derive(Clone, Debug, PartialEq)]
pub enum DataSource {
    Entity(String),
    Block(String),
    Storage(Identifier),
}

#[derive(Clone, Debug, PartialEq)]
pub struct NbtContents {
    /// NBT path source text.
    pub nbt_path: String,
    pub interpreting: bool,
    pub plain: bool,
    pub separator: Option<Box<Component>>,
    pub data_source: DataSource,
}

/// `ObjectInfo` (`atlas` / `player` sprites).
#[derive(Clone, Debug, PartialEq)]
pub enum ObjectInfo {
    AtlasSprite {
        atlas: Identifier,
        sprite: Identifier,
    },
    PlayerSprite {
        player: Box<ResolvableProfile>,
        hat: bool,
    },
}

impl ObjectInfo {
    /// `AtlasSprite.DEFAULT_ATLAS` (`minecraft:blocks`).
    pub fn default_atlas() -> Identifier {
        crate::mc_id("blocks")
    }

    pub fn font_description(&self) -> FontDescription {
        match self {
            ObjectInfo::AtlasSprite { atlas, sprite } => FontDescription::AtlasSprite {
                atlas_id: atlas.clone(),
                sprite_id: sprite.clone(),
            },
            ObjectInfo::PlayerSprite { player, hat } => FontDescription::PlayerSprite {
                profile: player.clone(),
                hat: *hat,
            },
        }
    }

    pub fn default_fallback(&self) -> String {
        fn short(id: &Identifier) -> String {
            if id.namespace() == "minecraft" {
                id.path().to_owned()
            } else {
                id.to_string()
            }
        }
        match self {
            ObjectInfo::AtlasSprite { atlas, sprite } => {
                if *atlas == Self::default_atlas() {
                    format!("[{}]", short(sprite))
                } else {
                    format!("[{}@{}]", short(sprite), short(atlas))
                }
            }
            ObjectInfo::PlayerSprite { player, .. } => match player.name() {
                Some(name) => format!("[{name} head]"),
                None => "[unknown player head]".to_owned(),
            },
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ObjectContents {
    pub contents: ObjectInfo,
    pub fallback: Option<Box<Component>>,
}

impl Default for Component {
    fn default() -> Self {
        Component::empty()
    }
}

impl Component {
    pub fn new(contents: ComponentContents) -> Component {
        Component {
            contents,
            siblings: Vec::new(),
            style: Style::EMPTY,
        }
    }

    /// `Component.literal` (`PlainTextContents.create`).
    pub fn literal(text: impl Into<JavaString>) -> Component {
        Component::new(ComponentContents::Literal(text.into()))
    }

    /// `Component.empty()`.
    pub fn empty() -> Component {
        Component::literal("")
    }

    /// `Component.nullToEmpty`.
    pub fn null_to_empty(text: Option<&str>) -> Component {
        Component::literal(text.unwrap_or(""))
    }

    pub fn translatable(key: impl Into<String>, args: Vec<TranslatableArg>) -> Component {
        Component::new(ComponentContents::Translatable(TranslatableContents::new(
            key, None, args,
        )))
    }

    pub fn translatable_with_fallback(
        key: impl Into<String>,
        fallback: Option<JavaString>,
        args: Vec<TranslatableArg>,
    ) -> Component {
        Component::new(ComponentContents::Translatable(TranslatableContents::new(
            key, fallback, args,
        )))
    }

    pub fn keybind(name: impl Into<JavaString>) -> Component {
        Component::new(ComponentContents::Keybind(name.into()))
    }

    pub fn score_selector(pattern: impl Into<String>, objective: impl Into<String>) -> Component {
        Component::new(ComponentContents::Score(ScoreContents {
            name: ScoreName::Selector(pattern.into()),
            objective: objective.into(),
        }))
    }

    pub fn score(name: impl Into<String>, objective: impl Into<String>) -> Component {
        Component::new(ComponentContents::Score(ScoreContents {
            name: ScoreName::Name(name.into()),
            objective: objective.into(),
        }))
    }

    pub fn selector(pattern: impl Into<String>, separator: Option<Component>) -> Component {
        Component::new(ComponentContents::Selector(SelectorContents {
            selector: pattern.into(),
            separator: separator.map(Box::new),
        }))
    }

    pub fn nbt(
        nbt_path: impl Into<String>,
        interpreting: bool,
        plain: bool,
        separator: Option<Component>,
        data_source: DataSource,
    ) -> Component {
        Component::new(ComponentContents::Nbt(NbtContents {
            nbt_path: nbt_path.into(),
            interpreting,
            plain,
            separator: separator.map(Box::new),
            data_source,
        }))
    }

    pub fn object(info: ObjectInfo, fallback: Option<Component>) -> Component {
        Component::new(ComponentContents::Object(ObjectContents {
            contents: info,
            fallback: fallback.map(Box::new),
        }))
    }

    pub fn contents(&self) -> &ComponentContents {
        &self.contents
    }

    pub fn siblings(&self) -> &[Component] {
        &self.siblings
    }

    pub fn style(&self) -> &Style {
        &self.style
    }

    /// `MutableComponent.setStyle`.
    pub fn set_style(mut self, style: Style) -> Component {
        self.style = style;
        self
    }

    /// `append(Component)`.
    pub fn append(mut self, component: Component) -> Component {
        self.siblings.push(component);
        self
    }

    /// `append(String)`: empty text is ignored.
    pub fn append_str(self, text: &str) -> Component {
        if text.is_empty() {
            self
        } else {
            self.append(Component::literal(text))
        }
    }

    /// In-place `append` for accumulation loops.
    pub fn push_sibling(&mut self, component: Component) {
        self.siblings.push(component);
    }

    /// `withStyle(UnaryOperator<Style>)`.
    pub fn with_style_fn(mut self, updater: impl FnOnce(&Style) -> Style) -> Component {
        self.style = updater(&self.style);
        self
    }

    /// `withStyle(Style)`: `patch.applyTo(current)`.
    pub fn with_style(mut self, patch: &Style) -> Component {
        self.style = patch.apply_to(&self.style);
        self
    }

    pub fn with_formats(mut self, formats: &[ChatFormatting]) -> Component {
        self.style = self.style.apply_formats(formats);
        self
    }

    /// `withStyle(ChatFormatting)`.
    pub fn with_format(mut self, format: ChatFormatting) -> Component {
        self.style = self.style.apply_format(format);
        self
    }

    /// `withColor(int)`.
    pub fn with_color(mut self, color: i32) -> Component {
        self.style = self.style.with_color_rgb(color);
        self
    }

    pub fn without_shadow(mut self) -> Component {
        self.style = self.style.without_shadow();
        self
    }

    /// `tryCollapseToString`: the text when this is plain, unstyled and childless.
    pub fn try_collapse_to_string(&self) -> Option<&str> {
        self.try_collapse_to_java_string()
            .map(JavaString::as_lossy_str)
    }

    /// [`try_collapse_to_string`](Self::try_collapse_to_string) with the exact string.
    pub fn try_collapse_to_java_string(&self) -> Option<&JavaString> {
        match &self.contents {
            ComponentContents::Literal(text)
                if self.siblings.is_empty() && self.style.is_empty() =>
            {
                Some(text)
            }
            _ => None,
        }
    }

    /// `plainCopy`.
    pub fn plain_copy(&self) -> Component {
        Component::new(self.contents.clone())
    }

    /// `copy` (same contents/style, copied sibling list).
    pub fn copy(&self) -> Component {
        self.clone()
    }

    // ---- visiting -----------------------------------------------------------------------

    /// `visit(ContentConsumer)`: stops at the first `Some`. A lone surrogate shows as U+FFFD; see
    /// [`visit_java`](Self::visit_java).
    pub fn visit<T>(&self, output: &mut dyn FnMut(&str) -> Option<T>) -> Option<T> {
        self.visit_java(&mut |s| output(s.as_lossy_str()))
    }

    /// [`visit`](Self::visit) with the exact strings.
    pub fn visit_java<T>(&self, output: &mut dyn FnMut(&JavaString) -> Option<T>) -> Option<T> {
        if let Some(r) = self.contents.visit(output) {
            return Some(r);
        }
        self.siblings.iter().find_map(|s| s.visit_java(output))
    }

    /// `visit(StyledContentConsumer, parentStyle)`.
    pub fn visit_styled<T>(
        &self,
        output: &mut dyn FnMut(&Style, &str) -> Option<T>,
        parent_style: &Style,
    ) -> Option<T> {
        self.visit_styled_java(
            &mut |style, s| output(style, s.as_lossy_str()),
            parent_style,
        )
    }

    /// [`visit_styled`](Self::visit_styled) with the exact strings.
    pub fn visit_styled_java<T>(
        &self,
        output: &mut dyn FnMut(&Style, &JavaString) -> Option<T>,
        parent_style: &Style,
    ) -> Option<T> {
        let self_style = self.style.apply_to(parent_style);
        if let Some(r) = self.contents.visit_styled(output, &self_style) {
            return Some(r);
        }
        self.siblings
            .iter()
            .find_map(|s| s.visit_styled_java(output, &self_style))
    }

    /// `getString()`: the plain text (translations resolved with the current `Language`). The halves of a surrogate
    /// pair that sit in different components join into one character.
    pub fn get_java_string(&self) -> JavaString {
        let mut out = JavaString::new();
        self.visit_java::<()>(&mut |s| {
            out.push_java(s);
            None
        });
        out
    }

    /// [`get_java_string`](Self::get_java_string) with each lone surrogate shown as U+FFFD.
    pub fn get_string(&self) -> String {
        self.get_java_string().into_string_lossy()
    }

    /// `getString(int limit)`; the limit is in UTF-16 code units.
    pub fn get_string_limit(&self, limit: usize) -> String {
        let mut out = String::new();
        let mut len16 = 0usize;
        self.visit(&mut |s: &str| {
            let remaining = limit.saturating_sub(len16);
            if remaining == 0 {
                return Some(());
            }
            let mut taken = 0;
            for c in s.chars() {
                let w = c.len_utf16();
                if taken + w > remaining {
                    break;
                }
                taken += w;
                out.push(c);
            }
            len16 += taken;
            None
        });
        out
    }

    /// `toFlatList(Style)`.
    pub fn to_flat_list(&self) -> Vec<Component> {
        self.to_flat_list_styled(&Style::EMPTY)
    }

    pub fn to_flat_list_styled(&self, root_style: &Style) -> Vec<Component> {
        let mut result = Vec::new();
        self.visit_styled_java::<()>(
            &mut |style, contents| {
                if !contents.as_lossy_str().is_empty() {
                    result.push(Component::literal(contents.clone()).with_style(style));
                }
                None
            },
            root_style,
        );
        result
    }

    /// `contains(Component)`.
    pub fn contains(&self, other: &Component) -> bool {
        if self == other {
            return true;
        }
        let flat = self.to_flat_list();
        let other_flat = other.to_flat_list_styled(&self.style);
        if other_flat.is_empty() {
            return true;
        }
        flat.windows(other_flat.len())
            .any(|w| w == other_flat.as_slice())
    }
}

impl ComponentContents {
    fn visit<T>(&self, output: &mut dyn FnMut(&JavaString) -> Option<T>) -> Option<T> {
        match self {
            ComponentContents::Literal(text) => output(text),
            ComponentContents::Translatable(t) => {
                let language = Language::get_instance();
                for part in t.decompose(&language) {
                    let r = match part {
                        Part::Text(s) => output(&s),
                        Part::Component(c) => c.visit_java(output),
                    };
                    if r.is_some() {
                        return r;
                    }
                }
                None
            }
            ComponentContents::Keybind(name) => output(name),
            ComponentContents::Selector(s) => output(&s.selector.as_str().into()),
            ComponentContents::Object(o) => match &o.fallback {
                Some(f) => f.visit_java(output),
                None => output(&o.contents.default_fallback().into()),
            },
            ComponentContents::Score(_) | ComponentContents::Nbt(_) => None,
        }
    }

    fn visit_styled<T>(
        &self,
        output: &mut dyn FnMut(&Style, &JavaString) -> Option<T>,
        current_style: &Style,
    ) -> Option<T> {
        match self {
            ComponentContents::Literal(text) => output(current_style, text),
            ComponentContents::Translatable(t) => {
                let language = Language::get_instance();
                for part in t.decompose(&language) {
                    let r = match part {
                        Part::Text(s) => output(current_style, &s),
                        Part::Component(c) => c.visit_styled_java(output, current_style),
                    };
                    if r.is_some() {
                        return r;
                    }
                }
                None
            }
            // KeybindResolver default: `Component.literal(name)`, which inherits the current style.
            ComponentContents::Keybind(name) => output(current_style, name),
            ComponentContents::Selector(s) => output(current_style, &s.selector.as_str().into()),
            ComponentContents::Object(o) => output(
                &current_style.with_font(Some(o.contents.font_description())),
                &OBJECT_PLACEHOLDER.into(),
            ),
            ComponentContents::Score(_) | ComponentContents::Nbt(_) => None,
        }
    }
}

/// `MutableComponent.toString()` / contents `toString()`.
impl fmt::Display for ComponentContents {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ComponentContents::Literal(t) if t.as_lossy_str().is_empty() => f.write_str("empty"),
            ComponentContents::Literal(t) => write!(f, "literal{{{}}}", t.as_lossy_str()),
            ComponentContents::Translatable(t) => {
                write!(f, "translation{{key='{}'", t.key)?;
                if let Some(fb) = &t.fallback {
                    write!(f, ", fallback='{}'", fb.as_lossy_str())?;
                }
                write!(f, ", args={:?}}}", t.args)
            }
            ComponentContents::Keybind(k) => write!(f, "keybind{{{}}}", k.as_lossy_str()),
            ComponentContents::Score(s) => write!(
                f,
                "score{{name='{:?}', objective='{}'}}",
                s.name, s.objective
            ),
            ComponentContents::Selector(s) => write!(f, "pattern{{{}}}", s.selector),
            ComponentContents::Nbt(n) => write!(f, "{n:?}"),
            ComponentContents::Object(o) => write!(f, "{o:?}"),
        }
    }
}

impl fmt::Display for Component {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.contents)?;
        let has_style = !self.style.is_empty();
        let has_siblings = !self.siblings.is_empty();
        if has_style || has_siblings {
            f.write_str("[")?;
            if has_style {
                write!(f, "style={}", self.style)?;
            }
            if has_style && has_siblings {
                f.write_str(", ")?;
            }
            if has_siblings {
                f.write_str("siblings=[")?;
                for (i, s) in self.siblings.iter().enumerate() {
                    if i > 0 {
                        f.write_str(", ")?;
                    }
                    write!(f, "{s}")?;
                }
                f.write_str("]")?;
            }
            f.write_str("]")?;
        }
        Ok(())
    }
}

impl From<&str> for Component {
    fn from(s: &str) -> Self {
        Component::literal(s)
    }
}
impl From<String> for Component {
    fn from(s: String) -> Self {
        Component::literal(s)
    }
}
