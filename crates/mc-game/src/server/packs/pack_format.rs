//! `net.minecraft.server.packs.metadata.pack.{PackFormat, PackMetadataSection}`, `OverlayMetadataSection`,
//! `FeatureFlagsMetadataSection`, `ResourceFilterSection` and `util.InclusiveRange`: the `pack.mcmeta` sections,
//! decoded from JSON with the exact DFU error messages vanilla surfaces through `getOrThrow(JsonParseException::new)`.

use std::fmt;

use mc_core::ident::Identifier;
use mc_text::Component;
use mc_world::level_storage::FeatureFlagSet;
use regex::Regex;
use serde_json::{Map, Value};

use super::PackType;
use crate::registry::feature_flags;

/// `PackFormat`: ordered by `(major, minor)`.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct PackFormat {
    pub major: i32,
    pub minor: i32,
}

impl PackFormat {
    pub const fn of(major: i32) -> PackFormat {
        PackFormat { major, minor: 0 }
    }

    pub const fn of_minor(major: i32, minor: i32) -> PackFormat {
        PackFormat { major, minor }
    }

    /// `PackFormat.minorRange()`.
    pub fn minor_range(self) -> InclusiveRange<PackFormat> {
        InclusiveRange { min_inclusive: self, max_inclusive: PackFormat::of_minor(self.major, i32::MAX) }
    }

    /// `PackFormat.lastPreMinorVersion`.
    pub fn last_pre_minor_version(ty: PackType) -> i32 {
        match ty {
            PackType::ClientResources => 64,
            PackType::ServerData => 81,
        }
    }
}

impl fmt::Display for PackFormat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.minor == i32::MAX { write!(f, "{}.*", self.major) } else { write!(f, "{}.{}", self.major, self.minor) }
    }
}

/// `InclusiveRange<T>`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct InclusiveRange<T> {
    pub min_inclusive: T,
    pub max_inclusive: T,
}

impl<T: PartialOrd + Copy> InclusiveRange<T> {
    pub fn single(value: T) -> Self {
        InclusiveRange { min_inclusive: value, max_inclusive: value }
    }

    pub fn is_value_in_range(&self, value: T) -> bool {
        value >= self.min_inclusive && value <= self.max_inclusive
    }
}

fn compact(v: &Value) -> String {
    serde_json::to_string(v).unwrap_or_default()
}

/// `JsonOps.getNumberValue` + `Number.intValue()`.
fn get_int(v: &Value) -> Result<i32, String> {
    match v {
        Value::Number(n) => Ok(n.as_i64().map(|i| i as i32).or_else(|| n.as_f64().map(|f| f as i32)).unwrap_or(0)),
        Value::Bool(b) => Ok(*b as i32),
        _ => Err(format!("Not a number: {}", compact(v))),
    }
}

/// `ExtraCodecs.NON_NEGATIVE_INT`.
fn get_non_negative_int(v: &Value) -> Result<i32, String> {
    let n = get_int(v)?;
    if n >= 0 { Ok(n) } else { Err(format!("Value must be non-negative: {n}")) }
}

/// `PackFormat.fullCodec(defaultMinor)`: a number or a 1..=256 element list of non-negative numbers.
fn decode_pack_format(v: &Value, default_minor: i32) -> Result<PackFormat, String> {
    match v {
        Value::Array(list) => {
            if list.is_empty() || list.len() > 256 {
                return Err(format!("List has {} elements, expected between 1 and 256", list.len()));
            }
            let nums = list.iter().map(get_non_negative_int).collect::<Result<Vec<_>, _>>()?;
            Ok(PackFormat::of_minor(nums[0], nums.get(1).copied().unwrap_or(default_minor)))
        }
        other => Ok(PackFormat::of_minor(get_non_negative_int(other)?, default_minor)),
    }
}

/// `InclusiveRange.codec(Codec.INT)` = `ExtraCodecs.intervalCodec`: a number, `[min, max]` or `{min_inclusive, max_inclusive}`.
fn decode_int_range(v: &Value) -> Result<InclusiveRange<i32>, String> {
    let make = |min: i32, max: i32| {
        if min <= max { Ok(InclusiveRange { min_inclusive: min, max_inclusive: max }) } else { Err("min_inclusive must be less than or equal to max_inclusive".to_owned()) }
    };
    match v {
        Value::Array(list) => {
            let nums = list.iter().map(get_int).collect::<Result<Vec<_>, _>>()?;
            if nums.len() != 2 {
                return Err("Input is not a list of 2 elements".to_owned());
            }
            make(nums[0], nums[1])
        }
        Value::Object(o) => {
            let field = |name: &str| o.get(name).ok_or_else(|| format!("No key {name} in MapLike[{}]", compact(v))).and_then(get_int);
            let (min, max) = join_errors(field("min_inclusive"), field("max_inclusive"))?;
            make(min, max)
        }
        other => {
            let n = get_int(other)?;
            make(n, n)
        }
    }
}

/// DFU applicative error accumulation: both errors are kept, joined by `"; "`.
fn join_errors<A, B>(a: Result<A, String>, b: Result<B, String>) -> Result<(A, B), String> {
    match (a, b) {
        (Ok(a), Ok(b)) => Ok((a, b)),
        (Err(a), Ok(_)) | (Ok(_), Err(a)) => Err(a),
        (Err(a), Err(b)) => Err(format!("{a}; {b}")),
    }
}

/// `PackFormat.IntermediaryFormat`.
#[derive(Clone, Debug, Default)]
pub struct IntermediaryFormat {
    pub min: Option<PackFormat>,
    pub max: Option<PackFormat>,
    pub format: Option<i32>,
    pub supported: Option<InclusiveRange<i32>>,
}

fn optional<T>(o: &Map<String, Value>, key: &str, f: impl Fn(&Value) -> Result<T, String>) -> Result<Option<T>, String> {
    o.get(key).map(f).transpose()
}

impl IntermediaryFormat {
    /// `PACK_CODEC` (the `pack` section) or, with `overlay`, `OVERLAY_CODEC` (`min_format`/`max_format`/`formats`).
    fn decode(o: &Map<String, Value>, overlay: bool) -> Result<IntermediaryFormat, String> {
        let min = optional(o, "min_format", |v| decode_pack_format(v, 0));
        let max = optional(o, "max_format", |v| decode_pack_format(v, i32::MAX));
        if overlay {
            let formats = optional(o, "formats", decode_int_range);
            let ((min, max), formats) = join_errors(join_errors(min, max), formats)?;
            return Ok(IntermediaryFormat { min, max, format: min.map(|m| m.major), supported: formats });
        }
        let format = optional(o, "pack_format", |v| get_int(v));
        let supported = optional(o, "supported_formats", decode_int_range);
        let (((min, max), format), supported) = join_errors(join_errors(join_errors(min, max), format), supported)?;
        Ok(IntermediaryFormat { min, max, format, supported })
    }

    /// `IntermediaryFormat.effectiveMinMajorVersion`.
    pub fn effective_min_major_version(&self) -> i32 {
        match (self.min, self.supported) {
            (Some(min), Some(sup)) => min.major.min(sup.min_inclusive),
            (Some(min), None) => min.major,
            (None, Some(sup)) => sup.min_inclusive,
            (None, None) => i32::MAX,
        }
    }

    /// `IntermediaryFormat.validate`.
    pub fn validate(&self, last_pre_minor: i32, has_pack_format_field: bool, require_old_field: bool, context: &str, old_field: &str) -> Result<InclusiveRange<PackFormat>, String> {
        if self.min.is_some() != self.max.is_some() {
            return Err(format!("{context} missing field, must declare both min_format and max_format"));
        }
        if require_old_field && self.supported.is_none() {
            return Err(format!("{context} missing required field {old_field}, must be present in all overlays for any overlays to work across game versions"));
        }
        if self.min.is_some() {
            return self.validate_new_format(last_pre_minor, has_pack_format_field, require_old_field, context, old_field);
        }
        if self.supported.is_some() {
            return self.validate_old_format(last_pre_minor, has_pack_format_field, context, old_field);
        }
        if has_pack_format_field && let Some(main) = self.format {
            return if main > last_pre_minor {
                Err(format!("{context} declares support for version newer than {last_pre_minor}, but is missing mandatory fields min_format and max_format"))
            } else {
                Ok(InclusiveRange::single(PackFormat::of(main)))
            };
        }
        Err(format!("{context} could not be parsed, missing format version information"))
    }

    fn validate_new_format(&self, last_pre_minor: i32, has_pack_format_field: bool, require_old_field: bool, context: &str, old_field: &str) -> Result<InclusiveRange<PackFormat>, String> {
        let (min, max) = (self.min.unwrap_or(PackFormat::of(0)), self.max.unwrap_or(PackFormat::of(0)));
        let (major_min, major_max) = (min.major, max.major);
        if min > max {
            return Err(format!("{context} min_format ({min}) is greater than max_format ({max})"));
        }
        if major_min > last_pre_minor && !require_old_field {
            if self.supported.is_some() {
                return Err(format!(
                    "{context} key {old_field} is deprecated starting from pack format {}. Remove {old_field} from your pack.mcmeta.",
                    last_pre_minor + 1
                ));
            }
            if has_pack_format_field && self.format.is_some() {
                if let Some(e) = self.validate_pack_format_for_range(major_min, major_max) {
                    return Err(e);
                }
            }
        } else {
            let Some(old) = self.supported else {
                return Err(format!(
                    "{context} declares support for format {major_min}, but game versions supporting formats 15 to {last_pre_minor} require a {old_field} field. Add \"{old_field}\": [{major_min}, {last_pre_minor}] or require a version greater or equal to {}.0.",
                    last_pre_minor + 1
                ));
            };
            if old.min_inclusive != major_min {
                return Err(format!("{context} version declaration mismatch between {old_field} (from {}) and min_format ({min})", old.min_inclusive));
            }
            if old.max_inclusive != major_max && old.max_inclusive != last_pre_minor {
                return Err(format!("{context} version declaration mismatch between {old_field} (up to {}) and max_format ({max})", old.max_inclusive));
            }
            if has_pack_format_field {
                if self.format.is_none() {
                    return Err(format!(
                        "{context} declares support for formats up to {last_pre_minor}, but game versions supporting formats 15 to {last_pre_minor} require a pack_format field. Add \"pack_format\": {major_min} or require a version greater or equal to {}.0.",
                        last_pre_minor + 1
                    ));
                }
                if let Some(e) = self.validate_pack_format_for_range(major_min, major_max) {
                    return Err(e);
                }
            }
        }
        Ok(InclusiveRange { min_inclusive: min, max_inclusive: max })
    }

    fn validate_old_format(&self, last_pre_minor: i32, has_pack_format_field: bool, context: &str, _old_field: &str) -> Result<InclusiveRange<PackFormat>, String> {
        let old = self.supported.unwrap_or(InclusiveRange::single(0));
        let (min, max) = (old.min_inclusive, old.max_inclusive);
        if max > last_pre_minor {
            return Err(format!("{context} declares support for version newer than {last_pre_minor}, but is missing mandatory fields min_format and max_format"));
        }
        if has_pack_format_field {
            if self.format.is_none() {
                return Err(format!(
                    "{context} declares support for formats up to {last_pre_minor}, but game versions supporting formats 15 to {last_pre_minor} require a pack_format field. Add \"pack_format\": {min} or require a version greater or equal to {}.0.",
                    last_pre_minor + 1
                ));
            }
            if let Some(e) = self.validate_pack_format_for_range(min, max) {
                return Err(e);
            }
        }
        Ok(InclusiveRange { min_inclusive: PackFormat::of(min), max_inclusive: PackFormat::of(max) })
    }

    fn validate_pack_format_for_range(&self, min: i32, max: i32) -> Option<String> {
        let main = self.format.unwrap_or(0);
        if main < min || main > max {
            Some(format!("Pack declared support for versions {min} to {max} but declared main format is {main}"))
        } else if main < 15 {
            Some("Multi-version packs cannot support minimum version of less than 15, since this will leave versions in range unable to load pack.".to_owned())
        } else {
            None
        }
    }
}

/// `PackMetadataSection`.
#[derive(Clone, Debug)]
pub struct PackMetadataSection {
    pub description: Component,
    pub supported_formats: InclusiveRange<PackFormat>,
}

impl PackMetadataSection {
    fn description(o: &Map<String, Value>, whole: &Value) -> Result<Component, String> {
        let d = o.get("description").ok_or_else(|| format!("No key description in MapLike[{}]", compact(whole)))?;
        Component::from_json(d).map_err(|e| e.to_string())
    }

    /// `PackMetadataSection.forPackType(type)` codec.
    pub fn decode(section: &Value, ty: PackType) -> Result<PackMetadataSection, String> {
        let o = section.as_object().ok_or_else(|| format!("Not a JSON object: {}", compact(section)))?;
        let last = PackFormat::last_pre_minor_version(ty);
        let description = PackMetadataSection::description(o, section);
        let formats = IntermediaryFormat::decode(o, false).and_then(|i| i.validate(last, true, false, "Pack", "supported_formats"));
        let (description, supported_formats) = join_errors(description, formats)?;
        Ok(PackMetadataSection { description, supported_formats })
    }

    /// `PackMetadataSection.FALLBACK_TYPE`: only the description, format `MAX_VALUE` (`PackCompatibility.UNKNOWN`).
    pub fn decode_fallback(section: &Value) -> Result<PackMetadataSection, String> {
        let o = section.as_object().ok_or_else(|| format!("Not a JSON object: {}", compact(section)))?;
        let description = PackMetadataSection::description(o, section)?;
        Ok(PackMetadataSection { description, supported_formats: InclusiveRange::single(PackFormat::of(i32::MAX)) })
    }
}

/// `FeatureFlagsMetadataSection` (`features.enabled`).
pub fn decode_feature_flags_section(section: &Value) -> Result<FeatureFlagSet, String> {
    let o = section.as_object().ok_or_else(|| format!("Not a JSON object: {}", compact(section)))?;
    let enabled = o.get("enabled").ok_or_else(|| format!("No key enabled in MapLike[{}]", compact(section)))?;
    feature_flags::decode_json(enabled)
}

/// `OverlayMetadataSection.OverlayEntry`.
#[derive(Clone, Debug)]
pub struct OverlayEntry {
    pub format: InclusiveRange<PackFormat>,
    pub overlay: String,
}

/// `OverlayMetadataSection`.
#[derive(Clone, Debug, Default)]
pub struct OverlayMetadataSection {
    pub overlays: Vec<OverlayEntry>,
}

impl OverlayMetadataSection {
    /// `codecForPackType(type)`: `{"entries": [{directory, min_format, max_format, formats}]}`.
    pub fn decode(section: &Value, ty: PackType) -> Result<OverlayMetadataSection, String> {
        let o = section.as_object().ok_or_else(|| format!("Not a JSON object: {}", compact(section)))?;
        let entries = o.get("entries").ok_or_else(|| format!("No key entries in MapLike[{}]", compact(section)))?;
        let list = entries.as_array().ok_or_else(|| format!("Not a list: {}", compact(entries)))?;
        let last = PackFormat::last_pre_minor_version(ty);
        // the codec decodes every entry first, then `validateHolderList` checks them against each other
        let mut parsed = Vec::new();
        for entry in list {
            let eo = entry.as_object().ok_or_else(|| format!("Not a JSON object: {}", compact(entry)))?;
            let format = IntermediaryFormat::decode(eo, true);
            let dir = eo
                .get("directory")
                .ok_or_else(|| format!("No key directory in MapLike[{}]", compact(entry)))
                .and_then(|d| d.as_str().ok_or_else(|| format!("Not a string: {}", compact(d))))
                .and_then(|d| if is_overlay_dir(d) { Ok(d.to_owned()) } else { Err(format!("{d} is not accepted directory name")) });
            let (format, dir) = join_errors(format, dir)?;
            parsed.push((format, dir));
        }
        let min_version = parsed.iter().map(|(f, _)| f.effective_min_major_version()).min().unwrap_or(i32::MAX);
        let mut overlays = Vec::new();
        for (format, dir) in parsed {
            if format.min.is_none() && format.max.is_none() && format.supported.is_none() {
                log::warn!("Unknown or broken overlay entry {dir}");
                continue;
            }
            let range = format.validate(last, false, min_version <= last, &format!("Overlay \"{dir}\""), "formats")?;
            overlays.push(OverlayEntry { format: range, overlay: dir });
        }
        Ok(OverlayMetadataSection { overlays })
    }

    /// `overlaysForVersion`.
    pub fn overlays_for_version(&self, version: PackFormat) -> Vec<String> {
        self.overlays.iter().filter(|e| e.format.is_value_in_range(version)).map(|e| e.overlay.clone()).collect()
    }
}

/// `DIR_VALIDATOR`: `[-_a-zA-Z0-9.]+`.
fn is_overlay_dir(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
}

/// `IdentifierPattern`: optional namespace / path regexes tested with `Pattern.asPredicate` (partial match).
#[derive(Clone, Debug)]
pub struct IdentifierPattern {
    namespace: Option<Regex>,
    path: Option<Regex>,
}

impl IdentifierPattern {
    pub fn namespace_matches(&self, namespace: &str) -> bool {
        self.namespace.as_ref().is_none_or(|r| r.is_match(namespace))
    }

    pub fn path_matches(&self, path: &str) -> bool {
        self.path.as_ref().is_none_or(|r| r.is_match(path))
    }
}

/// `ResourceFilterSection` (`filter.block`).
#[derive(Clone, Debug, Default)]
pub struct ResourceFilterSection {
    block_list: Vec<IdentifierPattern>,
}

impl ResourceFilterSection {
    pub fn decode(section: &Value) -> Result<ResourceFilterSection, String> {
        let o = section.as_object().ok_or_else(|| format!("Not a JSON object: {}", compact(section)))?;
        let block = o.get("block").ok_or_else(|| format!("No key block in MapLike[{}]", compact(section)))?;
        let list = block.as_array().ok_or_else(|| format!("Not a list: {}", compact(block)))?;
        let pattern = |po: &Map<String, Value>, key: &str| -> Result<Option<Regex>, String> {
            optional(po, key, |v| {
                let s = v.as_str().ok_or_else(|| format!("Not a string: {}", compact(v)))?;
                Regex::new(s).map_err(|e| format!("Invalid regex pattern '{s}': {e}"))
            })
        };
        let mut block_list = Vec::new();
        for entry in list {
            let po = entry.as_object().ok_or_else(|| format!("Not a JSON object: {}", compact(entry)))?;
            let (namespace, path) = join_errors(pattern(po, "namespace"), pattern(po, "path"))?;
            block_list.push(IdentifierPattern { namespace, path });
        }
        Ok(ResourceFilterSection { block_list })
    }

    pub fn is_namespace_filtered(&self, namespace: &str) -> bool {
        self.block_list.iter().any(|p| p.namespace_matches(namespace))
    }

    pub fn is_path_filtered(&self, path: &str) -> bool {
        self.block_list.iter().any(|p| p.path_matches(path))
    }

    pub fn is_filtered(&self, location: &Identifier) -> bool {
        self.is_path_filtered(location.path())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn server(v: Value) -> Result<PackMetadataSection, String> {
        PackMetadataSection::decode(&v, PackType::ServerData)
    }

    #[test]
    fn new_style_formats() {
        let s = server(json!({"description": "x", "min_format": 121, "max_format": [121, 3]})).unwrap();
        assert_eq!(s.supported_formats, InclusiveRange { min_inclusive: PackFormat::of(121), max_inclusive: PackFormat::of_minor(121, 3) });
        let s = server(json!({"description": "x", "min_format": 121, "max_format": 121})).unwrap();
        assert_eq!(s.supported_formats.max_inclusive.to_string(), "121.*");
    }

    #[test]
    fn old_style_formats_and_errors() {
        let s = server(json!({"description": "x", "pack_format": 48, "supported_formats": [40, 48]})).unwrap();
        assert_eq!(s.supported_formats.min_inclusive, PackFormat::of(40));
        // wrong declared format
        assert_eq!(
            server(json!({"description": "x", "pack_format": 49, "supported_formats": [40, 48]})).unwrap_err(),
            "Pack declared support for versions 40 to 48 but declared main format is 49"
        );
        assert_eq!(
            server(json!({"description": "x", "pack_format": 100})).unwrap_err(),
            "Pack declares support for version newer than 81, but is missing mandatory fields min_format and max_format"
        );
        assert_eq!(server(json!({"description": "x"})).unwrap_err(), "Pack could not be parsed, missing format version information");
        assert_eq!(
            server(json!({"description": "x", "min_format": 121})).unwrap_err(),
            "Pack missing field, must declare both min_format and max_format"
        );
        assert!(server(json!({"pack_format": 48})).unwrap_err().starts_with("No key description in MapLike[{"));
    }

    #[test]
    fn fallback_section_is_unknown_compatibility() {
        let f = PackMetadataSection::decode_fallback(&json!({"description": "d", "pack_format": "junk"})).unwrap();
        assert_eq!(f.supported_formats.min_inclusive.major, i32::MAX);
    }

    #[test]
    fn overlays() {
        let o = OverlayMetadataSection::decode(
            &json!({"entries": [{"directory": "a", "min_format": 121, "max_format": 121}, {"directory": "b", "min_format": 122, "max_format": 130}]}),
            PackType::ServerData,
        )
        .unwrap();
        assert_eq!(o.overlays_for_version(PackFormat::of(121)), vec!["a"]);
        assert!(OverlayMetadataSection::decode(&json!({"entries": [{"directory": "a/b", "min_format": 121, "max_format": 121}]}), PackType::ServerData)
            .unwrap_err()
            .contains("is not accepted directory name"));
    }

    #[test]
    fn filter_section() {
        let f = ResourceFilterSection::decode(&json!({"block": [{"namespace": "mine.*", "path": "tags/.*"}]})).unwrap();
        assert!(f.is_namespace_filtered("minecraft"));
        assert!(f.is_path_filtered("tags/block/x.json"));
        assert!(!f.is_path_filtered("recipe/x.json"));
    }
}
