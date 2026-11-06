//! `net.minecraft.network.chat.TextColor`.

use crate::chat_formatting::ChatFormatting;
use std::fmt;

/// An RGB text colour. Equality (like vanilla) compares only the RGB value; the optional name only
/// affects serialisation (`"red"` vs `"#FF5555"`).
#[derive(Clone, Copy, Debug)]
pub struct TextColor {
    value: i32,
    name: Option<&'static str>,
}

macro_rules! named {
    ($($konst:ident, $name:literal, $rgb:literal;)*) => {
        $(pub const $konst: TextColor = TextColor { value: $rgb, name: Some($name) };)*
        const NAMED_COLORS: &[TextColor] = &[$(TextColor::$konst),*];
    };
}

impl TextColor {
    named! {
        BLACK, "black", 0;
        DARK_BLUE, "dark_blue", 170;
        DARK_GREEN, "dark_green", 43520;
        DARK_AQUA, "dark_aqua", 43690;
        DARK_RED, "dark_red", 11141120;
        DARK_PURPLE, "dark_purple", 11141290;
        GOLD, "gold", 16755200;
        GRAY, "gray", 11184810;
        DARK_GRAY, "dark_gray", 5592405;
        BLUE, "blue", 5592575;
        GREEN, "green", 5635925;
        AQUA, "aqua", 5636095;
        RED, "red", 16733525;
        LIGHT_PURPLE, "light_purple", 16733695;
        YELLOW, "yellow", 16777045;
        WHITE, "white", 16777215;
    }

    pub const fn from_rgb(rgb: i32) -> TextColor {
        TextColor {
            value: rgb & 0xFFFFFF,
            name: None,
        }
    }

    pub const fn value(&self) -> i32 {
        self.value
    }

    /// `serialize()`: the colour name, or `#RRGGBB`.
    pub fn serialize(&self) -> String {
        match self.name {
            Some(n) => n.to_owned(),
            None => format!("#{:06X}", self.value),
        }
    }

    pub fn from_legacy_format(format: ChatFormatting) -> Option<TextColor> {
        use ChatFormatting::*;
        Some(match format {
            Black => Self::BLACK,
            DarkBlue => Self::DARK_BLUE,
            DarkGreen => Self::DARK_GREEN,
            DarkAqua => Self::DARK_AQUA,
            DarkRed => Self::DARK_RED,
            DarkPurple => Self::DARK_PURPLE,
            Gold => Self::GOLD,
            Gray => Self::GRAY,
            DarkGray => Self::DARK_GRAY,
            Blue => Self::BLUE,
            Green => Self::GREEN,
            Aqua => Self::AQUA,
            Red => Self::RED,
            LightPurple => Self::LIGHT_PURPLE,
            Yellow => Self::YELLOW,
            White => Self::WHITE,
            _ => return None,
        })
    }

    /// `TextColor.parseColor` (the `TextColor.CODEC` decoder). Errors carry vanilla's message.
    pub fn parse_color(color: &str) -> Result<TextColor, String> {
        if let Some(hex) = color.strip_prefix('#') {
            match java_parse_int_radix16(hex) {
                None => Err(format!("Invalid color value: {color}")),
                Some(v) if (0..=0xFFFFFF).contains(&v) => Ok(Self::from_rgb(v)),
                Some(_) => Err(format!("Color value out of range: {color}")),
            }
        } else {
            Self::NAMED_COLORS
                .iter()
                .find(|c| c.name == Some(color))
                .copied()
                .ok_or_else(|| format!("Invalid color name: {color}"))
        }
    }
}

/// `Integer.parseInt(s, 16)` (ASCII digits only): optional sign, at least one digit, must fit in `i32`.
fn java_parse_int_radix16(s: &str) -> Option<i32> {
    let (neg, digits) = match s.as_bytes().first()? {
        b'-' => (true, &s[1..]),
        b'+' => (false, &s[1..]),
        _ => (false, s),
    };
    if digits.is_empty() {
        return None;
    }
    let mut acc: i64 = 0;
    for c in digits.chars() {
        acc = acc * 16 + c.to_digit(16)? as i64;
        if acc > 0x8000_0000 {
            return None;
        }
    }
    let v = if neg { -acc } else { acc };
    i32::try_from(v).ok()
}

impl PartialEq for TextColor {
    fn eq(&self, other: &Self) -> bool {
        self.value == other.value
    }
}
impl Eq for TextColor {}

impl std::hash::Hash for TextColor {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.value.hash(state);
    }
}

impl fmt::Display for TextColor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.serialize())
    }
}
