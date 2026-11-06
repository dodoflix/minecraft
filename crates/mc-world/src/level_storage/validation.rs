//! `net.minecraft.world.level.validation`: `DirectoryValidator`, `PathAllowList` (`allowed_symlinks.txt`),
//! `ForbiddenSymlinkInfo`, `ContentValidationException`.

use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use regex::Regex;

/// `java.nio.file.PathMatcher`.
pub type PathMatcher = Arc<dyn Fn(&Path) -> bool + Send + Sync>;

/// `ForbiddenSymlinkInfo(link, target)`.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct ForbiddenSymlinkInfo {
    pub link: PathBuf,
    pub target: PathBuf,
}

/// `ContentValidationException`.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct ContentValidationException {
    pub directory: PathBuf,
    pub entries: Vec<ForbiddenSymlinkInfo>,
}

impl ContentValidationException {
    pub fn message(directory: &Path, entries: &[ForbiddenSymlinkInfo]) -> String {
        format!(
            "Failed to validate '{}'. Found forbidden symlinks: {}",
            directory.display(),
            entries
                .iter()
                .map(|e| format!("{}->{}", e.link.display(), e.target.display()))
                .collect::<Vec<_>>()
                .join(", ")
        )
    }
}

impl fmt::Display for ContentValidationException {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&ContentValidationException::message(
            &self.directory,
            &self.entries,
        ))
    }
}

impl std::error::Error for ContentValidationException {}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum EntryType {
    /// `FileSystem::getPathMatcher`: the pattern is `glob:...` / `regex:...`.
    Filesystem,
    /// `path.toString().startsWith(pattern)`.
    Prefix,
}

/// `PathAllowList.ConfigEntry`.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct ConfigEntry {
    pub entry_type: EntryType,
    pub pattern: String,
}

impl ConfigEntry {
    /// `ConfigEntry.parse(line)`: `Ok(None)` for blank/comment lines, `Err` for `IllegalArgumentException`.
    pub fn parse(definition: &str) -> Result<Option<ConfigEntry>, String> {
        if definition.trim().is_empty() || definition.starts_with('#') {
            return Ok(None);
        }
        if !definition.starts_with('[') {
            return Ok(Some(ConfigEntry {
                entry_type: EntryType::Prefix,
                pattern: definition.to_owned(),
            }));
        }
        let split = definition[1..]
            .find(']')
            .map(|i| i + 1)
            .ok_or_else(|| format!("Unterminated type in line '{definition}'"))?;
        let (kind, contents) = (&definition[1..split], &definition[split + 1..]);
        match kind {
            "glob" | "regex" => Ok(Some(ConfigEntry {
                entry_type: EntryType::Filesystem,
                pattern: format!("{kind}:{contents}"),
            })),
            "prefix" => Ok(Some(ConfigEntry {
                entry_type: EntryType::Prefix,
                pattern: contents.to_owned(),
            })),
            _ => Err(format!(
                "Unsupported definition type in line '{definition}'"
            )),
        }
    }

    /// `ConfigEntry.compile`: `Err` stands for the exceptions `getPathMatcher` throws.
    pub fn compile(&self) -> Result<PathMatcher, String> {
        match self.entry_type {
            EntryType::Prefix => {
                let prefix = self.pattern.clone();
                Ok(Arc::new(move |p: &Path| {
                    p.to_string_lossy().starts_with(&prefix)
                }))
            }
            EntryType::Filesystem => {
                let (syntax, input) = self.pattern.split_once(':').ok_or("No syntax specified")?;
                let expr = match syntax.to_ascii_lowercase().as_str() {
                    "glob" => glob_to_regex(input)?,
                    "regex" => input.to_owned(),
                    other => return Err(format!("Unsupported syntax '{other}'")),
                };
                let re = Regex::new(&format!("^(?:{expr})$")).map_err(|e| e.to_string())?;
                Ok(Arc::new(move |p: &Path| re.is_match(&p.to_string_lossy())))
            }
        }
    }
}

fn is_regex_meta(c: char) -> bool {
    ".^$+{[]|()".contains(c)
}

fn is_glob_meta(c: char) -> bool {
    "\\*?[{".contains(c)
}

/// `sun.nio.fs.Globs.toUnixRegexPattern`.
pub fn glob_to_regex(glob: &str) -> Result<String, String> {
    let g: Vec<char> = glob.chars().collect();
    let next = |i: usize| g.get(i).copied();
    let mut in_group = false;
    let mut regex = String::from("^");
    let mut i = 0;
    while i < g.len() {
        let mut c = g[i];
        i += 1;
        match c {
            '\\' => {
                let n = next(i).ok_or("No character to escape")?;
                i += 1;
                if is_glob_meta(n) || is_regex_meta(n) {
                    regex.push('\\');
                }
                regex.push(n);
            }
            '/' => regex.push(c),
            '[' => {
                regex.push_str("[[^/]&&[");
                if next(i) == Some('^') {
                    regex.push_str("\\^");
                    i += 1;
                } else {
                    if next(i) == Some('!') {
                        regex.push('^');
                        i += 1;
                    }
                    if next(i) == Some('-') {
                        regex.push('-');
                        i += 1;
                    }
                }
                let mut has_range_start = false;
                let mut last = '\0';
                let mut closed = false;
                while i < g.len() {
                    c = g[i];
                    i += 1;
                    if c == ']' {
                        closed = true;
                        break;
                    }
                    if c == '/' {
                        return Err("Explicit 'name separator' in class".into());
                    }
                    if c == '\\' || c == '[' || (c == '&' && next(i) == Some('&')) {
                        regex.push('\\');
                    }
                    regex.push(c);
                    if c == '-' {
                        if !has_range_start {
                            return Err("Invalid range".into());
                        }
                        let n = next(i);
                        i += 1;
                        match n {
                            None => break,
                            Some(']') => {
                                closed = true;
                                break;
                            }
                            Some(n) => {
                                if n < last {
                                    return Err("Invalid range".into());
                                }
                                regex.push(n);
                                has_range_start = false;
                            }
                        }
                    } else {
                        has_range_start = true;
                        last = c;
                    }
                }
                if !closed {
                    return Err("Missing ']".into());
                }
                regex.push_str("]]");
            }
            '{' => {
                if in_group {
                    return Err("Cannot nest groups".into());
                }
                regex.push_str("(?:(?:");
                in_group = true;
            }
            '}' => {
                if in_group {
                    regex.push_str("))");
                    in_group = false;
                } else {
                    regex.push('}');
                }
            }
            ',' => regex.push_str(if in_group { ")|(?:" } else { "," }),
            '*' => {
                if next(i) == Some('*') {
                    regex.push_str(".*");
                    i += 1;
                } else {
                    regex.push_str("[^/]*");
                }
            }
            '?' => regex.push_str("[^/]"),
            _ => {
                if is_regex_meta(c) {
                    regex.push('\\');
                }
                regex.push(c);
            }
        }
    }
    if in_group {
        return Err("Missing '}".into());
    }
    regex.push('$');
    Ok(regex)
}

/// `PathAllowList`.
#[derive(Clone, Debug, Default)]
pub struct PathAllowList {
    entries: Vec<ConfigEntry>,
}

impl PathAllowList {
    pub fn new(entries: Vec<ConfigEntry>) -> PathAllowList {
        PathAllowList { entries }
    }

    /// `PathAllowList.readPlain`: one entry per line, an invalid line fails the whole parse.
    pub fn read_plain(text: &str) -> Result<PathAllowList, String> {
        let mut entries = Vec::new();
        for line in text.lines() {
            entries.extend(ConfigEntry::parse(line)?);
        }
        Ok(PathAllowList { entries })
    }

    /// `getForFileSystem`: a pattern that fails to compile makes the whole list match nothing (logged).
    pub fn matcher(&self) -> PathMatcher {
        let compiled: Result<Vec<PathMatcher>, String> =
            self.entries.iter().map(ConfigEntry::compile).collect();
        match compiled {
            Ok(matchers) => Arc::new(move |p: &Path| matchers.iter().any(|m| m(p))),
            Err(e) => {
                log::error!("Failed to compile file pattern list: {e}");
                Arc::new(|_: &Path| false)
            }
        }
    }
}

/// `DirectoryValidator`.
#[derive(Clone)]
pub struct DirectoryValidator {
    symlink_target_allow_list: PathMatcher,
}

impl DirectoryValidator {
    pub fn new(symlink_target_allow_list: PathMatcher) -> DirectoryValidator {
        DirectoryValidator {
            symlink_target_allow_list,
        }
    }

    /// `LevelStorageSource.NO_SYMLINKS_ALLOWED`.
    pub fn no_symlinks_allowed() -> DirectoryValidator {
        DirectoryValidator::new(Arc::new(|_: &Path| false))
    }

    pub fn validate_symlink_into(
        &self,
        path: &Path,
        issues: &mut Vec<ForbiddenSymlinkInfo>,
    ) -> io::Result<()> {
        let target = fs::read_link(path)?;
        if !(self.symlink_target_allow_list)(&target) {
            issues.push(ForbiddenSymlinkInfo {
                link: path.to_path_buf(),
                target,
            });
        }
        Ok(())
    }

    pub fn validate_symlink(&self, path: &Path) -> io::Result<Vec<ForbiddenSymlinkInfo>> {
        let mut result = Vec::new();
        self.validate_symlink_into(path, &mut result)?;
        Ok(result)
    }

    pub fn validate_directory(
        &self,
        directory: &Path,
        allow_top_symlink: bool,
    ) -> io::Result<Vec<ForbiddenSymlinkInfo>> {
        let mut issues = Vec::new();
        let attrs = match fs::symlink_metadata(directory) {
            Ok(a) => a,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(issues),
            Err(e) => return Err(e),
        };
        if attrs.is_file() {
            return Err(io::Error::other(format!(
                "Path {} is not a directory",
                directory.display()
            )));
        }
        let mut directory = directory.to_path_buf();
        if attrs.file_type().is_symlink() {
            if !allow_top_symlink {
                self.validate_symlink_into(&directory, &mut issues)?;
                return Ok(issues);
            }
            directory = fs::read_link(&directory)?;
        }
        self.validate_known_directory(&directory, &mut issues)?;
        Ok(issues)
    }

    /// `validateKnownDirectory`: `Files.walkFileTree` without following links.
    pub fn validate_known_directory(
        &self,
        directory: &Path,
        issues: &mut Vec<ForbiddenSymlinkInfo>,
    ) -> io::Result<()> {
        let attrs = fs::symlink_metadata(directory)?;
        if attrs.file_type().is_symlink() {
            self.validate_symlink_into(directory, issues)
        } else if attrs.is_dir() {
            for entry in fs::read_dir(directory)? {
                self.validate_known_directory(&entry?.path(), issues)?;
            }
            Ok(())
        } else {
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn glob_matches(glob: &str, path: &str) -> bool {
        let entry = ConfigEntry {
            entry_type: EntryType::Filesystem,
            pattern: format!("glob:{glob}"),
        };
        entry.compile().unwrap()(Path::new(path))
    }

    #[test]
    fn glob_semantics() {
        assert!(glob_matches("/a/*.txt", "/a/b.txt"));
        assert!(!glob_matches("/a/*.txt", "/a/b/c.txt"));
        assert!(glob_matches("/a/**", "/a/b/c.txt"));
        assert!(glob_matches("/a/{x,y}?", "/a/yz"));
        assert!(glob_matches("/a/[!0-9]", "/a/q"));
        assert!(!glob_matches("/a/[!0-9]", "/a/5"));
    }

    #[test]
    fn config_parse() {
        assert_eq!(ConfigEntry::parse("# c").unwrap(), None);
        assert_eq!(ConfigEntry::parse("   ").unwrap(), None);
        assert_eq!(
            ConfigEntry::parse("/x").unwrap().unwrap().entry_type,
            EntryType::Prefix
        );
        assert_eq!(
            ConfigEntry::parse("[glob]/x/*").unwrap().unwrap().pattern,
            "glob:/x/*"
        );
        assert!(ConfigEntry::parse("[bad]x").is_err());
        assert!(ConfigEntry::parse("[glob").is_err());
    }

    #[cfg(unix)]
    #[test]
    fn symlink_validation() {
        let dir = tempfile::tempdir().unwrap();
        let world = dir.path().join("world");
        fs::create_dir_all(world.join("sub")).unwrap();
        std::os::unix::fs::symlink("/etc", world.join("sub/link")).unwrap();
        let v = DirectoryValidator::no_symlinks_allowed();
        let issues = v.validate_directory(&world, true).unwrap();
        assert_eq!(
            issues,
            vec![ForbiddenSymlinkInfo {
                link: world.join("sub/link"),
                target: "/etc".into()
            }]
        );
        let allow = PathAllowList::read_plain("/etc\n").unwrap();
        assert!(
            DirectoryValidator::new(allow.matcher())
                .validate_directory(&world, true)
                .unwrap()
                .is_empty()
        );
        assert!(
            v.validate_directory(&dir.path().join("missing"), true)
                .unwrap()
                .is_empty()
        );
    }
}
