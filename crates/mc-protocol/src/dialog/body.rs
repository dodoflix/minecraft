//! `net.minecraft.server.dialog.body`: the content blocks of a dialog (`DialogBody`, `PlainMessage`, `ItemBody`).

use super::Dialog;
use crate::components::holder::registry_by_name_codec;
use crate::components::{ItemStackTemplate, component_codec};
use mc_data::DialogBodyType;
use mc_dfu::extra::extra_int_range;
use mc_dfu::{BOOL, Codec, MapCodec};
use mc_text::Component;
use std::sync::LazyLock;

/// `PlainMessage`.
#[derive(Clone, Debug, PartialEq)]
pub struct PlainMessage {
    pub contents: Component,
    pub width: i32,
}

impl PlainMessage {
    pub const DEFAULT_WIDTH: i32 = 200;

    /// `PlainMessage.MAP_CODEC`.
    pub fn map_codec() -> MapCodec<PlainMessage> {
        mc_dfu::record!(PlainMessage {
            contents: component_codec().field_of("contents"),
            width: Dialog::width_codec().optional_field_of_default("width", Self::DEFAULT_WIDTH),
        })
    }

    /// `PlainMessage.CODEC`: the map form, or a bare component (default width).
    pub fn codec() -> Codec<PlainMessage> {
        Self::map_codec()
            .codec()
            .with_alternative_conv(&component_codec(), |contents| PlainMessage {
                contents,
                width: Self::DEFAULT_WIDTH,
            })
    }
}

/// `ItemBody`.
#[derive(Clone, Debug, PartialEq)]
pub struct ItemBody {
    pub item: ItemStackTemplate,
    pub description: Option<PlainMessage>,
    pub show_decorations: bool,
    pub show_tooltip: bool,
    pub width: i32,
    pub height: i32,
}

impl ItemBody {
    /// `ItemBody.MAP_CODEC`.
    pub fn map_codec() -> MapCodec<ItemBody> {
        mc_dfu::record!(ItemBody {
            item: ItemStackTemplate::codec().field_of("item"),
            description: PlainMessage::codec().optional_field_of("description"),
            show_decorations: BOOL.optional_field_of_default("show_decorations", true),
            show_tooltip: BOOL.optional_field_of_default("show_tooltip", true),
            width: extra_int_range(1, 256).optional_field_of_default("width", 16),
            height: extra_int_range(1, 256).optional_field_of_default("height", 16),
        })
    }
}

/// `DialogBody`.
#[derive(Clone, Debug, PartialEq)]
pub enum DialogBody {
    Item(ItemBody),
    PlainMessage(PlainMessage),
}

impl DialogBody {
    fn body_type(&self) -> DialogBodyType {
        match self {
            DialogBody::Item(_) => DialogBodyType::ITEM,
            DialogBody::PlainMessage(_) => DialogBodyType::PLAIN_MESSAGE,
        }
    }

    /// `DialogBody.DIALOG_BODY_CODEC`: `DIALOG_BODY_TYPE.byNameCodec().dispatch(DialogBody::mapCodec, c -> c)`.
    pub fn codec() -> Codec<DialogBody> {
        static CODEC: LazyLock<Codec<DialogBody>> = LazyLock::new(|| {
            let item = ItemBody::map_codec().xmap(DialogBody::Item, |b: &DialogBody| match b {
                DialogBody::Item(i) => i.clone(),
                _ => unreachable!("dispatched by body type"),
            });
            let message = PlainMessage::map_codec().xmap(
                DialogBody::PlainMessage,
                |b: &DialogBody| match b {
                    DialogBody::PlainMessage(m) => m.clone(),
                    _ => unreachable!("dispatched by body type"),
                },
            );
            registry_by_name_codec::<DialogBodyType>().dispatch(
                "type",
                |b: &DialogBody| b.body_type(),
                move |t| {
                    if *t == DialogBodyType::ITEM {
                        item.clone()
                    } else {
                        message.clone()
                    }
                },
            )
        });
        CODEC.clone()
    }

    /// `DialogBody.COMPACT_LIST_CODEC`.
    pub fn compact_list_codec() -> Codec<Vec<DialogBody>> {
        mc_dfu::extra::compact_list_codec(&Self::codec())
    }
}
