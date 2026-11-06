//! `net.minecraft.network.chat.numbers`: `NumberFormat` (`BlankFormat`, `FixedFormat`, `StyledFormat`) and its
//! registry-dispatched NBT codec (`{type: "minecraft:fixed", ...}`).

use mc_nbt::{CompoundTag, Tag};
use mc_text::{ChatFormatting, Component, Style};

use crate::util::codec::{self, DataResult};

#[derive(Debug, Clone, PartialEq)]
pub enum NumberFormat {
    /// `BlankFormat.INSTANCE`.
    Blank,
    /// `FixedFormat(value)`.
    Fixed(Component),
    /// `StyledFormat(style)`.
    Styled(Style),
}

impl NumberFormat {
    pub fn no_style() -> NumberFormat {
        NumberFormat::Styled(Style::EMPTY)
    }

    /// `StyledFormat.SIDEBAR_DEFAULT`.
    pub fn sidebar_default() -> NumberFormat {
        NumberFormat::Styled(Style::EMPTY.with_color_formatting(Some(ChatFormatting::Red)))
    }

    /// `StyledFormat.PLAYER_LIST_DEFAULT`.
    pub fn player_list_default() -> NumberFormat {
        NumberFormat::Styled(Style::EMPTY.with_color_formatting(Some(ChatFormatting::Yellow)))
    }

    /// `type()` registry key.
    pub fn type_id(&self) -> &'static str {
        match self {
            NumberFormat::Blank => "minecraft:blank",
            NumberFormat::Fixed(_) => "minecraft:fixed",
            NumberFormat::Styled(_) => "minecraft:styled",
        }
    }

    /// `format(value)`.
    pub fn format(&self, value: i32) -> Component {
        match self {
            NumberFormat::Blank => Component::empty(),
            NumberFormat::Fixed(c) => c.copy(),
            NumberFormat::Styled(style) => Component::literal(value.to_string()).with_style(style),
        }
    }

    /// `NumberFormatTypes.CODEC` decode.
    pub fn decode(tag: &Tag) -> DataResult<NumberFormat> {
        let m = codec::compound(tag)?;
        let ty = codec::identifier(codec::field(m, "type")?)?;
        match ty.to_string().as_str() {
            "minecraft:blank" => Ok(NumberFormat::Blank),
            "minecraft:fixed" => {
                let value = codec::field(m, "value")?;
                Component::from_nbt(value)
                    .map(NumberFormat::Fixed)
                    .map_err(|e| e.to_string())
            }
            "minecraft:styled" => Style::from_nbt(tag)
                .map(NumberFormat::Styled)
                .map_err(|e| e.to_string()),
            _ => Err(format!(
                "Unknown registry key in ResourceKey[minecraft:root / minecraft:number_format_type]: {ty}"
            )),
        }
    }

    /// `NumberFormatTypes.CODEC` encode.
    pub fn encode(&self) -> DataResult<Tag> {
        let mut m = match self {
            NumberFormat::Blank => CompoundTag::new(),
            NumberFormat::Fixed(c) => {
                CompoundTag::new().with("value", c.to_nbt().map_err(|e| e.to_string())?)
            }
            NumberFormat::Styled(style) => match style.to_nbt().map_err(|e| e.to_string())? {
                Tag::Compound(c) => c,
                other => return Err(format!("Not a map: {other}")),
            },
        };
        m.put("type", Tag::String(self.type_id().to_string()));
        Ok(Tag::Compound(m))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codec_round_trip_and_format() {
        for (snbt, f) in [
            ("{type:\"minecraft:blank\"}", NumberFormat::Blank),
            (
                "{type:\"minecraft:fixed\",value:\"hi\"}",
                NumberFormat::Fixed(Component::literal("hi")),
            ),
            (
                "{type:\"minecraft:styled\",color:\"red\",bold:1b}",
                NumberFormat::Styled(
                    Style::EMPTY
                        .with_color_formatting(Some(ChatFormatting::Red))
                        .with_bold(Some(true)),
                ),
            ),
        ] {
            let tag = mc_nbt::snbt::parse(snbt).unwrap();
            assert_eq!(NumberFormat::decode(&tag).unwrap(), f, "{snbt}");
            assert_eq!(f.encode().unwrap(), tag, "{snbt}");
        }
        assert_eq!(NumberFormat::Blank.format(5).get_string(), "");
        assert_eq!(
            NumberFormat::Fixed(Component::literal("x"))
                .format(5)
                .get_string(),
            "x"
        );
        assert_eq!(
            NumberFormat::sidebar_default().format(-7).get_string(),
            "-7"
        );
        assert!(
            NumberFormat::decode(&mc_nbt::snbt::parse("{type:\"minecraft:nope\"}").unwrap())
                .is_err()
        );
    }
}
