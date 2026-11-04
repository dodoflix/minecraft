//! The vanilla `en_us` language file (`assets/minecraft/lang/en_us.json`).

use std::collections::HashMap;
use std::sync::OnceLock;

/// Translation key → format string (`%s` / `%1$s` placeholders untouched).
pub fn en_us() -> &'static HashMap<String, String> {
    static LANG: OnceLock<HashMap<String, String>> = OnceLock::new();
    LANG.get_or_init(|| {
        let text = crate::datapack::read_string("assets/minecraft/lang/en_us.json").expect("en_us.json is embedded");
        serde_json::from_str(&text).expect("en_us.json is a flat string map")
    })
}

/// `Language.getOrDefault`-style lookup.
pub fn translate(key: &str) -> Option<&'static str> {
    en_us().get(key).map(String::as_str)
}
