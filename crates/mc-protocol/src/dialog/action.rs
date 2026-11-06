//! `net.minecraft.server.dialog.action`: what a dialog button does (`Action`, `StaticAction`, `CommandTemplate`, `CustomAll`).

use super::template::ParsedTemplate;
use crate::components::common::{compound_tag_codec, dyn_to_serde, serde_to_dyn};
use crate::components::holder::registry_by_name_codec;
use mc_core::ident::Identifier;
use mc_data::DialogActionType;
use mc_dfu::{Codec, DataResult, Dyn, MapCodec, OpsKind};
use mc_nbt::CompoundTag;
use mc_text::{ClickAction, ClickEvent};
use std::sync::LazyLock;

/// `net.minecraft.server.dialog.action.Action`.
#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    /// `StaticAction`: a fixed click event (everything `ClickEvent.Action.isAllowedFromServer`).
    Static(ClickEvent),
    /// `CommandTemplate` (`dynamic/run_command`).
    CommandTemplate(ParsedTemplate),
    /// `CustomAll` (`dynamic/custom`).
    CustomAll {
        id: Identifier,
        additions: Option<CompoundTag>,
    },
}

/// `StaticAction.WRAPPED_CODECS` key order (`EnumMap` order of the allowed `ClickEvent.Action`s).
fn static_type(action: ClickAction) -> Option<DialogActionType> {
    Some(match action {
        ClickAction::OpenUrl => DialogActionType::OPEN_URL,
        ClickAction::RunCommand => DialogActionType::RUN_COMMAND,
        ClickAction::SuggestCommand => DialogActionType::SUGGEST_COMMAND,
        ClickAction::ShowDialog => DialogActionType::SHOW_DIALOG,
        ClickAction::ChangePage => DialogActionType::CHANGE_PAGE,
        ClickAction::CopyToClipboard => DialogActionType::COPY_TO_CLIPBOARD,
        ClickAction::Custom => DialogActionType::CUSTOM,
        ClickAction::OpenFile => return None,
    })
}

fn static_click_action(t: DialogActionType) -> Option<ClickAction> {
    ClickAction::ALL
        .into_iter()
        .find(|a| static_type(*a) == Some(t))
}

impl Action {
    /// `Action.codec()`'s registry key.
    fn action_type(&self) -> DataResult<DialogActionType> {
        match self {
            Action::Static(event) => match static_type(event.action()) {
                Some(t) => DataResult::success(t),
                None => {
                    let name = event.action().serialized_name().to_ascii_uppercase();
                    DataResult::error(move || format!("Click event type not allowed: {name}"))
                }
            },
            Action::CommandTemplate(_) => {
                DataResult::success(DialogActionType::DYNAMIC_RUN_COMMAND)
            }
            Action::CustomAll { .. } => DataResult::success(DialogActionType::DYNAMIC_CUSTOM),
        }
    }

    /// `StaticAction.WRAPPED_CODECS.get(action)`: `action.valueCodec().xmap(StaticAction::new, StaticAction::value)`. The
    /// click event codec of `mc_text` is the single implementation of the value fields (it reads/writes them together
    /// with the `action` discriminator, which is added/stripped here).
    fn static_codec(action: ClickAction) -> MapCodec<Action> {
        MapCodec::from_fns(
            move |ops, input: &Action, b| {
                let Action::Static(event) = input else { return };
                let encoded: Result<Dyn, String> = match ops.kind() {
                    OpsKind::Json => event
                        .to_json()
                        .map(|v| serde_to_dyn(&v))
                        .map_err(|e| e.to_string()),
                    OpsKind::Nbt => event
                        .to_nbt()
                        .map(|t| Dyn::from(&t))
                        .map_err(|e| e.to_string()),
                    OpsKind::Hash => Err("Unsupported operation".into()),
                };
                match encoded {
                    Ok(Dyn::Map(entries)) => {
                        for (k, v) in entries {
                            if k.as_str() != Some("action") {
                                b.add_key(k, v);
                            }
                        }
                    }
                    Ok(_) => {}
                    Err(e) => {
                        b.with_errors_from::<()>(&DataResult::error(move || e.clone()));
                    }
                }
            },
            move |ops, m| {
                let mut entries = vec![(Dyn::str("action"), Dyn::str(action.serialized_name()))];
                entries.extend(
                    m.entries()
                        .iter()
                        .filter(|(k, _)| k.as_str() != Some("action"))
                        .cloned(),
                );
                let map = Dyn::Map(entries);
                let decoded = match ops.kind() {
                    OpsKind::Json => {
                        ClickEvent::from_json(&dyn_to_serde(&map)).map_err(|e| e.to_string())
                    }
                    OpsKind::Nbt => map
                        .to_tag()
                        .and_then(|t| ClickEvent::from_nbt(&t).map_err(|e| e.to_string())),
                    OpsKind::Hash => Err("Unsupported operation".into()),
                };
                match decoded {
                    Ok(event) => DataResult::success(Action::Static(event)),
                    Err(e) => DataResult::error(move || e.clone()),
                }
            },
            Vec::new,
        )
    }

    fn command_template_codec() -> MapCodec<Action> {
        mc_dfu::record!(CommandTemplateFields {
            template: ParsedTemplate::codec().field_of("template")
        })
        .xmap(
            |f| Action::CommandTemplate(f.template),
            |a: &Action| match a {
                Action::CommandTemplate(t) => CommandTemplateFields {
                    template: t.clone(),
                },
                _ => unreachable!("dispatched by action type"),
            },
        )
    }

    fn custom_all_codec() -> MapCodec<Action> {
        mc_dfu::record!(CustomAllFields {
            id: mc_dfu::extra::IDENTIFIER.field_of("id"),
            additions: compound_tag_codec().optional_field_of("additions"),
        })
        .xmap(
            |f| Action::CustomAll {
                id: f.id,
                additions: f.additions,
            },
            |a: &Action| match a {
                Action::CustomAll { id, additions } => CustomAllFields {
                    id: id.clone(),
                    additions: additions.clone(),
                },
                _ => unreachable!("dispatched by action type"),
            },
        )
    }

    /// `Action.CODEC`: `DIALOG_ACTION_TYPE.byNameCodec().dispatch(Action::codec, c -> c)` (key `type`).
    pub fn codec() -> Codec<Action> {
        static CODEC: LazyLock<Codec<Action>> = LazyLock::new(|| {
            registry_by_name_codec::<DialogActionType>().partial_dispatch(
                "type",
                |a: &Action| a.action_type(),
                |t| {
                    DataResult::success(match *t {
                        DialogActionType::DYNAMIC_RUN_COMMAND => Action::command_template_codec(),
                        DialogActionType::DYNAMIC_CUSTOM => Action::custom_all_codec(),
                        t => match static_click_action(t) {
                            Some(a) => Action::static_codec(a),
                            None => unreachable!("every registered action type is covered"),
                        },
                    })
                },
            )
        });
        CODEC.clone()
    }
}

struct CommandTemplateFields {
    template: ParsedTemplate,
}

struct CustomAllFields {
    id: Identifier,
    additions: Option<CompoundTag>,
}
