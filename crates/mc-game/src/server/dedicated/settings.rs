//! `net.minecraft.server.dedicated.Settings`: typed, self-normalizing access to a `java.util.Properties` table.
//! Every `get*` writes the effective value back, so storing the table afterwards produces vanilla's rewritten file.

use std::fs;
use std::path::Path;

use super::java_properties::{MalformedUnicodeEscape, Properties};

/// An exception vanilla lets escape from the settings constructors (the server then fails with
/// `Failed to start the minecraft server`). The text is the Java `Throwable.toString()`.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct SettingsError(pub String);

impl From<MalformedUnicodeEscape> for SettingsError {
    fn from(_: MalformedUnicodeEscape) -> Self {
        SettingsError("java.lang.IllegalArgumentException: Malformed \\uxxxx encoding.".to_owned())
    }
}

/// `Settings.loadFromFile`: UTF-8 first, ISO-8859-1 when the bytes are not UTF-8, an empty table (with an error log)
/// when the file cannot be read.
pub fn load_from_file(file: &Path) -> Result<Properties, SettingsError> {
    let bytes = match fs::read(file) {
        Ok(b) => b,
        Err(e) => {
            log::error!("Failed to load properties from file: {}\n{}", file.display(), java_io_error(&e, file));
            return Ok(Properties::new());
        }
    };
    let text = match String::from_utf8(bytes) {
        Ok(text) => text,
        Err(e) => {
            log::info!("Failed to load properties as UTF-8 from file {}, trying ISO_8859_1", file.display());
            return Ok(Properties::load_latin1(e.as_bytes())?);
        }
    };
    Ok(Properties::load(&text)?)
}

fn java_io_error(e: &std::io::Error, file: &Path) -> String {
    match e.kind() {
        std::io::ErrorKind::NotFound => format!("java.nio.file.NoSuchFileException: {}", file.display()),
        _ => format!("java.io.IOException: {e}"),
    }
}

/// `Settings.store`: UTF-8, header comment `Minecraft server properties`.
pub fn store(properties: &Properties, output: &Path) {
    if fs::write(output, properties.store(Some("Minecraft server properties"))).is_err() {
        log::error!("Failed to store properties to file: {}", output.display());
    }
}

/// `Settings.MutableValue<V>`: the current value plus how to serialize a replacement into the properties.
#[derive(Clone, Debug)]
pub struct MutableValue<V> {
    key: &'static str,
    value: V,
    serializer: fn(&V) -> String,
}

impl<V> MutableValue<V> {
    pub fn get(&self) -> &V {
        &self.value
    }

    pub fn key(&self) -> &'static str {
        self.key
    }

    /// `update`: the table with `key=serialize(value)` replaced, for the owner to re-parse (`Settings.reload`).
    pub fn updated_properties(&self, owner: &Properties, value: &V) -> Properties {
        let mut properties = owner.clone();
        properties.put(self.key, (self.serializer)(value));
        properties
    }
}

/// The mutable table behind a `Settings` subclass while its fields are being initialized.
pub struct Settings {
    pub properties: Properties,
}

/// `Boolean.valueOf(String)`.
pub fn java_boolean(s: &str) -> bool {
    s.eq_ignore_ascii_case("true")
}

impl Settings {
    pub fn new(properties: Properties) -> Settings {
        Settings { properties }
    }

    pub fn get_legacy<V>(&mut self, key: &str, deserializer: impl FnOnce(&str) -> V) -> Option<V> {
        let value = self.properties.get(key)?.to_owned();
        self.properties.remove(key);
        Some(deserializer(&value))
    }

    pub fn get_legacy_string(&mut self, key: &str) -> Option<String> {
        self.get_legacy(key, str::to_owned)
    }

    pub fn get_legacy_boolean(&mut self, key: &str) -> Option<bool> {
        self.get_legacy(key, java_boolean)
    }

    /// `get(key, deserializer, serializer, default)`: a `None` (Java `null`) from the deserializer, or a missing key,
    /// yields the default. `Err` is an exception escaping the deserializer.
    pub fn get_with<V>(
        &mut self,
        key: &str,
        deserializer: impl FnOnce(&str) -> Result<Option<V>, SettingsError>,
        serializer: impl FnOnce(&V) -> String,
        default: V,
    ) -> Result<V, SettingsError> {
        let parsed = match self.properties.get(key) {
            Some(value) => deserializer(value)?,
            None => None,
        };
        let result = parsed.unwrap_or(default);
        self.properties.put(key, serializer(&result));
        Ok(result)
    }

    pub fn get_mutable_with<V>(
        &mut self,
        key: &'static str,
        deserializer: impl FnOnce(&str) -> Result<Option<V>, SettingsError>,
        serializer: fn(&V) -> String,
        default: V,
    ) -> Result<MutableValue<V>, SettingsError> {
        let value = self.get_with(key, deserializer, serializer, default)?;
        Ok(MutableValue { key, value, serializer })
    }

    pub fn get_string(&mut self, key: &str, default: &str) -> String {
        let value = self.properties.get(key).unwrap_or(default).to_owned();
        self.properties.put(key, value.clone());
        value
    }

    pub fn get_mutable_string(&mut self, key: &'static str, default: &str) -> MutableValue<String> {
        let value = self.get_string(key, default);
        MutableValue { key, value, serializer: String::clone }
    }

    /// `get(key, int)`: `Integer.parseInt`, a `NumberFormatException` falls back to the default.
    pub fn get_int(&mut self, key: &str, default: i32) -> i32 {
        self.get_int_validated(key, |v| v, default)
    }

    pub fn get_int_validated(&mut self, key: &str, validator: impl FnOnce(i32) -> i32, default: i32) -> i32 {
        let parsed = self.properties.get(key).and_then(|s| s.parse::<i32>().ok()).map(validator);
        let result = parsed.unwrap_or(default);
        self.properties.put(key, result.to_string());
        result
    }

    pub fn get_mutable_int(&mut self, key: &'static str, default: i32) -> MutableValue<i32> {
        let value = self.get_int(key, default);
        MutableValue { key, value, serializer: |v| v.to_string() }
    }

    pub fn get_long(&mut self, key: &str, default: i64) -> i64 {
        let result = self.properties.get(key).and_then(|s| s.parse::<i64>().ok()).unwrap_or(default);
        self.properties.put(key, result.to_string());
        result
    }

    pub fn get_boolean(&mut self, key: &str, default: bool) -> bool {
        let result = self.properties.get(key).map_or(default, java_boolean);
        self.properties.put(key, result.to_string());
        result
    }

    pub fn get_mutable_boolean(&mut self, key: &'static str, default: bool) -> MutableValue<bool> {
        let value = self.get_boolean(key, default);
        MutableValue { key, value, serializer: |v| v.to_string() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn get_writes_back_effective_values() {
        let mut p = Properties::new();
        p.put("n", "abc");
        p.put("b", "yes");
        p.put("m", " 12");
        let mut s = Settings::new(p);
        assert_eq!(s.get_int("n", 7), 7);
        assert!(!s.get_boolean("b", true));
        assert_eq!(s.get_int("m", 3), 3);
        assert_eq!(s.get_long("missing", 5), 5);
        assert_eq!(s.properties.get("n"), Some("7"));
        assert_eq!(s.properties.get("b"), Some("false"));
        assert_eq!(s.properties.get("missing"), Some("5"));
    }

    #[test]
    fn legacy_removes_key() {
        let mut p = Properties::new();
        p.put("old", "TRUE");
        let mut s = Settings::new(p);
        assert_eq!(s.get_legacy_boolean("old"), Some(true));
        assert_eq!(s.get_legacy_boolean("old"), None);
        assert!(s.properties.get("old").is_none());
    }

    #[test]
    fn mutable_value_update() {
        let mut s = Settings::new(Properties::new());
        let v = s.get_mutable_int("max-players", 20);
        assert_eq!(*v.get(), 20);
        let props = v.updated_properties(&s.properties, &5);
        assert_eq!(props.get("max-players"), Some("5"));
    }
}
