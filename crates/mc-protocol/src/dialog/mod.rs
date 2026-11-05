//! `net.minecraft.server.dialog`: the `dialog` registry's value type with every codec vanilla defines for it
//! (`DIRECT_CODEC` / `CODEC` / `LIST_CODEC` over NBT and JSON, `STREAM_CODEC`, `CONTEXT_FREE_STREAM_CODEC`).
//!
//! Registry references resolve through [`crate::registry_context`] (see [`crate::registry_context::with_scoped_lookup`]
//! for loading a registry whose entries reference each other).

pub mod action;
pub mod body;
pub mod input;
mod java_letter_or_digit;
pub mod template;

use crate::codec::{decode_tag_with, encode_tag, Decode, DecodeError, DecodeResult, Encode, NbtLimit};
use crate::components::holder::{self, registry_by_name_codec, Holder, HolderSet};
use crate::components::component_codec;
pub use action::Action;
pub use body::{DialogBody, ItemBody, PlainMessage};
pub use input::{Input, InputControl};
use mc_data::DialogType;
use mc_dfu::extra::{extra_int_range, POSITIVE_INT};
use mc_dfu::{Codec, DataResult, MapCodec, BOOL};
use mc_text::Component;
use std::sync::LazyLock;

pub const REGISTRY: &str = "minecraft:dialog";

crate::string_enum! {
    /// `DialogAction`: what happens after the user acted in the dialog.
    pub enum DialogAction { Close = "close", None = "none", WaitForResponse = "wait_for_response" }
}

impl DialogAction {
    /// `willUnpause`.
    pub fn will_unpause(self) -> bool {
        matches!(self, DialogAction::Close | DialogAction::WaitForResponse)
    }
}

/// `CommonDialogData`.
#[derive(Clone, Debug, PartialEq)]
pub struct CommonDialogData {
    pub title: Component,
    pub external_title: Option<Component>,
    pub can_close_with_escape: bool,
    pub pause: bool,
    pub after_action: DialogAction,
    pub body: Vec<DialogBody>,
    pub inputs: Vec<Input>,
}

impl CommonDialogData {
    /// `CommonDialogData.MAP_CODEC`.
    pub fn map_codec() -> MapCodec<CommonDialogData> {
        mc_dfu::record!(CommonDialogData {
            title: component_codec().field_of("title"),
            external_title: component_codec().optional_field_of("external_title"),
            can_close_with_escape: BOOL.optional_field_of_default("can_close_with_escape", true),
            pause: BOOL.optional_field_of_default("pause", true),
            after_action: DialogAction::codec().optional_field_of_default("after_action", DialogAction::Close),
            body: DialogBody::compact_list_codec().optional_field_of_default("body", Vec::new()),
            inputs: Input::codec().list_of().optional_field_of_default("inputs", Vec::new()),
        })
        .validate(|data| {
            if data.pause && !data.after_action.will_unpause() {
                DataResult::error_str("Dialogs that pause the game must use after_action values that unpause it after user action!")
            } else {
                DataResult::success(data)
            }
        })
    }

    /// `computeExternalTitle`.
    pub fn compute_external_title(&self) -> &Component {
        self.external_title.as_ref().unwrap_or(&self.title)
    }
}

/// `CommonButtonData`.
#[derive(Clone, Debug, PartialEq)]
pub struct CommonButtonData {
    pub label: Component,
    pub tooltip: Option<Component>,
    pub width: i32,
}

impl CommonButtonData {
    pub const DEFAULT_WIDTH: i32 = 150;

    pub fn new(label: Component, width: i32) -> CommonButtonData {
        CommonButtonData { label, tooltip: None, width }
    }

    pub fn map_codec() -> MapCodec<CommonButtonData> {
        mc_dfu::record!(CommonButtonData {
            label: component_codec().field_of("label"),
            tooltip: component_codec().optional_field_of("tooltip"),
            width: Dialog::width_codec().optional_field_of_default("width", Self::DEFAULT_WIDTH),
        })
    }
}

/// `ActionButton`.
#[derive(Clone, Debug, PartialEq)]
pub struct ActionButton {
    pub button: CommonButtonData,
    pub action: Option<Action>,
}

impl ActionButton {
    pub fn codec() -> Codec<ActionButton> {
        mc_dfu::record!(ActionButton {
            button: CommonButtonData::map_codec(),
            action: Action::codec().optional_field_of("action"),
        })
        .codec()
    }
}

/// `NoticeDialog`.
#[derive(Clone, Debug, PartialEq)]
pub struct NoticeDialog {
    pub common: CommonDialogData,
    pub action: ActionButton,
}

impl NoticeDialog {
    /// `DEFAULT_ACTION`: an "OK" button.
    pub fn default_action() -> ActionButton {
        ActionButton { button: CommonButtonData::new(Component::translatable("gui.ok", vec![]), 150), action: None }
    }
}

/// `ConfirmationDialog`.
#[derive(Clone, Debug, PartialEq)]
pub struct ConfirmationDialog {
    pub common: CommonDialogData,
    pub yes: ActionButton,
    pub no: ActionButton,
}

/// `DialogListDialog`.
#[derive(Clone, Debug, PartialEq)]
pub struct DialogListDialog {
    pub common: CommonDialogData,
    pub dialogs: HolderSet<Dialog>,
    pub exit_action: Option<ActionButton>,
    pub columns: i32,
    pub button_width: i32,
}

/// `MultiActionDialog`.
#[derive(Clone, Debug, PartialEq)]
pub struct MultiActionDialog {
    pub common: CommonDialogData,
    pub actions: Vec<ActionButton>,
    pub exit_action: Option<ActionButton>,
    pub columns: i32,
}

/// `ServerLinksDialog`.
#[derive(Clone, Debug, PartialEq)]
pub struct ServerLinksDialog {
    pub common: CommonDialogData,
    pub exit_action: Option<ActionButton>,
    pub columns: i32,
    pub button_width: i32,
}

/// `net.minecraft.server.dialog.Dialog`.
#[derive(Clone, Debug, PartialEq)]
pub enum Dialog {
    Notice(NoticeDialog),
    ServerLinks(ServerLinksDialog),
    DialogList(DialogListDialog),
    MultiAction(MultiActionDialog),
    Confirmation(ConfirmationDialog),
}

macro_rules! variant_codec {
    ($variant:ident, $codec:expr) => {
        $codec.xmap(Dialog::$variant, |d: &Dialog| match d {
            Dialog::$variant(v) => v.clone(),
            _ => unreachable!("dispatched by dialog type"),
        })
    };
}

impl Dialog {
    /// `Dialog.WIDTH_CODEC`.
    pub fn width_codec() -> Codec<i32> {
        extra_int_range(1, 1024)
    }

    pub fn common(&self) -> &CommonDialogData {
        match self {
            Dialog::Notice(d) => &d.common,
            Dialog::ServerLinks(d) => &d.common,
            Dialog::DialogList(d) => &d.common,
            Dialog::MultiAction(d) => &d.common,
            Dialog::Confirmation(d) => &d.common,
        }
    }

    /// `onCancel`.
    pub fn on_cancel(&self) -> Option<&Action> {
        match self {
            Dialog::Notice(d) => d.action.action.as_ref(),
            Dialog::Confirmation(d) => d.no.action.as_ref(),
            Dialog::ServerLinks(d) => d.exit_action.as_ref().and_then(|b| b.action.as_ref()),
            Dialog::DialogList(d) => d.exit_action.as_ref().and_then(|b| b.action.as_ref()),
            Dialog::MultiAction(d) => d.exit_action.as_ref().and_then(|b| b.action.as_ref()),
        }
    }

    fn dialog_type(&self) -> DialogType {
        match self {
            Dialog::Notice(_) => DialogType::NOTICE,
            Dialog::ServerLinks(_) => DialogType::SERVER_LINKS,
            Dialog::DialogList(_) => DialogType::DIALOG_LIST,
            Dialog::MultiAction(_) => DialogType::MULTI_ACTION,
            Dialog::Confirmation(_) => DialogType::CONFIRMATION,
        }
    }

    fn notice_codec() -> MapCodec<Dialog> {
        variant_codec!(
            Notice,
            mc_dfu::record!(NoticeDialog {
                common: CommonDialogData::map_codec(),
                action: ActionButton::codec().optional_field_of_default("action", NoticeDialog::default_action()),
            })
        )
    }

    fn server_links_codec() -> MapCodec<Dialog> {
        variant_codec!(
            ServerLinks,
            mc_dfu::record!(ServerLinksDialog {
                common: CommonDialogData::map_codec(),
                exit_action: ActionButton::codec().optional_field_of("exit_action"),
                columns: POSITIVE_INT.optional_field_of_default("columns", 2),
                button_width: Self::width_codec().optional_field_of_default("button_width", 150),
            })
        )
    }

    fn dialog_list_codec() -> MapCodec<Dialog> {
        variant_codec!(
            DialogList,
            mc_dfu::record!(DialogListDialog {
                common: CommonDialogData::map_codec(),
                dialogs: Self::list_codec().field_of("dialogs"),
                exit_action: ActionButton::codec().optional_field_of("exit_action"),
                columns: POSITIVE_INT.optional_field_of_default("columns", 2),
                button_width: Self::width_codec().optional_field_of_default("button_width", 150),
            })
        )
    }

    fn multi_action_codec() -> MapCodec<Dialog> {
        variant_codec!(
            MultiAction,
            mc_dfu::record!(MultiActionDialog {
                common: CommonDialogData::map_codec(),
                actions: mc_dfu::extra::non_empty_list(&ActionButton::codec().list_of()).field_of("actions"),
                exit_action: ActionButton::codec().optional_field_of("exit_action"),
                columns: POSITIVE_INT.optional_field_of_default("columns", 2),
            })
        )
    }

    fn confirmation_codec() -> MapCodec<Dialog> {
        variant_codec!(
            Confirmation,
            mc_dfu::record!(ConfirmationDialog {
                common: CommonDialogData::map_codec(),
                yes: ActionButton::codec().field_of("yes"),
                no: ActionButton::codec().field_of("no"),
            })
        )
    }

    /// `Dialog.DIRECT_CODEC`: `DIALOG_TYPE.byNameCodec().dispatch(Dialog::codec, c -> c)` (key `type`).
    pub fn direct_codec() -> Codec<Dialog> {
        static CODEC: LazyLock<Codec<Dialog>> = LazyLock::new(|| {
            registry_by_name_codec::<DialogType>().dispatch("type", |d: &Dialog| d.dialog_type(), |t| match *t {
                DialogType::NOTICE => Dialog::notice_codec(),
                DialogType::SERVER_LINKS => Dialog::server_links_codec(),
                DialogType::DIALOG_LIST => Dialog::dialog_list_codec(),
                DialogType::MULTI_ACTION => Dialog::multi_action_codec(),
                _ => Dialog::confirmation_codec(),
            })
        });
        CODEC.clone()
    }

    /// `Dialog.CODEC`: `RegistryCodecs.holder(DIALOG, DIRECT_CODEC)` (a registry id or an inline dialog).
    pub fn codec() -> Codec<Holder<Dialog>> {
        static CODEC: LazyLock<Codec<Holder<Dialog>>> =
            LazyLock::new(|| holder::holder_file_codec(REGISTRY, &Codec::lazy(Dialog::direct_codec), true));
        CODEC.clone()
    }

    /// `Dialog.LIST_CODEC`: `RegistryCodecs.holderSet(DIALOG, DIRECT_CODEC)`.
    pub fn list_codec() -> Codec<HolderSet<Dialog>> {
        static CODEC: LazyLock<Codec<HolderSet<Dialog>>> =
            LazyLock::new(|| holder::holder_set_codec(REGISTRY, &Dialog::codec(), false));
        CODEC.clone()
    }
}

impl Dialog {
    /// `Dialog.DIRECT_CODEC.parse(NbtOps, tag)`: the typed dialog of an inline (SNBT / NBT) definition.
    pub fn from_inline_tag(tag: &mc_nbt::Tag) -> Result<Dialog, String> {
        mc_dfu::parse_nbt(&Self::direct_codec(), tag).into_result()
    }

    /// `Dialog.DIRECT_CODEC.encodeStart(NbtOps, dialog)`: the normalised NBT vanilla puts on the wire.
    pub fn to_tag(&self) -> Result<mc_nbt::Tag, String> {
        mc_dfu::encode_nbt(&Self::direct_codec(), self).into_result()
    }

    /// The wire holder of `ByteBufCodecs.holder(DIALOG, ..)`: a reference by name becomes its raw protocol id in the
    /// installed `dialog` registry (`None` if it is not registered), an inline dialog stays inline.
    pub fn wire_holder(holder: &Holder<Dialog>) -> Option<crate::codec::Holder<Dialog>> {
        match holder {
            Holder::Reference(name) => crate::registry_context::registry_id(REGISTRY, name).map(crate::codec::Holder::Reference),
            Holder::Direct(d) => Some(crate::codec::Holder::Direct(d.clone())),
        }
    }
}

/// `Dialog.CONTEXT_FREE_STREAM_CODEC` (`ByteBufCodecs.fromCodecTrusted(DIRECT_CODEC)`); also the direct branch of
/// `Dialog.STREAM_CODEC` (`fromCodecWithRegistriesTrusted`): the dialog as a network NBT root, read with an unlimited quota.
///
/// Encoding panics when the dialog cannot be encoded (vanilla throws `EncoderException`).
impl Encode for Dialog {
    fn encode(&self, out: &mut Vec<u8>) {
        match mc_dfu::encode_nbt(&Self::direct_codec(), self) {
            DataResult::Success(tag) => encode_tag(&tag, out),
            e => panic!("Failed to encode: {} {self:?}", e.error_message().unwrap_or_default()),
        }
    }
}

impl Decode for Dialog {
    fn decode(input: &mut &[u8]) -> DecodeResult<Dialog> {
        let tag = decode_tag_with(input, NbtLimit::UNLIMITED)?.ok_or_else(|| DecodeError::msg("Expected non-null compound tag"))?;
        match mc_dfu::parse_nbt(&Self::direct_codec(), &tag) {
            DataResult::Success(d) => Ok(d),
            e => Err(DecodeError::Message(format!("Failed to decode: {} {tag}", e.error_message().unwrap_or_default()))),
        }
    }
}
