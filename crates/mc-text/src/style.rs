//! `net.minecraft.network.chat.Style` and `FontDescription`.

use crate::chat_formatting::ChatFormatting;
use crate::events::{ClickEvent, HoverEvent};
use crate::profile::ResolvableProfile;
use crate::text_color::TextColor;
use mc_core::ident::Identifier;
use std::fmt;

/// `FontDescription`. Only `Resource` is serialisable (vanilla's codec rejects the sprite variants);
/// the sprite variants appear in the styles handed to visitors by `ObjectContents`.
#[derive(Clone, Debug, PartialEq)]
pub enum FontDescription {
    Resource(Identifier),
    AtlasSprite {
        atlas_id: Identifier,
        sprite_id: Identifier,
    },
    PlayerSprite {
        profile: Box<ResolvableProfile>,
        hat: bool,
    },
}

impl FontDescription {
    /// `FontDescription.DEFAULT` (`minecraft:default`).
    pub fn default_font() -> FontDescription {
        FontDescription::Resource(crate::mc_id("default"))
    }
}

/// An immutable chat style. `None` means "inherit".
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Style {
    color: Option<TextColor>,
    shadow_color: Option<i32>,
    bold: Option<bool>,
    italic: Option<bool>,
    underlined: Option<bool>,
    strikethrough: Option<bool>,
    obfuscated: Option<bool>,
    // boxed: these are rare and large, keeping `Style`/`Component` small
    click_event: Option<Box<ClickEvent>>,
    hover_event: Option<Box<HoverEvent>>,
    insertion: Option<String>,
    font: Option<Box<FontDescription>>,
}

macro_rules! with_fn {
    ($name:ident, $field:ident, $ty:ty) => {
        pub fn $name(&self, $field: $ty) -> Style {
            let mut s = self.clone();
            s.$field = $field;
            s
        }
    };
}

impl Style {
    pub const EMPTY: Style = Style {
        color: None,
        shadow_color: None,
        bold: None,
        italic: None,
        underlined: None,
        strikethrough: None,
        obfuscated: None,
        click_event: None,
        hover_event: None,
        insertion: None,
        font: None,
    };
    /// `Style.NO_SHADOW`.
    pub const NO_SHADOW: i32 = 0;

    /// Used by the codec (`Style.create`).
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn create(
        color: Option<TextColor>,
        shadow_color: Option<i32>,
        bold: Option<bool>,
        italic: Option<bool>,
        underlined: Option<bool>,
        strikethrough: Option<bool>,
        obfuscated: Option<bool>,
        click_event: Option<ClickEvent>,
        hover_event: Option<HoverEvent>,
        insertion: Option<String>,
        font: Option<FontDescription>,
    ) -> Style {
        Style {
            color,
            shadow_color,
            bold,
            italic,
            underlined,
            strikethrough,
            obfuscated,
            click_event: click_event.map(Box::new),
            hover_event: hover_event.map(Box::new),
            insertion,
            font: font.map(Box::new),
        }
    }

    pub fn color(&self) -> Option<TextColor> {
        self.color
    }
    pub fn shadow_color(&self) -> Option<i32> {
        self.shadow_color
    }
    pub fn is_bold(&self) -> bool {
        self.bold == Some(true)
    }
    pub fn is_italic(&self) -> bool {
        self.italic == Some(true)
    }
    pub fn is_strikethrough(&self) -> bool {
        self.strikethrough == Some(true)
    }
    pub fn is_underlined(&self) -> bool {
        self.underlined == Some(true)
    }
    pub fn is_obfuscated(&self) -> bool {
        self.obfuscated == Some(true)
    }
    /// Raw tri-state accessors (`None` = unset), as the codec needs them.
    pub fn bold(&self) -> Option<bool> {
        self.bold
    }
    pub fn italic(&self) -> Option<bool> {
        self.italic
    }
    pub fn underlined(&self) -> Option<bool> {
        self.underlined
    }
    pub fn strikethrough(&self) -> Option<bool> {
        self.strikethrough
    }
    pub fn obfuscated(&self) -> Option<bool> {
        self.obfuscated
    }
    pub fn is_empty(&self) -> bool {
        *self == Style::EMPTY
    }
    pub fn click_event(&self) -> Option<&ClickEvent> {
        self.click_event.as_deref()
    }
    pub fn hover_event(&self) -> Option<&HoverEvent> {
        self.hover_event.as_deref()
    }
    pub fn insertion(&self) -> Option<&str> {
        self.insertion.as_deref()
    }
    /// `getFont()`: the explicit font or `minecraft:default`.
    pub fn font(&self) -> FontDescription {
        self.font
            .as_deref()
            .cloned()
            .unwrap_or_else(FontDescription::default_font)
    }
    /// The explicitly set font, if any.
    pub fn font_opt(&self) -> Option<&FontDescription> {
        self.font.as_deref()
    }

    with_fn!(with_color, color, Option<TextColor>);
    with_fn!(with_bold, bold, Option<bool>);
    with_fn!(with_italic, italic, Option<bool>);
    with_fn!(with_underlined, underlined, Option<bool>);
    with_fn!(with_strikethrough, strikethrough, Option<bool>);
    with_fn!(with_obfuscated, obfuscated, Option<bool>);
    with_fn!(with_insertion, insertion, Option<String>);

    pub fn with_click_event(&self, click_event: Option<ClickEvent>) -> Style {
        Style {
            click_event: click_event.map(Box::new),
            ..self.clone()
        }
    }

    pub fn with_hover_event(&self, hover_event: Option<HoverEvent>) -> Style {
        Style {
            hover_event: hover_event.map(Box::new),
            ..self.clone()
        }
    }

    pub fn with_font(&self, font: Option<FontDescription>) -> Style {
        Style {
            font: font.map(Box::new),
            ..self.clone()
        }
    }

    pub fn with_color_formatting(&self, color: Option<ChatFormatting>) -> Style {
        self.with_color(color.and_then(TextColor::from_legacy_format))
    }

    pub fn with_color_rgb(&self, color: i32) -> Style {
        self.with_color(Some(TextColor::from_rgb(color)))
    }

    pub fn with_shadow_color(&self, shadow_color: i32) -> Style {
        let mut s = self.clone();
        s.shadow_color = Some(shadow_color);
        s
    }

    /// Removes an explicit shadow colour (the `None` case of vanilla's nullable setter).
    pub fn clear_shadow_color(&self) -> Style {
        let mut s = self.clone();
        s.shadow_color = None;
        s
    }

    pub fn without_shadow(&self) -> Style {
        self.with_shadow_color(Style::NO_SHADOW)
    }

    fn apply_format_to(&mut self, format: ChatFormatting) -> bool {
        match format {
            ChatFormatting::Obfuscated => self.obfuscated = Some(true),
            ChatFormatting::Bold => self.bold = Some(true),
            ChatFormatting::Strikethrough => self.strikethrough = Some(true),
            ChatFormatting::Underline => self.underlined = Some(true),
            ChatFormatting::Italic => self.italic = Some(true),
            ChatFormatting::Reset => return false,
            color => self.color = TextColor::from_legacy_format(color),
        }
        true
    }

    /// `applyFormat`: `Reset` yields `EMPTY`.
    pub fn apply_format(&self, format: ChatFormatting) -> Style {
        let mut s = self.clone();
        if s.apply_format_to(format) {
            s
        } else {
            Style::EMPTY
        }
    }

    /// `applyLegacyFormat`: a colour also clears every other formatting flag.
    pub fn apply_legacy_format(&self, format: ChatFormatting) -> Style {
        let mut s = self.clone();
        match format {
            ChatFormatting::Obfuscated
            | ChatFormatting::Bold
            | ChatFormatting::Strikethrough
            | ChatFormatting::Underline
            | ChatFormatting::Italic => {
                s.apply_format_to(format);
            }
            ChatFormatting::Reset => return Style::EMPTY,
            color => {
                s.obfuscated = Some(false);
                s.bold = Some(false);
                s.strikethrough = Some(false);
                s.underlined = Some(false);
                s.italic = Some(false);
                s.color = TextColor::from_legacy_format(color);
            }
        }
        s
    }

    pub fn apply_formats(&self, formats: &[ChatFormatting]) -> Style {
        let mut s = self.clone();
        for &f in formats {
            if !s.apply_format_to(f) {
                return Style::EMPTY;
            }
        }
        s
    }

    /// `this.applyTo(other)`: `self` wins, `other` fills the unset fields.
    pub fn apply_to(&self, other: &Style) -> Style {
        if self.is_empty() {
            return other.clone();
        }
        if other.is_empty() {
            return self.clone();
        }
        Style {
            color: self.color.or(other.color),
            shadow_color: self.shadow_color.or(other.shadow_color),
            bold: self.bold.or(other.bold),
            italic: self.italic.or(other.italic),
            underlined: self.underlined.or(other.underlined),
            strikethrough: self.strikethrough.or(other.strikethrough),
            obfuscated: self.obfuscated.or(other.obfuscated),
            click_event: self
                .click_event
                .clone()
                .or_else(|| other.click_event.clone()),
            hover_event: self
                .hover_event
                .clone()
                .or_else(|| other.hover_event.clone()),
            insertion: self.insertion.clone().or_else(|| other.insertion.clone()),
            font: self.font.clone().or_else(|| other.font.clone()),
        }
    }
}

/// Mirrors `Style.toString()` (`{color=red,bold,!italic,...}`).
impl fmt::Display for Style {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut first = true;
        let mut sep = |f: &mut fmt::Formatter<'_>| -> fmt::Result {
            if !first {
                f.write_str(",")?;
            }
            first = false;
            Ok(())
        };
        f.write_str("{")?;
        if let Some(c) = &self.color {
            sep(f)?;
            write!(f, "color={c}")?;
        }
        if let Some(c) = &self.shadow_color {
            sep(f)?;
            write!(f, "shadowColor={c}")?;
        }
        for (name, v) in [
            ("bold", self.bold),
            ("italic", self.italic),
            ("underlined", self.underlined),
            ("strikethrough", self.strikethrough),
            ("obfuscated", self.obfuscated),
        ] {
            if let Some(v) = v {
                sep(f)?;
                write!(f, "{}{}", if v { "" } else { "!" }, name)?;
            }
        }
        if let Some(c) = &self.click_event {
            sep(f)?;
            write!(f, "clickEvent={c:?}")?;
        }
        if let Some(c) = &self.hover_event {
            sep(f)?;
            write!(f, "hoverEvent={c:?}")?;
        }
        if let Some(c) = &self.insertion {
            sep(f)?;
            write!(f, "insertion={c}")?;
        }
        if let Some(c) = &self.font {
            sep(f)?;
            write!(f, "font={c:?}")?;
        }
        f.write_str("}")
    }
}
