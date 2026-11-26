//! `util.CsvOutput`: a CSV table with a fixed header, `\r\n` line endings and `StringEscapeUtils.escapeCsv` quoting.

use std::fmt::Display;

/// `StringEscapeUtils.escapeCsv`: a value with a comma, quote, CR or LF is quoted with doubled quotes.
pub fn escape_csv(value: &str) -> String {
    if value.contains([',', '"', '\r', '\n']) {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_owned()
    }
}

/// `CsvOutput` over an in-memory buffer (the caller writes [`CsvOutput::finish`] to the file).
pub struct CsvOutput {
    out: String,
    column_count: usize,
}

impl CsvOutput {
    /// `CsvOutput.builder().addColumn(..)...build(writer)`: writes the header row.
    pub fn new(headers: &[&str]) -> Self {
        let mut csv = Self {
            out: String::new(),
            column_count: headers.len(),
        };
        csv.write_line(headers.iter().copied());
        csv
    }

    /// `writeRow(values...)`: panics (`IllegalArgumentException`) when the value count differs from the header's.
    pub fn write_row(&mut self, values: &[&dyn Display]) {
        assert_eq!(
            values.len(),
            self.column_count,
            "Invalid number of columns, expected {}, but got {}",
            self.column_count,
            values.len()
        );
        let values: Vec<String> = values.iter().map(|v| v.to_string()).collect();
        self.write_line(values.iter().map(String::as_str));
    }

    /// `getStringValue` of a `null` value.
    pub const NULL: &'static str = "[null]";

    /// The written table.
    pub fn finish(self) -> String {
        self.out
    }

    fn write_line<'a>(&mut self, values: impl Iterator<Item = &'a str>) {
        let line: Vec<String> = values.map(escape_csv).collect();
        self.out.push_str(&line.join(","));
        self.out.push_str("\r\n");
    }
}

/// A nullable cell: the value, or `[null]` (`CsvOutput.getStringValue`).
pub fn nullable<T: Display>(value: Option<T>) -> String {
    value.map_or_else(|| CsvOutput::NULL.to_owned(), |v| v.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_and_rows_end_with_crlf_and_quote_special_values() {
        let mut csv = CsvOutput::new(&["x", "name"]);
        csv.write_row(&[&1, &"plain"]);
        csv.write_row(&[&2, &"a,b"]);
        csv.write_row(&[&3, &"say \"hi\""]);
        csv.write_row(&[&4, &"two\nlines"]);
        csv.write_row(&[&5, &nullable::<i32>(None)]);
        assert_eq!(
            csv.finish(),
            "x,name\r\n1,plain\r\n2,\"a,b\"\r\n3,\"say \"\"hi\"\"\"\r\n4,\"two\nlines\"\r\n5,[null]\r\n"
        );
    }

    #[test]
    #[should_panic(expected = "Invalid number of columns, expected 2, but got 1")]
    fn wrong_column_count_is_rejected() {
        CsvOutput::new(&["a", "b"]).write_row(&[&1]);
    }
}
