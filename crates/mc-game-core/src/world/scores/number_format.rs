//! `net.minecraft.network.chat.numbers`: `NumberFormat` (`BlankFormat`, `FixedFormat`, `StyledFormat`) and its
//! registry-dispatched NBT codec (`{type: "minecraft:fixed", ...}`).

use mc_core::ident::Identifier;
use mc_dfu::extra::IDENTIFIER;
use mc_dfu::{Codec, DataResult, Dyn, MapCodec, NBT};
use mc_nbt::Tag;
use mc_protocol::components::component_codec;
use mc_text::{ChatFormatting, Component, Style};

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

    /// `NumberFormatTypes.CODEC`: the registered type under `type`, the fields of the format beside it.
    pub fn codec() -> Codec<NumberFormat> {
        let kind = IDENTIFIER.flat_xmap(
            |id| match id.to_string().as_str() {
                "minecraft:blank" | "minecraft:fixed" | "minecraft:styled" => DataResult::success(id),
                _ => DataResult::error(move || {
                    format!(
                        "Unknown registry key in ResourceKey[minecraft:root / minecraft:number_format_type]: {id}"
                    )
                }),
            },
            |id: &Identifier| DataResult::success(id.clone()),
        );
        kind.dispatch_map(
            "type",
            |format: &NumberFormat| Identifier::parse(format.type_id()).expect("static id"),
            |kind| match kind.path() {
                "blank" => MapCodec::unit(NumberFormat::Blank),
                "fixed" => component_codec().field_of("value").xmap(
                    NumberFormat::Fixed,
                    |format: &NumberFormat| match format {
                        NumberFormat::Fixed(value) => value.clone(),
                        _ => Component::empty(),
                    },
                ),
                _ => styled_codec(),
            },
        )
        .codec()
    }
}

/// `StyledFormat.MAP_CODEC`: the style fields of the surrounding map.
fn styled_codec() -> MapCodec<NumberFormat> {
    MapCodec::from_fns(
        |_, format: &NumberFormat, builder| {
            if let NumberFormat::Styled(style) = format {
                match style.to_nbt() {
                    Ok(Tag::Compound(fields)) => {
                        for (key, value) in fields.iter() {
                            builder.add(key.as_lossy_str(), Dyn::from(value));
                        }
                    }
                    Ok(_) => {}
                    Err(e) => {
                        let message = e.to_string();
                        builder.add_result("style", DataResult::error(move || message.clone()));
                    }
                }
            }
        },
        |ops, input| {
            let map = ops.create_map(input.entries().to_vec());
            match ops.convert_to(&NBT, &map).to_tag() {
                Ok(tag) => match Style::from_nbt(&tag) {
                    Ok(style) => DataResult::success(NumberFormat::Styled(style)),
                    Err(e) => {
                        let message = e.to_string();
                        DataResult::error(move || message.clone())
                    }
                },
                Err(e) => {
                    let message = e.to_string();
                    DataResult::error(move || message.clone())
                }
            }
        },
        Vec::new,
    )
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
            let codec = NumberFormat::codec();
            assert_eq!(
                mc_dfu::parse_nbt(&codec, &tag).into_result().unwrap(),
                f,
                "{snbt}"
            );
            assert_eq!(
                mc_dfu::encode_nbt(&codec, &f).into_result().unwrap(),
                tag,
                "{snbt}"
            );
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
            mc_dfu::parse_nbt(
                &NumberFormat::codec(),
                &mc_nbt::snbt::parse("{type:\"minecraft:nope\"}").unwrap()
            )
            .is_error()
        );
    }
}
