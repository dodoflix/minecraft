//! Port of `net.minecraft.resources.Identifier` (+ `IdentifierException`).
//! Not ported: `IdentifierPattern` (needs Java regex), brigadier `StringReader` readers, `resolveAgainst`.
use std::cmp::Ordering;
use std::fmt;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::math::java::java_string_hash;

pub const NAMESPACE_SEPARATOR: char = ':';
pub const DEFAULT_NAMESPACE: &str = "minecraft";
pub const REALMS_NAMESPACE: &str = "realms";
pub const ALLOWED_NAMESPACE_CHARACTERS: &str = "[a-z0-9_.-]";

/// `IdentifierException`; the message is `StringEscapeUtils.escapeJava`d like vanilla's.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IdentifierException(pub String);

impl IdentifierException {
    fn new(message: &str) -> Self {
        Self(escape_java(message))
    }
}

impl fmt::Display for IdentifierException {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for IdentifierException {}

/// `org.apache.commons.lang3.StringEscapeUtils.escapeJava`
pub fn escape_java(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\u{8}' => out.push_str("\\b"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            '\u{c}' => out.push_str("\\f"),
            '\r' => out.push_str("\\r"),
            ' '..='\u{7f}' => out.push(c),
            _ => {
                let mut buf = [0u16; 2];
                for unit in c.encode_utf16(&mut buf) {
                    out.push_str(&format!("\\u{unit:04X}"));
                }
            }
        }
    }
    out
}

#[derive(Clone, PartialEq, Eq, Hash)]
pub struct Identifier {
    namespace: String,
    path: String,
}

impl Identifier {
    /// `fromNamespaceAndPath`
    pub fn new(namespace: &str, path: &str) -> Result<Self, IdentifierException> {
        Self::create_untrusted(namespace, path)
    }

    pub fn from_namespace_and_path(namespace: &str, path: &str) -> Result<Self, IdentifierException> {
        Self::create_untrusted(namespace, path)
    }

    fn create_untrusted(namespace: &str, path: &str) -> Result<Self, IdentifierException> {
        assert_valid_namespace(namespace, path)?;
        assert_valid_path(namespace, path)?;
        Ok(Self { namespace: namespace.to_owned(), path: path.to_owned() })
    }

    pub fn parse(identifier: &str) -> Result<Self, IdentifierException> {
        Self::by_separator(identifier, ':')
    }

    pub fn with_default_namespace(path: &str) -> Result<Self, IdentifierException> {
        assert_valid_path(DEFAULT_NAMESPACE, path)?;
        Ok(Self { namespace: DEFAULT_NAMESPACE.to_owned(), path: path.to_owned() })
    }

    pub fn try_parse(identifier: &str) -> Option<Self> {
        Self::try_by_separator(identifier, ':')
    }

    pub fn try_build(namespace: &str, path: &str) -> Option<Self> {
        (is_valid_namespace(namespace) && is_valid_path(path)).then(|| Self { namespace: namespace.to_owned(), path: path.to_owned() })
    }

    pub fn by_separator(identifier: &str, separator: char) -> Result<Self, IdentifierException> {
        match identifier.find(separator) {
            Some(i) => {
                let path = &identifier[i + separator.len_utf8()..];
                if i != 0 { Self::create_untrusted(&identifier[..i], path) } else { Self::with_default_namespace(path) }
            }
            None => Self::with_default_namespace(identifier),
        }
    }

    pub fn try_by_separator(identifier: &str, separator: char) -> Option<Self> {
        match identifier.find(separator) {
            Some(i) => {
                let path = &identifier[i + separator.len_utf8()..];
                if !is_valid_path(path) {
                    None
                } else if i != 0 {
                    let namespace = &identifier[..i];
                    is_valid_namespace(namespace).then(|| Self { namespace: namespace.to_owned(), path: path.to_owned() })
                } else {
                    Some(Self { namespace: DEFAULT_NAMESPACE.to_owned(), path: path.to_owned() })
                }
            }
            None => is_valid_path(identifier).then(|| Self { namespace: DEFAULT_NAMESPACE.to_owned(), path: identifier.to_owned() }),
        }
    }

    /// `Identifier.read(String)`: the `DataResult` error message on failure.
    pub fn read(input: &str) -> Result<Self, String> {
        Self::parse(input).map_err(|e| format!("Not a valid resource location: {input} {}", e.0))
    }

    pub fn path(&self) -> &str {
        &self.path
    }

    pub fn namespace(&self) -> &str {
        &self.namespace
    }

    pub fn with_path(&self, new_path: &str) -> Result<Self, IdentifierException> {
        assert_valid_path(&self.namespace, new_path)?;
        Ok(Self { namespace: self.namespace.clone(), path: new_path.to_owned() })
    }

    pub fn with_prefix(&self, prefix: &str) -> Result<Self, IdentifierException> {
        self.with_path(&format!("{prefix}{}", self.path))
    }

    pub fn with_suffix(&self, suffix: &str) -> Result<Self, IdentifierException> {
        self.with_path(&format!("{}{suffix}", self.path))
    }

    /// Java `hashCode()`: `31 * namespace.hashCode() + path.hashCode()`.
    pub fn java_hash_code(&self) -> i32 {
        31i32.wrapping_mul(java_string_hash(&self.namespace)).wrapping_add(java_string_hash(&self.path))
    }

    pub fn to_debug_file_name(&self) -> String {
        self.to_string().replace('/', "_").replace(':', "_")
    }

    pub fn to_language_key(&self) -> String {
        format!("{}.{}", self.namespace, self.path)
    }

    pub fn to_short_language_key(&self) -> String {
        if self.namespace == DEFAULT_NAMESPACE { self.path.clone() } else { self.to_language_key() }
    }

    pub fn to_short_string(&self) -> String {
        if self.namespace == DEFAULT_NAMESPACE { self.path.clone() } else { self.to_string() }
    }

    pub fn to_language_key_prefixed(&self, prefix: &str) -> String {
        format!("{prefix}.{}", self.to_language_key())
    }

    pub fn to_language_key_prefixed_suffixed(&self, prefix: &str, suffix: &str) -> String {
        format!("{prefix}.{}.{suffix}", self.to_language_key())
    }
}

pub fn is_allowed_in_identifier(c: char) -> bool {
    matches!(c, '0'..='9' | 'a'..='z' | '_' | ':' | '/' | '.' | '-')
}

pub fn is_valid_path(path: &str) -> bool {
    path.chars().all(valid_path_char)
}

pub fn is_valid_namespace(namespace: &str) -> bool {
    namespace != ".." && namespace.chars().all(valid_namespace_char)
}

pub fn valid_path_char(c: char) -> bool {
    matches!(c, '_' | '-' | 'a'..='z' | '0'..='9' | '/' | '.')
}

fn valid_namespace_char(c: char) -> bool {
    matches!(c, '_' | '-' | 'a'..='z' | '0'..='9' | '.')
}

fn assert_valid_namespace(namespace: &str, path: &str) -> Result<(), IdentifierException> {
    if is_valid_namespace(namespace) {
        Ok(())
    } else {
        Err(IdentifierException::new(&format!("Non [a-z0-9_.-] character in namespace of identifier: {namespace}:{path}")))
    }
}

fn assert_valid_path(namespace: &str, path: &str) -> Result<(), IdentifierException> {
    if is_valid_path(path) {
        Ok(())
    } else {
        Err(IdentifierException::new(&format!("Non [a-z0-9/._-] character in path of location: {namespace}:{path}")))
    }
}

impl fmt::Display for Identifier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.namespace, self.path)
    }
}

impl fmt::Debug for Identifier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.namespace, self.path)
    }
}

/// `compareTo`: path first, then namespace (identifier chars are ASCII, so byte order == UTF-16 order).
impl Ord for Identifier {
    fn cmp(&self, other: &Self) -> Ordering {
        self.path.cmp(&other.path).then_with(|| self.namespace.cmp(&other.namespace))
    }
}

impl PartialOrd for Identifier {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Serialize for Identifier {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for Identifier {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        Self::read(&s).map_err(serde::de::Error::custom)
    }
}
