//! `StringTemplate` parsing (`net.minecraft.commands.functions.StringTemplate`) and `ParsedTemplate` as used by dialog
//! inputs and `dynamic/run_command` actions. Only the validation half is needed server side: instantiation happens in the client.

use super::java_letter_or_digit::LETTER_OR_DIGIT;
use mc_dfu::{Codec, DataResult};
use std::sync::LazyLock;

/// `Character.isLetterOrDigit(char)` for one UTF-16 code unit.
fn is_letter_or_digit(unit: u16) -> bool {
    LETTER_OR_DIGIT.binary_search_by(|&(lo, hi)| if unit < lo { std::cmp::Ordering::Greater } else if unit > hi { std::cmp::Ordering::Less } else { std::cmp::Ordering::Equal }).is_ok()
}

/// `StringTemplate.isValidVariableName` (iterates UTF-16 code units like Java `char`s).
pub fn is_valid_variable_name(variable: &str) -> bool {
    variable.encode_utf16().all(|c| is_letter_or_digit(c) || c == u16::from(b'_'))
}

/// `StringTemplate`: literal segments around `$(variable)` placeholders.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StringTemplate {
    pub segments: Vec<String>,
    pub variables: Vec<String>,
}

impl StringTemplate {
    /// `fromString`; `Err` carries the `IllegalArgumentException` message.
    pub fn from_string(input: &str) -> Result<StringTemplate, String> {
        let units: Vec<u16> = input.encode_utf16().collect();
        let (dollar, open, close) = (u16::from(b'$'), u16::from(b'('), u16::from(b')'));
        let find = |c: u16, from: usize| units[from.min(units.len())..].iter().position(|u| *u == c).map(|p| p + from);
        let text = |a: usize, b: usize| String::from_utf16_lossy(&units[a..b]);
        let (mut segments, mut variables) = (Vec::new(), Vec::new());
        let mut start = 0;
        let mut index = find(dollar, 0);
        while let Some(i) = index {
            if i != units.len() - 1 && units[i + 1] == open {
                segments.push(text(start, i));
                let Some(end) = find(close, i + 1) else { return Err("Unterminated macro variable".into()) };
                let variable = text(i + 2, end);
                if !is_valid_variable_name(&variable) {
                    return Err(format!("Invalid macro variable name '{variable}'"));
                }
                variables.push(variable);
                start = end + 1;
                index = find(dollar, start);
            } else {
                index = find(dollar, i + 1);
            }
        }
        if start == 0 {
            return Err("No variables in macro".into());
        }
        if start != units.len() {
            segments.push(text(start, units.len()));
        }
        Ok(StringTemplate { segments, variables })
    }
}

/// `ParsedTemplate`: the raw template string plus its parse (the codec's value, written back as the raw string).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParsedTemplate {
    raw: String,
    parsed: StringTemplate,
}

impl ParsedTemplate {
    pub fn raw(&self) -> &str {
        &self.raw
    }
    pub fn parsed(&self) -> &StringTemplate {
        &self.parsed
    }

    /// `ParsedTemplate.CODEC`.
    pub fn codec() -> Codec<ParsedTemplate> {
        mc_dfu::STRING.comap_flat_map(
            |value| match StringTemplate::from_string(&value) {
                Ok(parsed) => DataResult::success(ParsedTemplate { raw: value, parsed }),
                Err(e) => DataResult::error(move || format!("Failed to parse template {value}: {e}")),
            },
            |t: &ParsedTemplate| t.raw.clone(),
        )
    }

    /// `ParsedTemplate.VARIABLE_CODEC`.
    pub fn variable_codec() -> Codec<String> {
        static CODEC: LazyLock<Codec<String>> = LazyLock::new(|| {
            mc_dfu::STRING.validate(|s| {
                if is_valid_variable_name(&s) { DataResult::success(s) } else { DataResult::error(move || format!("{s} is not a valid input name")) }
            })
        });
        CODEC.clone()
    }
}
