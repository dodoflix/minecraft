//! `net.minecraft.ChatFormatting`: the legacy `§` formatting codes.

use std::fmt;

/// Legacy formatting code prefix (`§`).
pub const PREFIX_CODE: char = '\u{00a7}';

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ChatFormatting {
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
    Obfuscated,
    Bold,
    Strikethrough,
    Underline,
    Italic,
    Reset,
}

impl ChatFormatting {
    pub const ALL: [ChatFormatting; 22] = [
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
        Self::Obfuscated,
        Self::Bold,
        Self::Strikethrough,
        Self::Underline,
        Self::Italic,
        Self::Reset,
    ];

    /// The character after `§`.
    pub const fn code(self) -> char {
        match self {
            Self::Black => '0',
            Self::DarkBlue => '1',
            Self::DarkGreen => '2',
            Self::DarkAqua => '3',
            Self::DarkRed => '4',
            Self::DarkPurple => '5',
            Self::Gold => '6',
            Self::Gray => '7',
            Self::DarkGray => '8',
            Self::Blue => '9',
            Self::Green => 'a',
            Self::Aqua => 'b',
            Self::Red => 'c',
            Self::LightPurple => 'd',
            Self::Yellow => 'e',
            Self::White => 'f',
            Self::Obfuscated => 'k',
            Self::Bold => 'l',
            Self::Strikethrough => 'm',
            Self::Underline => 'n',
            Self::Italic => 'o',
            Self::Reset => 'r',
        }
    }

    /// The enum constant name in lower case (`dark_blue`, `underline`, ...).
    pub const fn name(self) -> &'static str {
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
            Self::Obfuscated => "obfuscated",
            Self::Bold => "bold",
            Self::Strikethrough => "strikethrough",
            Self::Underline => "underline",
            Self::Italic => "italic",
            Self::Reset => "reset",
        }
    }

    /// `ChatFormatting.getByCode`: `Character.toLowerCase(code)` lookup.
    pub fn get_by_code(code: char) -> Option<ChatFormatting> {
        let mut lower = code.to_lowercase();
        let sanitized = match (lower.next(), lower.next()) {
            (Some(c), None) => c,
            _ => code,
        };
        Self::ALL.into_iter().find(|f| f.code() == sanitized)
    }

    /// `ChatFormatting.stripFormatting`: removes `§[0-9A-FK-OR]` (ASCII case-insensitive).
    pub fn strip_formatting(input: &str) -> String {
        let mut out = String::with_capacity(input.len());
        let mut chars = input.chars().peekable();
        while let Some(c) = chars.next() {
            if c == PREFIX_CODE
                && chars
                    .peek()
                    .is_some_and(|n| matches!(n, '0'..='9' | 'a'..='f' | 'A'..='F' | 'k'..='o' | 'K'..='O' | 'r' | 'R'))
            {
                chars.next();
            } else {
                out.push(c);
            }
        }
        out
    }
}

/// `toString()`: `§` followed by the code character.
impl fmt::Display for ChatFormatting {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}{}", PREFIX_CODE, self.code())
    }
}
