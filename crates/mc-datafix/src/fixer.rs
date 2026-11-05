//! `DataFixerBuilder` and `DataFixerUpper` (the `DataFixer`): schema/fix registration and `update`.
//!
//! Not ported: `DataFixerBuilder.Result.optimize` (pre-warming the rewrite caches on an executor; the caches fill
//! lazily instead) and `DataFixerUpper.ERRORS_ARE_FATAL` (always `false` in vanilla).

use crate::dsl::TypeReference;
use crate::datafix::DataFix;
use crate::rewrite::{OptimizationRule, TypeRewriteRule};
use crate::schema::Schema;
use crate::ty::Type;
use mc_dfu::Dynamic;
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
    if version_key < first { first } else { *schemas.range(..=version_key).next_back().unwrap().0 }
}

pub struct DataFixerBuilder {
    data_version: i32,
    schemas: BTreeMap<i32, Schema>,
    global_list: Vec<DataFix>,
    fixer_versions: BTreeSet<i32>,
    unported: Vec<(i32, String)>,
}

impl DataFixerBuilder {
    /// `new DataFixerBuilder(dataVersion)`.
    pub fn new(data_version: i32) -> DataFixerBuilder {
        DataFixerBuilder { data_version, schemas: BTreeMap::new(), global_list: Vec::new(), fixer_versions: BTreeSet::new(), unported: Vec::new() }
    }

    /// `addSchema(version, factory)`; `factory(versionKey, parent)` builds the schema (see `Schema::same`).
    pub fn add_schema(&mut self, version: i32, factory: impl FnOnce(i32, Option<Schema>) -> Schema) -> Schema {
        self.add_schema_sub(version, 0, factory)
    }

    /// `addSchema(version, subVersion, factory)`.
    pub fn add_schema_sub(&mut self, version: i32, sub_version: i32, factory: impl FnOnce(i32, Option<Schema>) -> Schema) -> Schema {
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
            log::warn!("Ignored fix registered for version: {version} as the DataVersion of the game is: {}", self.data_version);
        } else {
            self.fixer_versions.insert(fix.version_key());
            self.global_list.push(fix);
        }
    }

    /// Marks a fix of `DataFixers.addFixers` that is not ported yet: `step_is_complete(version)` stays false and
    /// the differential harness skips that step. Remove the call when the fix is registered with `add_fixer`.
    pub fn unported(&mut self, schema: &Schema, fix: &str) {
        self.unported.push((get_version(schema.version_key()), fix.to_string()));
    }

    /// `build().fixer()`.
    pub fn build(self) -> DataFixer {
        DataFixer {
            schemas: self.schemas,
            global_list: self.global_list,
            fixer_versions: self.fixer_versions,
            unported: self.unported,
            rules: Mutex::new(HashMap::new()),
        }
    }
}

/// `DataFixerUpper`.
pub struct DataFixer {
    schemas: BTreeMap<i32, Schema>,
    global_list: Vec<DataFix>,
    fixer_versions: BTreeSet<i32>,
    unported: Vec<(i32, String)>,
    rules: Mutex<HashMap<(i32, i32), TypeRewriteRule>>,
}

impl DataFixer {
    /// `getSchema(key)`.
    pub fn get_schema(&self, key: i32) -> &Schema {
        &self.schemas[&lowest_schema_same_version(&self.schemas, key)]
    }

    fn get_type(&self, ty: &TypeReference, version: i32) -> Type {
        self.get_schema(make_key(version, 0)).get_type_raw(ty)
    }

    fn lowest_fix_same_version(&self, version_key: i32) -> i32 {
        let first = *self.fixer_versions.iter().next().expect("no fixes registered");
        if version_key < first { first - 1 } else { *self.fixer_versions.range(..=version_key).next_back().unwrap() }
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
        self.rules.lock().unwrap().insert((version, new_version), rule.clone());
        rule
    }

    /// `DataFixer.update(type, input, version, newVersion)`: any codec error is logged and the input returned
    /// unchanged (partial results are used when present).
    pub fn update(&self, ty: &TypeReference, input: Dynamic, version: i32, new_version: i32) -> Dynamic {
        if version >= new_version {
            return input;
        }
        let data_type = self.get_type(ty, version);
        let new_type = self.get_type(ty, new_version);
        let rule = self.get_rule(version, new_version);
        let read = data_type.read_and_write(&input.ops, &new_type, &rule, OptimizationRule::Standard, &input.value);
        if let Some(m) = read.error_message() {
            log::error!("{m}");
        }
        let ops = input.ops;
        let result = read.result_or_partial().unwrap_or(input.value);
        Dynamic::new(ops, result)
    }

    // ---- per-version stepping (differential testing) ------------------------------------------------------------

    /// Sorted distinct data versions that have at least one registered fix.
    pub fn step_versions(&self) -> Vec<i32> {
        self.fixer_versions.iter().map(|k| get_version(*k)).collect::<BTreeSet<_>>().into_iter().collect()
    }

    /// Number of fixes registered for data version `k` (all sub-versions).
    pub fn fixes_in_step(&self, k: i32) -> usize {
        self.global_list.iter().filter(|f| get_version(f.version_key()) == k).count()
    }

    /// `false` while a fix of version `k` is still marked [`DataFixerBuilder::unported`].
    pub fn step_is_complete(&self, k: i32) -> bool {
        !self.unported.iter().any(|(v, _)| *v == k)
    }

    /// The fixes (class names) of version `k` not ported yet.
    pub fn unported_in_step(&self, k: i32) -> Vec<&str> {
        self.unported.iter().filter(|(v, _)| *v == k).map(|(_, n)| n.as_str()).collect()
    }

    /// Like `update`, but applying only the fixes registered at data version `k` (sub-versions k.0..k.9): input
    /// type = schema as of the key just before `k`, output type = schema as of `k.9`.
    pub fn update_step(&self, ty: &TypeReference, input: Dynamic, k: i32) -> Dynamic {
        let in_schema = self.get_schema(make_key(k, 0) - 1);
        let out_schema = self.get_schema(make_key(k, 9));
        let rules: Vec<TypeRewriteRule> = self
            .global_list
            .iter()
            .filter(|f| get_version(f.version_key()) == k)
            .map(DataFix::get_rule)
            .filter(|r| !r.is_nop())
            .collect();
        let rule = TypeRewriteRule::seq_list(rules);
        let read = in_schema.get_type_raw(ty).read_and_write(
            &input.ops,
            &out_schema.get_type_raw(ty),
            &rule,
            OptimizationRule::Standard,
            &input.value,
        );
        if let Some(m) = read.error_message() {
            log::error!("{m}");
        }
        let ops = input.ops;
        Dynamic::new(ops, read.result_or_partial().unwrap_or(input.value))
    }
}
