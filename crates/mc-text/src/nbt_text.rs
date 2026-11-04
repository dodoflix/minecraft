//! `net.minecraft.nbt.TextComponentTagVisitor`: pretty-prints a tag as a (syntax highlighted)
//! component, as `/data get` and non-interpreted `nbt` components do.

use crate::chat_formatting::ChatFormatting;
use crate::component::Component;
use crate::style::Style;
use mc_nbt::{snbt, Tag};

const MAX_DEPTH: usize = 64;
const MAX_LENGTH: usize = 128;
const INLINE_LIST_THRESHOLD: usize = 8;

/// `TextComponentTagVisitor.Styling` (`PlainStyling` / `RichStyling`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Styling {
    Plain,
    Rich,
}

#[derive(Clone, Copy)]
enum Token {
    Folded,
    ElementSeparator,
    ListClose,
    ListOpen,
    ListTypeSeparator,
    StructClose,
    StructOpen,
    NameValueSeparator,
    ByteSuffix,
    ByteArrayPrefix,
    ShortSuffix,
    IntArrayPrefix,
    LongSuffix,
    LongArrayPrefix,
    FloatSuffix,
    DoubleSuffix,
}

impl Token {
    fn text(self) -> &'static str {
        match self {
            Token::Folded => "<...>",
            Token::ElementSeparator => ",",
            Token::ListClose => "]",
            Token::ListOpen => "[",
            Token::ListTypeSeparator => ";",
            Token::StructClose => "}",
            Token::StructOpen => "{",
            Token::NameValueSeparator => ":",
            Token::ByteSuffix => "b",
            Token::ByteArrayPrefix => "B",
            Token::ShortSuffix => "s",
            Token::IntArrayPrefix => "I",
            Token::LongSuffix | Token::LongArrayPrefix => "L" ,
            Token::FloatSuffix => "f",
            Token::DoubleSuffix => "d",
        }
    }
}

impl Styling {
    fn key_style(self) -> Style {
        match self {
            Styling::Plain => Style::EMPTY,
            Styling::Rich => Style::EMPTY.with_color_formatting(Some(ChatFormatting::Aqua)),
        }
    }

    fn string_style(self) -> Style {
        match self {
            Styling::Plain => Style::EMPTY,
            Styling::Rich => Style::EMPTY.with_color_formatting(Some(ChatFormatting::Green)),
        }
    }

    fn number_style(self) -> Style {
        match self {
            Styling::Plain => Style::EMPTY,
            Styling::Rich => Style::EMPTY.with_color_formatting(Some(ChatFormatting::Gold)),
        }
    }

    fn token(self, token: Token) -> Component {
        let literal = Component::literal(token.text());
        if self == Styling::Plain {
            return literal;
        }
        match token {
            Token::Folded => literal.set_style(Style::EMPTY.with_color_formatting(Some(ChatFormatting::Gray))),
            Token::ByteSuffix
            | Token::ByteArrayPrefix
            | Token::ShortSuffix
            | Token::IntArrayPrefix
            | Token::LongSuffix
            | Token::LongArrayPrefix
            | Token::FloatSuffix
            | Token::DoubleSuffix => literal.set_style(Style::EMPTY.with_color_formatting(Some(ChatFormatting::Red))),
            _ => literal,
        }
    }
}

/// Builds the component for a tag.
pub struct TextComponentTagVisitor {
    indentation: String,
    styling: Styling,
    sort_keys: bool,
    indent_depth: usize,
    depth: usize,
    result: Component,
}

impl TextComponentTagVisitor {
    /// `new TextComponentTagVisitor(indentation)` (rich styling). Compound keys are emitted in vanilla's
    /// `HashMap` iteration order; use [`TextComponentTagVisitor::with_options`] to sort them
    /// (vanilla sorts when its logger has debug enabled).
    pub fn new(indentation: &str) -> Self {
        Self::with_options(indentation, Styling::Rich, false)
    }

    pub fn with_options(indentation: &str, styling: Styling, sort_keys: bool) -> Self {
        TextComponentTagVisitor { indentation: indentation.to_owned(), styling, sort_keys, indent_depth: 0, depth: 0, result: Component::empty() }
    }

    /// `visit(Tag)`.
    pub fn visit(mut self, tag: &Tag) -> Component {
        self.visit_tag(tag);
        self.result
    }

    fn append_str(&mut self, s: &str, style: &Style) {
        self.result.push_sibling(Component::literal(s).with_style(style));
    }

    fn append(&mut self, c: Component) {
        self.result.push_sibling(c);
    }

    fn token(&mut self, t: Token) {
        let c = self.styling.token(t);
        self.append(c);
    }

    fn number(&mut self, text: &str, suffix: Option<Token>) {
        let style = self.styling.number_style();
        self.append_str(text, &style);
        if let Some(s) = suffix {
            self.token(s);
        }
    }

    fn visit_tag(&mut self, tag: &Tag) {
        match tag {
            Tag::End => {}
            Tag::Byte(v) => self.number(&v.to_string(), Some(Token::ByteSuffix)),
            Tag::Short(v) => self.number(&v.to_string(), Some(Token::ShortSuffix)),
            Tag::Int(v) => self.number(&v.to_string(), None),
            Tag::Long(v) => self.number(&v.to_string(), Some(Token::LongSuffix)),
            Tag::Float(v) => self.number(&crate::value::java_float_to_string(*v), Some(Token::FloatSuffix)),
            Tag::Double(v) => self.number(&crate::value::java_double_to_string(*v), Some(Token::DoubleSuffix)),
            Tag::String(s) => {
                let quoted = snbt::quote_and_escape(s);
                let quote = Component::literal(&quoted[..1]);
                let style = self.styling.string_style();
                self.append(quote.clone());
                self.append_str(&quoted[1..quoted.len() - 1], &style);
                self.append(quote);
            }
            Tag::ByteArray(a) => self.visit_array(Token::ByteArrayPrefix, a.iter().map(|v| v.to_string()), a.len(), Some(Token::ByteSuffix)),
            Tag::IntArray(a) => self.visit_array(Token::IntArrayPrefix, a.iter().map(|v| v.to_string()), a.len(), None),
            Tag::LongArray(a) => self.visit_array(Token::LongArrayPrefix, a.iter().map(|v| v.to_string()), a.len(), Some(Token::LongSuffix)),
            Tag::List(l) => self.visit_list(l),
            Tag::Compound(c) => self.visit_compound(c),
        }
    }

    fn visit_array(&mut self, prefix: Token, values: impl Iterator<Item = String>, len: usize, suffix: Option<Token>) {
        self.token(Token::ListOpen);
        self.token(prefix);
        self.token(Token::ListTypeSeparator);
        for (i, v) in values.take(MAX_LENGTH).enumerate() {
            self.append(Component::literal(" "));
            self.number(&v, suffix);
            if i != len - 1 {
                self.token(Token::ElementSeparator);
            }
        }
        if len > MAX_LENGTH {
            self.token(Token::Folded);
        }
        self.token(Token::ListClose);
    }

    fn should_wrap_list_elements(list: &[Tag]) -> bool {
        list.len() < INLINE_LIST_THRESHOLD && list.iter().any(|e| !e.is_numeric())
    }

    fn visit_list(&mut self, list: &[Tag]) {
        if list.is_empty() {
            self.token(Token::ListOpen);
            self.token(Token::ListClose);
        } else if self.depth >= MAX_DEPTH {
            self.token(Token::ListOpen);
            self.token(Token::Folded);
            self.token(Token::ListClose);
        } else if !Self::should_wrap_list_elements(list) {
            self.token(Token::ListOpen);
            for (i, e) in list.iter().enumerate() {
                if i != 0 {
                    self.token(Token::ElementSeparator);
                    self.append(Component::literal(" "));
                }
                self.append_sub_tag(e, false);
            }
            self.token(Token::ListClose);
        } else {
            self.token(Token::ListOpen);
            if !self.indentation.is_empty() {
                self.append(Component::literal("\n"));
            }
            let entry_indent = Component::literal(self.indentation.repeat(self.indent_depth + 1));
            let element_spacing = if self.indentation.is_empty() { Component::literal(" ") } else { Component::literal("\n") };
            for (i, e) in list.iter().take(MAX_LENGTH).enumerate() {
                self.append(entry_indent.clone());
                self.append_sub_tag(e, true);
                if i != list.len() - 1 {
                    self.token(Token::ElementSeparator);
                    self.append(element_spacing.clone());
                }
            }
            if list.len() > MAX_LENGTH {
                self.append(entry_indent);
                self.token(Token::Folded);
            }
            if !self.indentation.is_empty() {
                self.append(Component::literal("\n"));
                self.append(Component::literal(self.indentation.repeat(self.indent_depth)));
            }
            self.token(Token::ListClose);
        }
    }

    fn visit_compound(&mut self, tag: &mc_nbt::CompoundTag) {
        if tag.is_empty() {
            self.token(Token::StructOpen);
            self.token(Token::StructClose);
        } else if self.depth >= MAX_DEPTH {
            self.token(Token::StructOpen);
            self.token(Token::Folded);
            self.token(Token::StructClose);
        } else {
            self.token(Token::StructOpen);
            let mut entries: Vec<(&String, &Tag)> = tag.iter_java_order();
            if self.sort_keys {
                // Java `String.compareTo`: UTF-16 code unit order.
                entries.sort_by(|a, b| a.0.encode_utf16().cmp(b.0.encode_utf16()));
            }
            if !self.indentation.is_empty() {
                self.append(Component::literal("\n"));
            }
            let entry_indent = Component::literal(self.indentation.repeat(self.indent_depth + 1));
            let element_spacing = if self.indentation.is_empty() { Component::literal(" ") } else { Component::literal("\n") };
            let last = entries.len() - 1;
            for (i, (key, value)) in entries.into_iter().enumerate() {
                self.append(entry_indent.clone());
                let k = self.handle_escape_pretty(key);
                self.append(k);
                self.token(Token::NameValueSeparator);
                self.append(Component::literal(" "));
                self.append_sub_tag(value, true);
                if i != last {
                    self.token(Token::ElementSeparator);
                    self.append(element_spacing.clone());
                }
            }
            if !self.indentation.is_empty() {
                self.append(Component::literal("\n"));
                self.append(Component::literal(self.indentation.repeat(self.indent_depth)));
            }
            self.token(Token::StructClose);
        }
    }

    fn append_sub_tag(&mut self, tag: &Tag, indent: bool) {
        if indent {
            self.indent_depth += 1;
        }
        self.depth += 1;
        self.visit_tag(tag);
        if indent {
            self.indent_depth -= 1;
        }
        self.depth -= 1;
    }

    fn handle_escape_pretty(&self, input: &str) -> Component {
        let key_style = self.styling.key_style();
        let simple = !input.is_empty() && input.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '+' | '-'));
        if simple {
            Component::literal(input).with_style(&key_style)
        } else {
            let quoted = snbt::quote_and_escape(input);
            let quote = &quoted[..1];
            let inner = Component::literal(&quoted[1..quoted.len() - 1]).with_style(&key_style);
            Component::literal(quote).append(inner).append(Component::literal(quote))
        }
    }
}
