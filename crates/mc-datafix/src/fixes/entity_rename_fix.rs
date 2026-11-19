//! `EntityRenameFix`: renames and rewrites entities with the tagged choice of `ENTITY` changing. Abstract in Java;
//! `fix` is the subclass's `fix(name, entity)`: the entity (typed with the input type of its old id, patched to the
//! new choice type) in, the new id and the typed entity out.

use crate::datafix::DataFix;
use crate::extra_data_fix_utils::patch_sub_type;
use crate::fixes::references::References;
use crate::schema::Schema;
use crate::ty::Type;
use crate::typed::Typed;
use crate::value::Value;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// The helpful `NullPointerException` text of `expectedType.equals(..)` on a missing choice.
pub(crate) fn null_pointer(variable: &str) -> String {
    format!(
        "java.lang.NullPointerException: Cannot invoke \"com.mojang.datafixers.types.Type.equals(Object, boolean, boolean)\" because \"{variable}\" is null"
    )
}

/// `new EntityRenameFix(name, outputSchema, changesType)` of the subclass `class`.
pub fn new(
    class: &str,
    name: &str,
    output_schema: &Schema,
    changes_type: bool,
    fix: impl Fn(&str, Typed) -> (String, Typed) + Send + Sync + 'static,
) -> DataFix {
    let name = name.to_string();
    let fix = Arc::new(fix);
    DataFix::new(class, output_schema, changes_type, move |df| {
        let old_type = df.input_schema().find_choice_type(&References::ENTITY);
        let new_type = df.output_schema().find_choice_type(&References::ENTITY);
        let patched_input_types: Mutex<HashMap<String, Type>> = Mutex::new(HashMap::new());
        let (old_type2, new_type2, fix) = (old_type.clone(), new_type.clone(), fix.clone());
        df.fix_type_everywhere_new(&name, old_type.ty(), new_type.ty(), move |ops, input| {
            let (old_name, old_value) = input.into_named();
            let cached = patched_input_types.lock().unwrap().get(&old_name).cloned();
            let old_entity_type = cached.unwrap_or_else(|| {
                let ty = old_type2.get(&old_name).unwrap_or_else(|| {
                    panic!(
                        "java.lang.NullPointerException: Cannot invoke \"com.mojang.datafixers.types.Type.all(com.mojang.datafixers.TypeRewriteRule, boolean, boolean)\" because \"type\" is null"
                    )
                });
                let patched = patch_sub_type(ty, old_type2.ty(), new_type2.ty());
                patched_input_types
                    .lock()
                    .unwrap()
                    .insert(old_name.clone(), patched.clone());
                patched
            });
            let (new_name, new_entity) = fix(&old_name, Typed::new(old_entity_type, *ops, old_value));
            let expected_type = new_type2
                .get(&new_name)
                .unwrap_or_else(|| panic!("{}", null_pointer("expectedType")));
            if !expected_type.equals_with(&new_entity.ty, true, true) {
                panic!(
                    "java.lang.IllegalStateException: Dynamic type check failed: {expected_type} not equal to {}",
                    new_entity.ty
                );
            }
            Value::named(new_name, new_entity.value)
        })
    })
}
