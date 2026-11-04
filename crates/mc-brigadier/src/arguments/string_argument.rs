use crate::arguments::{ArgumentType, ParsedValue, value};
use crate::context::CommandContext;
use crate::exceptions::CommandSyntaxException;
use crate::string_reader::{ImmutableStringReader, StringReader};

/// `StringArgumentType.StringType`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StringType {
    SingleWord,
    QuotablePhrase,
    GreedyPhrase,
}

impl StringType {
    pub fn get_examples(self) -> &'static [&'static str] {
        match self {
            StringType::SingleWord => &["word", "words_with_underscores"],
            StringType::QuotablePhrase => &["\"quoted phrase\"", "word", "\"\""],
            StringType::GreedyPhrase => &["word", "words with spaces", "\"and symbols\""],
        }
    }
}

/// `StringArgumentType`.
#[derive(Clone, Copy, Debug)]
pub struct StringArgumentType {
    type_: StringType,
}

impl StringArgumentType {
    pub fn word() -> Self {
        Self { type_: StringType::SingleWord }
    }

    pub fn string() -> Self {
        Self { type_: StringType::QuotablePhrase }
    }

    pub fn greedy_string() -> Self {
        Self { type_: StringType::GreedyPhrase }
    }

    pub fn get_string<'a, S: 'static>(context: &'a CommandContext<S>, name: &str) -> &'a str {
        context.get_argument::<String>(name)
    }

    pub fn get_type(&self) -> StringType {
        self.type_
    }
}

impl<S: 'static> ArgumentType<S> for StringArgumentType {
    fn parse(&self, reader: &mut StringReader) -> Result<ParsedValue, CommandSyntaxException> {
        Ok(value(match self.type_ {
            StringType::GreedyPhrase => {
                let text = reader.get_remaining();
                reader.set_cursor(reader.get_total_length());
                text
            }
            StringType::SingleWord => reader.read_unquoted_string(),
            StringType::QuotablePhrase => reader.read_string()?,
        }))
    }

    fn get_examples(&self) -> Vec<String> {
        self.type_.get_examples().iter().map(|s| (*s).to_owned()).collect()
    }

    fn describe(&self) -> String {
        "string()".into()
    }
}

/// `StringArgumentType.escapeIfRequired`.
pub fn escape_if_required(input: &str) -> String {
    if input.chars().all(StringReader::is_allowed_in_unquoted_string) {
        return input.to_owned();
    }
    let mut result = String::from("\"");
    for c in input.chars() {
        if c == '\\' || c == '"' {
            result.push('\\');
        }
        result.push(c);
    }
    result.push('"');
    result
}
