//! `FunctionBuilder`.

use super::macro_function::{MacroEntry, MacroFunction};
use super::string_template::StringTemplate;
use super::{CommandFunction, InstantiatedFunction};
use crate::commands::execution::UnboundEntryAction;
use crate::commands::CommandSourceStack;
use mc_core::ident::Identifier;
use std::sync::Arc;

pub struct FunctionBuilder {
    plain_entries: Option<Vec<Arc<dyn UnboundEntryAction>>>,
    macro_entries: Option<Vec<MacroEntry>>,
    macro_arguments: Vec<String>,
}

impl Default for FunctionBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl FunctionBuilder {
    pub fn new() -> Self {
        Self { plain_entries: Some(Vec::new()), macro_entries: None, macro_arguments: Vec::new() }
    }

    pub fn add_command(&mut self, command: Arc<dyn UnboundEntryAction>) {
        match &mut self.macro_entries {
            Some(entries) => entries.push(MacroEntry::PlainText(command)),
            None => self.plain_entries.as_mut().expect("plain entries").push(command),
        }
    }

    fn get_argument_index(&mut self, id: &str) -> usize {
        match self.macro_arguments.iter().position(|a| a == id) {
            Some(i) => i,
            None => {
                self.macro_arguments.push(id.to_owned());
                self.macro_arguments.len() - 1
            }
        }
    }

    pub fn add_macro(&mut self, command: &str, line: usize, compilation_context: &CommandSourceStack) -> Result<(), String> {
        let parse_results = StringTemplate::from_string(command).map_err(|_| format!("Can't parse function line {line}: '{command}'"))?;
        if let Some(plain) = self.plain_entries.take() {
            let mut entries = Vec::with_capacity(plain.len() + 1);
            entries.extend(plain.into_iter().map(MacroEntry::PlainText));
            self.macro_entries = Some(entries);
        }
        let parameters = parse_results.variables.clone().iter().map(|v| self.get_argument_index(v)).collect();
        self.macro_entries.as_mut().expect("macro entries").push(MacroEntry::Macro {
            template: parse_results,
            parameters,
            compilation_context: compilation_context.clone(),
        });
        Ok(())
    }

    pub fn build(self, id: Identifier) -> Arc<dyn CommandFunction> {
        match self.macro_entries {
            Some(entries) => Arc::new(MacroFunction::new(id, entries, self.macro_arguments)),
            None => Arc::new(Arc::new(InstantiatedFunction { id, entries: self.plain_entries.expect("plain entries") })),
        }
    }
}
