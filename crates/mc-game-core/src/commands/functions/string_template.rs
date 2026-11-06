//! `StringTemplate`: a macro line split at `$(variable)`.

#[derive(Clone, Debug, PartialEq)]
pub struct StringTemplate {
    pub segments: Vec<String>,
    pub variables: Vec<String>,
}

fn is_valid_variable_name(variable: &str) -> bool {
    variable.chars().all(|c| c.is_alphanumeric() || c == '_')
}

impl StringTemplate {
    /// `fromString(input)`; errors are vanilla's `IllegalArgumentException` messages.
    pub fn from_string(input: &str) -> Result<StringTemplate, String> {
        let chars: Vec<char> = input.chars().collect();
        let mut segments = Vec::new();
        let mut variables = Vec::new();
        let length = chars.len();
        let mut start = 0usize;
        let mut index = chars.iter().position(|c| *c == '$');
        let sub = |a: usize, b: usize| chars[a..b].iter().collect::<String>();
        while let Some(i) = index {
            if i != length - 1 && chars[i + 1] == '(' {
                segments.push(sub(start, i));
                let Some(variable_end) = chars[i + 1..]
                    .iter()
                    .position(|c| *c == ')')
                    .map(|p| p + i + 1)
                else {
                    return Err("Unterminated macro variable".to_owned());
                };
                let variable = sub(i + 2, variable_end);
                if !is_valid_variable_name(&variable) {
                    return Err(format!("Invalid macro variable name '{variable}'"));
                }
                variables.push(variable);
                start = variable_end + 1;
                index = chars[start..]
                    .iter()
                    .position(|c| *c == '$')
                    .map(|p| p + start);
            } else {
                index = chars[i + 1..]
                    .iter()
                    .position(|c| *c == '$')
                    .map(|p| p + i + 1);
            }
        }
        if start == 0 {
            return Err("No variables in macro".to_owned());
        }
        if start != length {
            segments.push(sub(start, length));
        }
        Ok(StringTemplate {
            segments,
            variables,
        })
    }

    /// `substitute(arguments)`.
    pub fn substitute(&self, arguments: &[String]) -> Result<String, String> {
        let mut builder = String::new();
        for (i, segment) in self.segments.iter().enumerate().take(self.variables.len()) {
            builder.push_str(segment);
            builder.push_str(&arguments[i]);
            super::check_command_line_length(&builder)?;
        }
        if self.segments.len() > self.variables.len() {
            builder.push_str(self.segments.last().expect("segment"));
        }
        super::check_command_line_length(&builder)?;
        Ok(builder)
    }
}
