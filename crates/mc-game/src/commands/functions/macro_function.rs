//! `MacroFunction`: a function with `$(variable)` lines, instantiated per distinct argument list.

use super::string_template::StringTemplate;
use super::{CommandFunction, FunctionInstantiationException, InstantiatedFunction, parse_command};
use crate::commands::execution::UnboundEntryAction;
use crate::commands::CommandSourceStack;
use mc_brigadier::{CommandDispatcher, StringReader};
use mc_core::ident::Identifier;
use mc_nbt::{CompoundTag, Tag};
use mc_text::Component;
use std::cell::RefCell;
use std::sync::Arc;

const MAX_CACHE_ENTRIES: usize = 8;

/// `MacroFunction.Entry`.
pub enum MacroEntry {
    PlainText(Arc<dyn UnboundEntryAction>),
    Macro { template: StringTemplate, parameters: Vec<usize>, compilation_context: CommandSourceStack },
}

impl MacroEntry {
    fn parameters(&self) -> &[usize] {
        match self {
            MacroEntry::PlainText(_) => &[],
            MacroEntry::Macro { parameters, .. } => parameters,
        }
    }

    fn instantiate(
        &self,
        substitutions: &[String],
        dispatcher: &CommandDispatcher<CommandSourceStack>,
        function_id: &Identifier,
    ) -> Result<Arc<dyn UnboundEntryAction>, FunctionInstantiationException> {
        match self {
            MacroEntry::PlainText(action) => Ok(action.clone()),
            MacroEntry::Macro { template, compilation_context, .. } => {
                let command = template.substitute(substitutions).map_err(|e| {
                    // `IllegalStateException` (command too long) escapes in vanilla; reported as a failed instantiation here.
                    FunctionInstantiationException::new(Component::literal(e))
                })?;
                parse_command(dispatcher, compilation_context, StringReader::new(command.clone())).map_err(|e| {
                    FunctionInstantiationException::new(crate::translatable!(
                        "commands.function.error.parse",
                        Component::literal(function_id.to_string()),
                        command.clone(),
                        e.get_message()
                    ))
                })
            }
        }
    }
}

pub struct MacroFunction {
    parameters: Vec<String>,
    cache: RefCell<Vec<(Vec<String>, Arc<InstantiatedFunction>)>>,
    id: Identifier,
    entries: Vec<MacroEntry>,
}

impl MacroFunction {
    pub fn new(id: Identifier, entries: Vec<MacroEntry>, parameters: Vec<String>) -> Self {
        Self { parameters, cache: RefCell::new(Vec::new()), id, entries }
    }
}

/// `DecimalFormat("#")` with up to 15 fraction digits (HALF_EVEN on the shortest decimal representation of the double).
fn format_decimal(value: f64) -> String {
    if value.is_nan() {
        return "NaN".to_owned();
    }
    if value.is_infinite() {
        return if value < 0.0 { "-∞".to_owned() } else { "∞".to_owned() };
    }
    let negative = value.is_sign_negative();
    let shortest = format!("{}", value.abs());
    let (int_part, frac_part) = shortest.split_once('.').unwrap_or((&shortest, ""));
    let mut int_digits: Vec<u8> = int_part.bytes().map(|b| b - b'0').collect();
    let mut frac_digits: Vec<u8> = frac_part.bytes().map(|b| b - b'0').collect();
    if frac_digits.len() > 15 {
        let round_digit = frac_digits[15];
        let rest_nonzero = frac_digits[16..].iter().any(|d| *d != 0);
        frac_digits.truncate(15);
        let last_odd = frac_digits.last().or(int_digits.last()).is_some_and(|d| d % 2 == 1);
        if round_digit > 5 || (round_digit == 5 && (rest_nonzero || last_odd)) {
            let mut carry = true;
            for d in frac_digits.iter_mut().rev().chain(int_digits.iter_mut().rev()) {
                if !carry {
                    break;
                }
                if *d == 9 {
                    *d = 0;
                } else {
                    *d += 1;
                    carry = false;
                }
            }
            if carry {
                int_digits.insert(0, 1);
            }
        }
    }
    while frac_digits.last() == Some(&0) {
        frac_digits.pop();
    }
    let int_s: String = int_digits.iter().map(|d| (b'0' + d) as char).collect();
    let int_s = match int_s.trim_start_matches('0') {
        "" => "0".to_owned(),
        t => t.to_owned(),
    };
    let body = if frac_digits.is_empty() {
        int_s
    } else {
        format!("{int_s}.{}", frac_digits.iter().map(|d| (b'0' + d) as char).collect::<String>())
    };
    if negative { format!("-{body}") } else { body }
}

/// `MacroFunction.stringify(tag)`.
fn stringify(tag: &Tag) -> String {
    match tag {
        Tag::Float(v) => format_decimal(f64::from(*v)),
        Tag::Double(v) => format_decimal(*v),
        Tag::Byte(v) => i32::from(*v).to_string(),
        Tag::Short(v) => i32::from(*v).to_string(),
        Tag::Long(v) => v.to_string(),
        Tag::String(s) => s.clone(),
        other => other.to_string(),
    }
}

impl CommandFunction for MacroFunction {
    fn id(&self) -> &Identifier {
        &self.id
    }

    fn instantiate(
        &self,
        arguments: Option<&CompoundTag>,
        dispatcher: &CommandDispatcher<CommandSourceStack>,
    ) -> Result<Arc<InstantiatedFunction>, FunctionInstantiationException> {
        let Some(arguments) = arguments else {
            return Err(FunctionInstantiationException::new(crate::translatable!(
                "commands.function.error.missing_arguments",
                Component::literal(self.id.to_string())
            )));
        };
        let mut parameter_values = Vec::with_capacity(self.parameters.len());
        for argument in &self.parameters {
            let Some(value) = arguments.get(argument) else {
                return Err(FunctionInstantiationException::new(crate::translatable!(
                    "commands.function.error.missing_argument",
                    Component::literal(self.id.to_string()),
                    argument.clone()
                )));
            };
            parameter_values.push(stringify(value));
        }
        {
            let mut cache = self.cache.borrow_mut();
            if let Some(pos) = cache.iter().position(|(k, _)| *k == parameter_values) {
                let entry = cache.remove(pos);
                let function = entry.1.clone();
                cache.push(entry);
                return Ok(function);
            }
            if cache.len() >= MAX_CACHE_ENTRIES {
                cache.remove(0);
            }
        }
        let function = self.substitute_and_parse(&parameter_values, dispatcher)?;
        self.cache.borrow_mut().push((parameter_values, function.clone()));
        Ok(function)
    }
}

impl MacroFunction {
    fn substitute_and_parse(
        &self,
        values: &[String],
        dispatcher: &CommandDispatcher<CommandSourceStack>,
    ) -> Result<Arc<InstantiatedFunction>, FunctionInstantiationException> {
        let mut new_entries = Vec::with_capacity(self.entries.len());
        for entry in &self.entries {
            let entry_arguments: Vec<String> = entry.parameters().iter().map(|&i| values[i].clone()).collect();
            new_entries.push(entry.instantiate(&entry_arguments, dispatcher, &self.id)?);
        }
        // `id.withPath(id -> id + "/" + keys.hashCode())` where `keys` is the `List<String>` of parameter names.
        let mut hash: i32 = 1;
        for key in &self.parameters {
            hash = hash.wrapping_mul(31).wrapping_add(mc_brigadier::utf16::java_hash(key));
        }
        let id = self.id.with_path(&format!("{}/{}", self.id.path(), hash)).unwrap_or_else(|_| self.id.clone());
        Ok(Arc::new(InstantiatedFunction { id, entries: new_entries }))
    }
}

#[cfg(test)]
mod tests {
    use super::format_decimal;

    #[test]
    fn decimal_format_matches_java() {
        assert_eq!(format_decimal(1.0), "1");
        assert_eq!(format_decimal(1.5), "1.5");
        assert_eq!(format_decimal(0.1_f32 as f64), "0.100000001490116");
        assert_eq!(format_decimal(-2.25), "-2.25");
        assert_eq!(format_decimal(1e21), "1000000000000000000000");
        assert_eq!(format_decimal(0.0), "0");
    }
}
