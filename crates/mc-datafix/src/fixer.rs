//! `DataFixerBuilder` and `DataFixerUpper` (the `DataFixer`): schema/fix registration and `update`.
//!
//! Not ported: `DataFixerBuilder.Result.optimize` (pre-warming the rewrite caches on an executor; the caches fill
//! lazily instead) and `DataFixerUpper.ERRORS_ARE_FATAL` (always `false` in vanilla).

use crate::datafix::DataFix;
use crate::dsl::TypeReference;
use crate::rewrite::{OptimizationRule, TypeRewriteRule};
use crate::schema::Schema;
use crate::ty::Type;
use mc_dfu::{Dyn, Dynamic, NBT};
use mc_nbt::{CompoundTag, Tag};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::Mutex;

/// `DataFixUtils.makeKey(version, subVersion)`.
pub fn make_key(version: i32, sub_version: i32) -> i32 {
    version * 10 + sub_version
}
/// `DataFixUtils.getVersion(key)`.
pub fn get_version(key: i32) -> i32 {
    key / 10
}
/// `DataFixUtils.getSubVersion(key)`.
pub fn get_sub_version(key: i32) -> i32 {
    key % 10
}

/// `DataFixerUpper.getLowestSchemaSameVersion`: the greatest registered key <= `version_key` (the first key when
/// `version_key` precedes all of them).
fn lowest_schema_same_version(schemas: &BTreeMap<i32, Schema>, version_key: i32) -> i32 {
    let first = *schemas.keys().next().expect("no schemas registered");
    if version_key < first {
        first
    } else {
        *schemas.range(..=version_key).next_back().unwrap().0
    }
}

/// One `FileFixerUpper.Builder.addFixer`: the file fix `name` (the simple name of the Java class) and the version of
/// the schema it was registered with. The fix itself is built where the files are handled (`mc-world`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct FileFixRegistration {
    pub name: &'static str,
    pub version: i32,
}

/// The `FileFixerUpper.Builder` state: the registered file fixes in registration order and
/// `latestFileFixerVersion`, the highest file schema version (every file schema carries a fix, so it is the highest
/// registered fix version).
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct FileFixerRegistry {
    fixes: Vec<FileFixRegistration>,
    latest_file_fixer_version: i32,
}

impl FileFixerRegistry {
    /// `FileFixerUpper.fileFixes`.
    pub fn fixes(&self) -> &[FileFixRegistration] {
        &self.fixes
    }

    /// `FileFixerUpper.latestFileFixerVersion`.
    pub fn latest_file_fixer_version(&self) -> i32 {
        self.latest_file_fixer_version
    }
}

pub struct DataFixerBuilder {
    data_version: i32,
    schemas: BTreeMap<i32, Schema>,
    global_list: Vec<DataFix>,
    fixer_versions: BTreeSet<i32>,
    file_fixer: FileFixerRegistry,
}

impl DataFixerBuilder {
    /// `new DataFixerBuilder(dataVersion)`.
    pub fn new(data_version: i32) -> DataFixerBuilder {
        DataFixerBuilder {
            data_version,
            schemas: BTreeMap::new(),
            global_list: Vec::new(),
            fixer_versions: BTreeSet::new(),
            file_fixer: FileFixerRegistry::default(),
        }
    }

    /// `addSchema(version, factory)`; `factory(versionKey, parent)` builds the schema (see `Schema::same`).
    pub fn add_schema(
        &mut self,
        version: i32,
        factory: impl FnOnce(i32, Option<Schema>) -> Schema,
    ) -> Schema {
        self.add_schema_sub(version, 0, factory)
    }

    /// `addSchema(version, subVersion, factory)`.
    pub fn add_schema_sub(
        &mut self,
        version: i32,
        sub_version: i32,
        factory: impl FnOnce(i32, Option<Schema>) -> Schema,
    ) -> Schema {
        let key = make_key(version, sub_version);
        let parent = if self.schemas.is_empty() {
            None
        } else {
            Some(self.schemas[&lowest_schema_same_version(&self.schemas, key - 1)].clone())
        };
        let schema = factory(key, parent);
        self.add_schema_object(schema.clone());
        schema
    }

    /// `addSchema(schema)`.
    pub fn add_schema_object(&mut self, schema: Schema) {
        self.schemas.insert(schema.version_key(), schema);
    }

    /// `addFixer(fix)`.
    pub fn add_fixer(&mut self, fix: DataFix) {
        let version = get_version(fix.version_key());
        if version > self.data_version {
            log::warn!(
                "Ignored fix registered for version: {version} as the DataVersion of the game is: {}",
                self.data_version
            );
        } else {
            self.fixer_versions.insert(fix.version_key());
            self.global_list.push(fix);
        }
    }

    /// `FileFixerUpper.Builder.addFixer(fix)` for the file fix `name` of the schema `version`: file fixes run in
    /// registration order, which must not go back in version and never past the game's data version.
    ///
    /// # Panics
    /// Like vanilla's `IllegalArgumentException`.
    pub fn add_file_fix(&mut self, name: &'static str, version: i32) {
        assert!(
            version <= self.data_version,
            "Tried to add too recent file fix for version: {version}. The data version of the game is: {}",
            self.data_version
        );
        if let Some(last) = self.file_fixer.fixes.last() {
            assert!(
                last.version <= version,
                "Tried to add too recent file fix for version: {version}. The most recent file fix version is {}",
                last.version
            );
        }
        self.file_fixer
            .fixes
            .push(FileFixRegistration { name, version });
        self.file_fixer.latest_file_fixer_version =
            self.file_fixer.latest_file_fixer_version.max(version);
    }

    /// `build()`: the data fixer and the file fix registrations.
    pub fn build_with_file_fixes(self) -> (DataFixer, FileFixerRegistry) {
        let file_fixer = self.file_fixer.clone();
        (self.build(), file_fixer)
    }

    /// `build().fixer()`.
    pub fn build(self) -> DataFixer {
        DataFixer {
            oldest_supported_version: oldest_supported_version(
                &self.schemas,
                &self.fixer_versions,
                &self.file_fixer,
            ),
            schemas: self.schemas,
            global_list: self.global_list,
            fixer_versions: self.fixer_versions,
            rules: Mutex::new(HashMap::new()),
        }
    }
}

/// The oldest data version [`DataFixer::update`] brings to the current one correctly: vanilla registers a data fix
/// with every schema but its first and the schemas of its file fixes, so any other schema key without a registered
/// fix is a fix that is not ported yet and data older than it would silently skip it (`i32::MIN` when there is none).
/// A fix of key `v * 10 + s` is skipped by data of every version below `ceil(key / 10)`: the fixes of a sub-version
/// still run for data of version `v`.
fn oldest_supported_version(
    schemas: &BTreeMap<i32, Schema>,
    fixer_versions: &BTreeSet<i32>,
    file_fixer: &FileFixerRegistry,
) -> i32 {
    schemas
        .keys()
        .skip(1)
        .filter(|key| !fixer_versions.contains(key))
        .filter(|key| !file_fixer.fixes.iter().any(|f| f.version * 10 == **key))
        .map(|key| (key + 9).div_euclid(10))
        .max()
        .unwrap_or(i32::MIN)
}

/// A data version below the part of the fix chain that is registered: updating it would skip fixes.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct UnsupportedVersion {
    pub from_version: i32,
    pub oldest_supported: i32,
}

impl std::fmt::Display for UnsupportedVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Data version {} is older than the oldest version that can be upgraded ({})",
            self.from_version, self.oldest_supported
        )
    }
}

impl std::error::Error for UnsupportedVersion {}

/// `DataFixerUpper`.
pub struct DataFixer {
    oldest_supported_version: i32,
    schemas: BTreeMap<i32, Schema>,
    global_list: Vec<DataFix>,
    fixer_versions: BTreeSet<i32>,
    rules: Mutex<HashMap<(i32, i32), TypeRewriteRule>>,
}

impl DataFixer {
    /// Drops the rules built so far; `update` builds them again on demand. A caller that updates over many different
    /// version ranges one after the other keeps its memory bounded this way.
    pub fn clear_rule_cache(&self) {
        self.rules.lock().unwrap().clear();
    }

    /// The oldest data version that [`DataFixer::update`] upgrades with every fix of the chain (`i32::MIN` once the
    /// whole chain is registered).
    pub fn oldest_supported_version(&self) -> i32 {
        self.oldest_supported_version
    }

    /// `Err` when `from_version` is below [`DataFixer::oldest_supported_version`].
    pub fn check_supported(&self, from_version: i32) -> Result<(), UnsupportedVersion> {
        if from_version < self.oldest_supported_version {
            return Err(UnsupportedVersion {
                from_version,
                oldest_supported: self.oldest_supported_version,
            });
        }
        Ok(())
    }

    /// The keys of the registered schemas, ascending.
    pub fn schema_keys(&self) -> impl Iterator<Item = i32> + '_ {
        self.schemas.keys().copied()
    }

    /// `globalList`: the registered fixes in registration order.
    pub fn fixes(&self) -> &[DataFix] {
        &self.global_list
    }

    /// `getSchema(key)`.
    pub fn get_schema(&self, key: i32) -> &Schema {
        &self.schemas[&lowest_schema_same_version(&self.schemas, key)]
    }

    fn get_type(&self, ty: &TypeReference, version: i32) -> Type {
        self.get_schema(make_key(version, 0)).get_type_raw(ty)
    }

    fn lowest_fix_same_version(&self, version_key: i32) -> i32 {
        let first = *self
            .fixer_versions
            .iter()
            .next()
            .expect("no fixes registered");
        if version_key < first {
            first - 1
        } else {
            *self
                .fixer_versions
                .range(..=version_key)
                .next_back()
                .unwrap()
        }
    }

    /// `getRule(version, newVersion)`.
    fn get_rule(&self, version: i32, new_version: i32) -> TypeRewriteRule {
        if version >= new_version {
            return TypeRewriteRule::nop();
        }
        if let Some(r) = self.rules.lock().unwrap().get(&(version, new_version)) {
            return r.clone();
        }
        let expanded_version = self.lowest_fix_same_version(make_key(version, 0));
        let mut rules = Vec::new();
        for fix in &self.global_list {
            let expanded_fix_version = fix.version_key();
            let fix_version = get_version(expanded_fix_version);
            if expanded_fix_version > expanded_version && fix_version <= new_version {
                let rule = fix.get_rule();
                if !rule.is_nop() {
                    rules.push(rule);
                }
            }
        }
        let rule = TypeRewriteRule::seq_list(rules);
        self.rules
            .lock()
            .unwrap()
            .insert((version, new_version), rule.clone());
        rule
    }

    /// `DataFixer.update(type, input, version, newVersion)`: any codec error is logged and the input returned
    /// unchanged (partial results are used when present).
    pub fn update(
        &self,
        ty: &TypeReference,
        input: Dynamic,
        version: i32,
        new_version: i32,
    ) -> Dynamic {
        if version >= new_version {
            return input;
        }
        let data_type = self.get_type(ty, version);
        let new_type = self.get_type(ty, new_version);
        let rule = self.get_rule(version, new_version);
        let read = data_type.read_and_write(
            &input.ops,
            &new_type,
            &rule,
            OptimizationRule::Standard,
            &input.value,
        );
        if let Some(m) = read.error_message() {
            log::error!("{m}");
        }
        let ops = input.ops;
        let result = read.result_or_partial().unwrap_or(input.value);
        Dynamic::new(ops, result)
    }

    /// `update` of an NBT compound: the unchanged input when the update fails, like [`DataFixer::update`].
    pub fn update_tag(
        &self,
        ty: &TypeReference,
        tag: &CompoundTag,
        version: i32,
        new_version: i32,
    ) -> CompoundTag {
        let input = Dynamic::new(NBT, Dyn::from(&Tag::Compound(tag.clone())));
        match self.update(ty, input, version, new_version).value.to_tag() {
            Ok(Tag::Compound(c)) => c,
            Ok(other) => panic!("data fixing turned a compound into {other:?}"),
            Err(e) => panic!("data fixing produced an invalid NBT tree: {e}"),
        }
    }
}
