//! `net.minecraft.server.dialog.input`: the form controls of a dialog (`Input`, `InputControl` and its four types).

use super::template::ParsedTemplate;
use super::Dialog;
use crate::components::holder::registry_by_name_codec;
use crate::components::component_codec;
use mc_core::math::java::java_double_to_string;
use mc_data::InputControlType;
use mc_dfu::extra::{extra_int_range, POSITIVE_FLOAT, POSITIVE_INT};
use mc_dfu::{java_len, Codec, DataResult, MapCodec, BOOL, FLOAT, STRING};
use mc_text::Component;
use std::sync::LazyLock;

/// `BooleanInput`.
#[derive(Clone, Debug, PartialEq)]
pub struct BooleanInput {
    pub label: Component,
    pub initial: bool,
    pub on_true: String,
    pub on_false: String,
}

impl BooleanInput {
    pub fn map_codec() -> MapCodec<BooleanInput> {
        mc_dfu::record!(BooleanInput {
            label: component_codec().field_of("label"),
            initial: BOOL.optional_field_of_default("initial", false),
            on_true: STRING.optional_field_of_default("on_true", "true".to_owned()),
            on_false: STRING.optional_field_of_default("on_false", "false".to_owned()),
        })
    }
}

/// `NumberRangeInput.RangeInfo`.
#[derive(Clone, Debug, PartialEq)]
pub struct RangeInfo {
    pub start: f32,
    pub end: f32,
    pub initial: Option<f32>,
    pub step: Option<f32>,
}

/// `Math.min(float, float)`.
fn java_min(a: f32, b: f32) -> f32 {
    if a.is_nan() || b.is_nan() {
        f32::NAN
    } else if a == 0.0 && b == 0.0 {
        if a.is_sign_negative() { a } else { b }
    } else if a < b {
        a
    } else {
        b
    }
}

/// `Math.max(float, float)`.
fn java_max(a: f32, b: f32) -> f32 {
    if a.is_nan() || b.is_nan() {
        f32::NAN
    } else if a == 0.0 && b == 0.0 {
        if a.is_sign_negative() { b } else { a }
    } else if a > b {
        a
    } else {
        b
    }
}

impl RangeInfo {
    pub fn map_codec() -> MapCodec<RangeInfo> {
        mc_dfu::record!(RangeInfo {
            start: FLOAT.field_of("start"),
            end: FLOAT.field_of("end"),
            initial: FLOAT.optional_field_of("initial"),
            step: POSITIVE_FLOAT.optional_field_of("step"),
        })
        .validate(|range| {
            if let Some(initial) = range.initial {
                let initial = f64::from(initial);
                let min = f64::from(java_min(range.start, range.end));
                let max = f64::from(java_max(range.start, range.end));
                if initial < min || initial > max {
                    let (i, lo, hi) = (java_double_to_string(initial), java_double_to_string(min), java_double_to_string(max));
                    return DataResult::error(move || format!("Initial value {i} is outside of range [{lo}, {hi}]"));
                }
            }
            DataResult::success(range)
        })
    }
}

/// `NumberRangeInput`.
#[derive(Clone, Debug, PartialEq)]
pub struct NumberRangeInput {
    pub width: i32,
    pub label: Component,
    pub label_format: String,
    pub range_info: RangeInfo,
}

impl NumberRangeInput {
    pub fn map_codec() -> MapCodec<NumberRangeInput> {
        mc_dfu::record!(NumberRangeInput {
            width: Dialog::width_codec().optional_field_of_default("width", 200),
            label: component_codec().field_of("label"),
            label_format: STRING.optional_field_of_default("label_format", "options.generic_value".to_owned()),
            range_info: RangeInfo::map_codec(),
        })
    }
}

/// `SingleOptionInput.Entry`.
#[derive(Clone, Debug, PartialEq)]
pub struct SingleOptionEntry {
    pub id: String,
    pub display: Option<Component>,
    pub initial: bool,
}

impl SingleOptionEntry {
    /// `Entry.CODEC`: the record form, or a bare id string.
    pub fn codec() -> Codec<SingleOptionEntry> {
        let full = mc_dfu::record!(SingleOptionEntry {
            id: STRING.field_of("id"),
            display: component_codec().optional_field_of("display"),
            initial: BOOL.optional_field_of_default("initial", false),
        })
        .codec();
        full.with_alternative_conv(&STRING, |id| SingleOptionEntry { id, display: None, initial: false })
    }
}

/// `SingleOptionInput`.
#[derive(Clone, Debug, PartialEq)]
pub struct SingleOptionInput {
    pub width: i32,
    pub entries: Vec<SingleOptionEntry>,
    pub label: Component,
    pub label_visible: bool,
}

impl SingleOptionInput {
    pub fn map_codec() -> MapCodec<SingleOptionInput> {
        mc_dfu::record!(SingleOptionInput {
            width: Dialog::width_codec().optional_field_of_default("width", 200),
            entries: mc_dfu::extra::non_empty_list(&SingleOptionEntry::codec().list_of()).field_of("options"),
            label: component_codec().field_of("label"),
            label_visible: BOOL.optional_field_of_default("label_visible", true),
        })
        .validate(|o| {
            if o.entries.iter().filter(|e| e.initial).count() > 1 {
                DataResult::error_str("Multiple initial values")
            } else {
                DataResult::success(o)
            }
        })
    }
}

/// `TextInput.MultilineOptions`.
#[derive(Clone, Debug, PartialEq)]
pub struct MultilineOptions {
    pub max_lines: Option<i32>,
    pub height: Option<i32>,
}

impl MultilineOptions {
    pub const MAX_HEIGHT: i32 = 512;

    pub fn codec() -> Codec<MultilineOptions> {
        mc_dfu::record!(MultilineOptions {
            max_lines: POSITIVE_INT.optional_field_of("max_lines"),
            height: extra_int_range(1, Self::MAX_HEIGHT).optional_field_of("height"),
        })
        .codec()
    }
}

/// `TextInput`.
#[derive(Clone, Debug, PartialEq)]
pub struct TextInput {
    pub width: i32,
    pub label: Component,
    pub label_visible: bool,
    pub initial: String,
    pub max_length: i32,
    pub multiline: Option<MultilineOptions>,
}

impl TextInput {
    pub fn map_codec() -> MapCodec<TextInput> {
        mc_dfu::record!(TextInput {
            width: Dialog::width_codec().optional_field_of_default("width", 200),
            label: component_codec().field_of("label"),
            label_visible: BOOL.optional_field_of_default("label_visible", true),
            initial: STRING.optional_field_of_default("initial", String::new()),
            max_length: POSITIVE_INT.optional_field_of_default("max_length", 32),
            multiline: MultilineOptions::codec().optional_field_of("multiline"),
        })
        .validate(|o| {
            if java_len(&o.initial) as i64 > i64::from(o.max_length) {
                DataResult::error_str("Default text length exceeds allowed size")
            } else {
                DataResult::success(o)
            }
        })
    }
}

/// `InputControl`.
#[derive(Clone, Debug, PartialEq)]
pub enum InputControl {
    Boolean(BooleanInput),
    NumberRange(NumberRangeInput),
    SingleOption(SingleOptionInput),
    Text(TextInput),
}

macro_rules! control_codec {
    ($variant:ident, $inner:ty) => {
        <$inner>::map_codec().xmap(InputControl::$variant, |c: &InputControl| match c {
            InputControl::$variant(v) => v.clone(),
            _ => unreachable!("dispatched by control type"),
        })
    };
}

impl InputControl {
    fn control_type(&self) -> InputControlType {
        match self {
            InputControl::Boolean(_) => InputControlType::BOOLEAN,
            InputControl::NumberRange(_) => InputControlType::NUMBER_RANGE,
            InputControl::SingleOption(_) => InputControlType::SINGLE_OPTION,
            InputControl::Text(_) => InputControlType::TEXT,
        }
    }

    /// `InputControl.MAP_CODEC`: `INPUT_CONTROL_TYPE.byNameCodec().dispatchMap(InputControl::mapCodec, c -> c)` (key `type`).
    pub fn map_codec() -> MapCodec<InputControl> {
        static CODEC: LazyLock<MapCodec<InputControl>> = LazyLock::new(|| {
            let boolean = control_codec!(Boolean, BooleanInput);
            let number_range = control_codec!(NumberRange, NumberRangeInput);
            let single_option = control_codec!(SingleOption, SingleOptionInput);
            let text = control_codec!(Text, TextInput);
            registry_by_name_codec::<InputControlType>().dispatch_map(
                "type",
                |c: &InputControl| c.control_type(),
                move |t| match *t {
                    InputControlType::BOOLEAN => boolean.clone(),
                    InputControlType::NUMBER_RANGE => number_range.clone(),
                    InputControlType::SINGLE_OPTION => single_option.clone(),
                    _ => text.clone(),
                },
            )
        });
        CODEC.clone()
    }
}

/// `Input`: a named control.
#[derive(Clone, Debug, PartialEq)]
pub struct Input {
    pub key: String,
    pub control: InputControl,
}

impl Input {
    pub fn codec() -> Codec<Input> {
        mc_dfu::record!(Input {
            key: ParsedTemplate::variable_codec().field_of("key"),
            control: InputControl::map_codec(),
        })
        .codec()
    }
}
