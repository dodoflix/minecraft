//! `GameTestInstance` (`test_instance` registry) with its two types, `FunctionGameTestInstance` (`function`) and
//! `BlockBasedTestInstance` (`block_based`), and `TestData`.

use super::game_test_exception::GameTestException;
use super::game_test_helper::GameTestHelper;
use super::test_environment_definition::{EnvironmentRef, decode_environment_ref};
use crate::registry::codec::RegistryDecodeContext;
use crate::registry::decoders::{DYNAMIC_REGISTRY_DECODERS, DecodedEntry, DynamicRegistryDecoder};
use crate::registry::{RegistryAccess, keys};
use crate::translatable;
use crate::world::level::DimensionKey;
use mc_core::ident::Identifier;
use mc_core::rotation::Rotation;
use mc_text::{ChatFormatting, Component};
use serde_json::Value;
use std::sync::Arc;

/// `TestData<EnvironmentType>`.
#[derive(Clone)]
pub struct TestData<E> {
    pub environment: E,
    pub dimension: DimensionKey,
    pub structure: Identifier,
    pub max_ticks: i32,
    pub setup_ticks: i32,
    pub required: bool,
    pub rotation: Rotation,
    pub manual_only: bool,
    pub max_attempts: i32,
    pub required_successes: i32,
    pub sky_access: bool,
    pub padding: i32,
}

pub enum GameTestInstanceKind {
    /// `FunctionGameTestInstance`: the `TEST_FUNCTION` key to run.
    Function(Identifier),
    /// `BlockBasedTestInstance`.
    BlockBased,
}

pub struct GameTestInstance {
    pub kind: GameTestInstanceKind,
    info: TestData<EnvironmentRef>,
}

/// `Holder.Reference<GameTestInstance>`: the registry key and the value.
#[derive(Clone)]
pub struct TestHolder {
    pub key: Identifier,
    pub value: Arc<GameTestInstance>,
}

impl PartialEq for TestHolder {
    fn eq(&self, other: &Self) -> bool {
        self.key == other.key
    }
}

impl Eq for TestHolder {}

impl std::hash::Hash for TestHolder {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.key.hash(state);
    }
}

impl TestHolder {
    /// `Holder.getRegisteredName()`.
    pub fn registered_name(&self) -> String {
        self.key.to_string()
    }
}

/// `registryAccess.get(testKey)`: the instance registered under `key`.
pub fn lookup_test(registries: &RegistryAccess, key: &Identifier) -> Option<TestHolder> {
    let entry = registries
        .registry(keys::TEST_INSTANCE)?
        .entry_by_key(key)?;
    let value = entry.value.clone()?.downcast::<GameTestInstance>().ok()?;
    Some(TestHolder {
        key: key.clone(),
        value,
    })
}

/// Every `test_instance` entry in registry order (`listElements()`).
pub fn all_tests(registries: &RegistryAccess) -> Vec<TestHolder> {
    let Some(registry) = registries.registry(keys::TEST_INSTANCE) else {
        return Vec::new();
    };
    registry
        .entries()
        .iter()
        .filter_map(|e| {
            Some(TestHolder {
                key: e.id.clone(),
                value: e.value.clone()?.downcast::<GameTestInstance>().ok()?,
            })
        })
        .collect()
}

impl GameTestInstance {
    pub fn info(&self) -> &TestData<EnvironmentRef> {
        &self.info
    }

    pub fn batch(&self) -> &EnvironmentRef {
        &self.info.environment
    }

    pub fn dimension(&self) -> &DimensionKey {
        &self.info.dimension
    }

    pub fn structure(&self) -> &Identifier {
        &self.info.structure
    }

    pub fn max_ticks(&self) -> i32 {
        self.info.max_ticks
    }

    pub fn setup_ticks(&self) -> i32 {
        self.info.setup_ticks
    }

    pub fn required(&self) -> bool {
        self.info.required
    }

    pub fn manual_only(&self) -> bool {
        self.info.manual_only
    }

    pub fn max_attempts(&self) -> i32 {
        self.info.max_attempts
    }

    pub fn required_successes(&self) -> i32 {
        self.info.required_successes
    }

    pub fn sky_access(&self) -> bool {
        self.info.sky_access
    }

    pub fn rotation(&self) -> Rotation {
        self.info.rotation
    }

    pub fn padding(&self) -> i32 {
        self.info.padding
    }

    /// `run(helper)`.
    pub fn run(&self, helper: &mut GameTestHelper<'_>) -> Result<(), GameTestException> {
        match &self.kind {
            GameTestInstanceKind::Function(function) => {
                let known = helper
                    .registries()
                    .registry("minecraft:test_function")
                    .is_some_and(|r| r.contains(function));
                match super::builtin_test_functions::find(function).filter(|_| known) {
                    Some(run) => run(helper),
                    None => Err(GameTestException::Unknown {
                        reason: format!("Trying to access missing test function: {function}"),
                    }),
                }
            }
            GameTestInstanceKind::BlockBased => super::block_based_test_instance::run(helper),
        }
    }

    fn type_description(&self) -> Component {
        match self.kind {
            GameTestInstanceKind::Function(_) => translatable!("test_instance.type.function"),
            GameTestInstanceKind::BlockBased => translatable!("test_instance.type.block_based"),
        }
    }

    /// `describe()`.
    pub fn describe(&self) -> Component {
        match &self.kind {
            GameTestInstanceKind::Function(function) => self
                .describe_type()
                .append(description_row(
                    "test_instance.description.function",
                    Component::literal(function.to_string()),
                ))
                .append(self.describe_info()),
            GameTestInstanceKind::BlockBased => self.describe_type().append(self.describe_info()),
        }
    }

    fn describe_type(&self) -> Component {
        description_row("test_instance.description.type", self.type_description())
    }

    fn describe_info(&self) -> Component {
        description_row(
            "test_instance.description.structure",
            Component::literal(self.info.structure.to_string()),
        )
        .append(description_row(
            "test_instance.description.batch",
            Component::literal(self.info.environment.registered_name()),
        ))
        .append(description_row(
            "test_instance.description.dimension",
            Component::literal(self.info.dimension.to_string()),
        ))
    }
}

/// `descriptionRow(translationKey, value)`.
fn description_row(key: &str, value: Component) -> Component {
    Component::translatable(
        key.to_owned(),
        vec![value.with_format(ChatFormatting::Blue).into()],
    )
    .append(Component::literal("\n"))
}

// ---- codec ----

fn positive_int(value: &Value) -> Result<i32, String> {
    let n = value
        .as_f64()
        .ok_or_else(|| format!("Not a number: {value}"))? as i32;
    if n >= 1 {
        Ok(n)
    } else {
        Err(format!("Value must be positive: {n}"))
    }
}

fn optional<'a, T>(
    json: &'a Value,
    key: &str,
    default: T,
    read: impl FnOnce(&'a Value) -> Result<T, String>,
) -> Result<T, String> {
    match json.get(key) {
        None => Ok(default),
        Some(v) => read(v),
    }
}

fn boolean(value: &Value) -> Result<bool, String> {
    match value {
        Value::Bool(b) => Ok(*b),
        Value::Number(n) => Ok(n.as_i64().unwrap_or(0) as i8 != 0),
        _ => Err(format!("Not a boolean: {value}")),
    }
}

/// `TestData.CODEC` (a `MapCodec`: the fields sit next to the instance's own).
fn decode_test_data(
    json: &Value,
    ctx: &RegistryDecodeContext,
) -> Result<TestData<EnvironmentRef>, String> {
    let environment = decode_environment_ref(
        json.get("environment")
            .ok_or_else(|| format!("No key environment in MapLike[{json}]"))?,
        ctx,
    )?;
    let dimension = optional(
        json,
        "dimension",
        Identifier::parse("minecraft:overworld").map_err(|e| e.to_string())?,
        |v| Identifier::read(v.as_str().ok_or_else(|| format!("Not a string: {v}"))?),
    )?;
    let structure = Identifier::read(
        json.get("structure")
            .ok_or_else(|| format!("No key structure in MapLike[{json}]"))?
            .as_str()
            .ok_or_else(|| "Not a string: structure".to_owned())?,
    )?;
    let max_ticks = positive_int(
        json.get("max_ticks")
            .ok_or_else(|| format!("No key max_ticks in MapLike[{json}]"))?,
    )?;
    let setup_ticks = optional(json, "setup_ticks", 0, |v| {
        let n = v.as_f64().ok_or_else(|| format!("Not a number: {v}"))? as i32;
        if n >= 0 {
            Ok(n)
        } else {
            Err(format!("Value must be non-negative: {n}"))
        }
    })?;
    let required = optional(json, "required", true, boolean)?;
    let rotation = optional(json, "rotation", Rotation::None, |v| {
        let name = v.as_str().ok_or_else(|| format!("Not a string: {v}"))?;
        Rotation::by_name(name).ok_or_else(|| format!("Unknown element name:{name}"))
    })?;
    let manual_only = optional(json, "manual_only", false, boolean)?;
    let max_attempts = optional(json, "max_attempts", 1, positive_int)?;
    let required_successes = optional(json, "required_successes", 1, positive_int)?;
    let sky_access = optional(json, "sky_access", false, boolean)?;
    let padding = optional(json, "padding", 0, |v| {
        let n = v.as_f64().ok_or_else(|| format!("Not a number: {v}"))? as i32;
        if (0..=128).contains(&n) {
            Ok(n)
        } else {
            Err(format!("Value {n} outside of range [0:128]"))
        }
    })?;
    Ok(TestData {
        environment,
        dimension,
        structure,
        max_ticks,
        setup_ticks,
        required,
        rotation,
        manual_only,
        max_attempts,
        required_successes,
        sky_access,
        padding,
    })
}

/// `GameTestInstance.DIRECT_CODEC` (dispatch on `type` through `TEST_INSTANCE_TYPE`).
pub fn decode_instance(
    json: &Value,
    ctx: &RegistryDecodeContext,
) -> Result<GameTestInstance, String> {
    if !json.is_object() {
        return Err(format!("Not a JSON object: {json}"));
    }
    let ty = json
        .get("type")
        .ok_or_else(|| format!("No key type in MapLike[{json}]"))?;
    let ty = Identifier::read(ty.as_str().ok_or_else(|| format!("Not a string: {ty}"))?)?;
    let kind = match ty.to_string().as_str() {
        "minecraft:function" => {
            let function = json
                .get("function")
                .ok_or_else(|| format!("No key function in MapLike[{json}]"))?;
            GameTestInstanceKind::Function(Identifier::read(
                function
                    .as_str()
                    .ok_or_else(|| format!("Not a string: {function}"))?,
            )?)
        }
        "minecraft:block_based" => GameTestInstanceKind::BlockBased,
        other => {
            return Err(format!(
                "Unknown registry key in ResourceKey[minecraft:root / minecraft:test_instance_type]: {other}"
            ));
        }
    };
    Ok(GameTestInstance {
        kind,
        info: decode_test_data(json, ctx)?,
    })
}

fn decode(json: &Value, ctx: &RegistryDecodeContext) -> Result<DecodedEntry, String> {
    Ok(DecodedEntry::new(decode_instance(json, ctx)?))
}

#[linkme::distributed_slice(DYNAMIC_REGISTRY_DECODERS)]
static DECODER: DynamicRegistryDecoder = DynamicRegistryDecoder {
    registry: keys::TEST_INSTANCE,
    decode,
};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::codec::{Lookup, RegistryLookup};
    use serde_json::json;

    struct All;

    impl RegistryLookup for All {
        fn contains(&self, _registry: &str, _id: &Identifier) -> Lookup {
            Lookup::Yes
        }
        fn tag_exists(&self, _registry: &str, _tag: &Identifier) -> Lookup {
            Lookup::Yes
        }
    }

    #[test]
    fn decodes_the_vanilla_always_pass_instance() {
        let json = json!({"type":"minecraft:function","environment":"minecraft:default","function":"minecraft:always_pass","max_ticks":1,"required":false,"setup_ticks":1,"structure":"minecraft:empty"});
        let instance = decode_instance(&json, &RegistryDecodeContext::new(&All)).unwrap();
        assert!(
            matches!(&instance.kind, GameTestInstanceKind::Function(f) if f.to_string() == "minecraft:always_pass")
        );
        assert_eq!(
            (
                instance.max_ticks(),
                instance.setup_ticks(),
                instance.required(),
                instance.max_attempts(),
                instance.padding()
            ),
            (1, 1, false, 1, 0)
        );
        assert_eq!(instance.rotation(), Rotation::None);
        assert_eq!(instance.dimension().to_string(), "minecraft:overworld");
        assert_eq!(instance.batch().registered_name(), "minecraft:default");
    }

    #[test]
    fn rejects_invalid_values_like_the_codec() {
        let ctx = RegistryDecodeContext::new(&All);
        let base = json!({"type":"minecraft:block_based","environment":"minecraft:default","max_ticks":5,"structure":"minecraft:empty"});
        assert!(decode_instance(&base, &ctx).is_ok());
        for (key, value) in [
            ("max_ticks", json!(0)),
            ("padding", json!(129)),
            ("max_attempts", json!(0)),
            ("rotation", json!("sideways")),
        ] {
            let mut bad = base.clone();
            bad[key] = value;
            assert!(
                decode_instance(&bad, &ctx).is_err(),
                "{key} should be rejected"
            );
        }
        assert!(
            decode_instance(&json!({"type":"minecraft:nope"}), &ctx)
                .err()
                .unwrap()
                .contains("test_instance_type")
        );
    }
}
