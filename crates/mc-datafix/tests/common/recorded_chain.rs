//! The recorded fix chain (`fixtures/fixes.tsv`, written by `scripts/datafix-oracle.sh --regen` from the real
//! `DataFixerUpper.globalList` and `FileFixerUpper.fileFixes`): `INDEX VERSION SUBVERSION KIND CLASS` per row.

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Row {
    pub index: usize,
    pub version: i32,
    pub sub: i32,
    /// `data` or `file`.
    pub kind: String,
    /// Simple class name; a factory-built fix is `Outer$N`.
    pub class: String,
}

impl Row {
    pub fn parse(line: &str) -> Row {
        let f: Vec<&str> = line.split('\t').collect();
        assert_eq!(f.len(), 5, "malformed fix row: {line}");
        Row {
            index: f[0].parse().expect("index"),
            version: f[1].parse().expect("version"),
            sub: f[2].parse().expect("subversion"),
            kind: f[3].to_string(),
            class: f[4].to_string(),
        }
    }

    pub fn line(&self) -> String {
        format!(
            "{}\t{}\t{}\t{}\t{}",
            self.index, self.version, self.sub, self.kind, self.class
        )
    }
}

pub fn parse(text: &str) -> Vec<Row> {
    text.lines()
        .filter(|l| !l.is_empty())
        .map(Row::parse)
        .collect()
}
