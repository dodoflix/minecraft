//! `net.minecraft.locale.Language` + `DeprecatedTranslationsInfo`: translation lookup backed by
//! the vanilla `en_us.json`.

use std::collections::HashMap;
use std::sync::{Arc, LazyLock, RwLock};

const EN_US: &str = include_str!(env!("EN_US_JSON"));
const DEPRECATED: &str = include_str!(env!("DEPRECATED_JSON"));

pub const DEFAULT: &str = "en_us";

#[derive(Debug, Default)]
pub struct Language {
    storage: HashMap<String, String>,
}

static DEFAULT_INSTANCE: LazyLock<Arc<Language>> =
    LazyLock::new(|| Arc::new(Language::load_default()));
static INSTANCE: RwLock<Option<Arc<Language>>> = RwLock::new(None);

impl Language {
    /// Builds a language from key/value pairs after applying vanilla's `%d`/`%f` -> `%s` rewrite
    /// (`Language.loadFromJson`).
    pub fn from_entries(entries: impl IntoIterator<Item = (String, String)>) -> Language {
        Language {
            storage: entries
                .into_iter()
                .map(|(k, v)| (k, replace_unsupported_formats(&v)))
                .collect(),
        }
    }

    /// Parses a language JSON object (`Language.loadFromJson`); non-string values are rejected.
    pub fn from_json(json: &str) -> Result<Language, String> {
        let value: serde_json::Value = serde_json::from_str(json).map_err(|e| e.to_string())?;
        let obj = value
            .as_object()
            .ok_or("Language file is not a JSON object")?;
        let mut entries = Vec::with_capacity(obj.len());
        for (k, v) in obj {
            let s = v
                .as_str()
                .ok_or_else(|| format!("Expected {k} to be a string"))?;
            entries.push((k.clone(), s.to_owned()));
        }
        Ok(Language::from_entries(entries))
    }

    /// `Language.loadDefault`: en_us.json with deprecated.json applied.
    fn load_default() -> Language {
        let mut lang = Language::from_json(EN_US).expect("bundled en_us.json is valid");
        let deprecated: serde_json::Value =
            serde_json::from_str(DEPRECATED).expect("bundled deprecated.json is valid");
        if let Some(removed) = deprecated.get("removed").and_then(|v| v.as_array()) {
            for key in removed.iter().filter_map(|k| k.as_str()) {
                lang.storage.remove(key);
            }
        }
        if let Some(renamed) = deprecated.get("renamed").and_then(|v| v.as_object()) {
            for (from, to) in renamed {
                let Some(to) = to.as_str() else { continue };
                match lang.storage.remove(from) {
                    Some(value) => {
                        lang.storage.insert(to.to_owned(), value);
                    }
                    None => {
                        lang.storage.remove(to);
                    }
                }
            }
        }
        lang
    }

    /// `Language.DEFAULT_INSTANCE`.
    pub fn default_instance() -> Arc<Language> {
        DEFAULT_INSTANCE.clone()
    }

    /// `Language.getInstance()`.
    pub fn get_instance() -> Arc<Language> {
        match &*INSTANCE.read().unwrap_or_else(|e| e.into_inner()) {
            Some(l) => l.clone(),
            None => DEFAULT_INSTANCE.clone(),
        }
    }

    /// `Language.inject`.
    pub fn inject(language: Language) {
        *INSTANCE.write().unwrap_or_else(|e| e.into_inner()) = Some(Arc::new(language));
    }

    pub fn get_or_default<'a>(&'a self, element_id: &str, default: &'a str) -> &'a str {
        self.storage.get(element_id).map_or(default, String::as_str)
    }

    pub fn get(&self, element_id: &str) -> Option<&str> {
        self.storage.get(element_id).map(String::as_str)
    }

    pub fn has(&self, element_id: &str) -> bool {
        self.storage.contains_key(element_id)
    }

    pub fn len(&self) -> usize {
        self.storage.len()
    }

    pub fn is_empty(&self) -> bool {
        self.storage.is_empty()
    }

    pub fn is_default_right_to_left(&self) -> bool {
        false
    }
}

/// `UNSUPPORTED_FORMAT_PATTERN.replaceAll("%$1s")` with `%(\d+\$)?[\d.]*[df]`.
fn replace_unsupported_formats(s: &str) -> String {
    if !s.contains('%') {
        return s.to_owned();
    }
    let b = s.as_bytes();
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    let mut copied = 0;
    while i < b.len() {
        if b[i] != b'%' {
            i += 1;
            continue;
        }
        if let Some((group, end)) = match_unsupported(b, i + 1) {
            out.push_str(&s[copied..i]);
            out.push('%');
            out.push_str(group);
            out.push('s');
            i = end;
            copied = end;
        } else {
            i += 1;
        }
    }
    out.push_str(&s[copied..]);
    out
}

/// Matches `(\d+\$)?[\d.]*[df]` at `start`; returns the group text and the end offset.
fn match_unsupported(b: &[u8], start: usize) -> Option<(&str, usize)> {
    let tail = |from: usize| -> Option<usize> {
        let mut j = from;
        while j < b.len() && (b[j].is_ascii_digit() || b[j] == b'.') {
            j += 1;
        }
        // Greedy `[\d.]*` then `[df]`; digits/dots are never d or f so no backtracking is needed.
        (j < b.len() && (b[j] == b'd' || b[j] == b'f')).then_some(j + 1)
    };
    let mut j = start;
    while j < b.len() && b[j].is_ascii_digit() {
        j += 1;
    }
    if j > start
        && j < b.len()
        && b[j] == b'$'
        && let Some(end) = tail(j + 1)
    {
        return Some((std::str::from_utf8(&b[start..=j]).ok()?, end));
    }
    tail(start).map(|end| ("", end))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unsupported_formats_become_s() {
        assert_eq!(replace_unsupported_formats("%d items"), "%s items");
        assert_eq!(replace_unsupported_formats("%1$d/%2$.2f"), "%1$s/%2$s");
        assert_eq!(replace_unsupported_formats("%5.2f%%"), "%s%%");
        assert_eq!(replace_unsupported_formats("100%"), "100%");
        assert_eq!(replace_unsupported_formats("%s %1$s"), "%s %1$s");
    }

    #[test]
    fn default_language_loaded() {
        let l = Language::default_instance();
        assert_eq!(l.get("gui.done"), Some("Done"));
        assert!(
            !l.has("block.minecraft.grass"),
            "removed by deprecated.json"
        );
    }
}
