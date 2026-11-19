//! `net.minecraft.util.datafix.ExtraDataFixUtils`.

use crate::finder::OpticFinder;
use crate::rewrite::{BitSet, OptimizationRule, RewriteResult, TypeRewriteRule, View};
use crate::ty::Type;
use crate::typed::Typed;
use crate::util::read_typed_or_throw;
use crate::value::Value;
use mc_dfu::{Dynamic, Ops};

/// `ExtraDataFixUtils.fixBlockPos(pos)`: `{X, Y, Z}` numbers become an int list `[X, Y, Z]`; anything else is kept.
pub fn fix_block_pos(pos: &Dynamic) -> Dynamic {
    let (x, y, z) = (
        pos.get("X").as_number().result(),
        pos.get("Y").as_number().result(),
        pos.get("Z").as_number().result(),
    );
    match (x, y, z) {
        (Some(x), Some(y), Some(z)) => {
            create_block_pos(pos, x.int_value(), y.int_value(), z.int_value())
        }
        _ => pos.clone(),
    }
}

/// `ExtraDataFixUtils.fixInlineBlockPos(input, fieldX, fieldY, fieldZ, newField)`.
pub fn fix_inline_block_pos(
    input: &Dynamic,
    field_x: &str,
    field_y: &str,
    field_z: &str,
    new_field: &str,
) -> Dynamic {
    let (x, y, z) = (
        input.get(field_x).as_number().result(),
        input.get(field_y).as_number().result(),
        input.get(field_z).as_number().result(),
    );
    match (x, y, z) {
        (Some(x), Some(y), Some(z)) => input.remove(field_x).remove(field_y).remove(field_z).set(
            new_field,
            create_block_pos(input, x.int_value(), y.int_value(), z.int_value()),
        ),
        _ => input.clone(),
    }
}

/// `ExtraDataFixUtils.createBlockPos(dynamic, x, y, z)`.
pub fn create_block_pos(dynamic: &Dynamic, x: i32, y: i32, z: i32) -> Dynamic {
    dynamic.create_int_list([x, y, z])
}

/// `ExtraDataFixUtils.fixStringField(dynamic, fieldName, fix)`: rewrites `fieldName` when it is a string.
pub fn fix_string_field(
    dynamic: &Dynamic,
    field_name: &str,
    fix: impl FnOnce(&str) -> String,
) -> Dynamic {
    dynamic.update(field_name, |field| match field.as_string().result() {
        Some(string) => dynamic.create_string(fix(&string)),
        None => field,
    })
}

/// `ExtraDataFixUtils.getPlainTranslationKey(textComponent)`: the `translate` string of a component whose only field
/// it is.
pub fn get_plain_translation_key(text_component: &Dynamic) -> Option<String> {
    text_component
        .get_map_values()
        .result()
        .filter(|fields| fields.len() == 1)
        .and_then(|_| text_component.get("translate").as_string().result())
}

/// `ExtraDataFixUtils.readAndSet(target, optic, value)`: reads `value` as the focused type (partial results accepted)
/// and sets it.
#[track_caller]
pub fn read_and_set(target: Typed, optic: &OpticFinder, value: &Dynamic) -> Typed {
    target.set_typed(optic, read_typed_or_throw(optic.ty(), value, true))
}

/// `ExtraDataFixUtils.cast(type, typed)`: relabel `typed` with `ty` (no check, no conversion).
pub fn cast(ty: &Type, typed: Typed) -> Typed {
    Typed::new(ty.clone(), typed.ops, typed.value)
}

/// `ExtraDataFixUtils.cast(type, value, ops)`.
pub fn cast_value(ty: &Type, value: Value, ops: Ops) -> Typed {
    Typed::new(ty.clone(), ops, value)
}

/// `ExtraDataFixUtils.patchSubType(type, find, replace)`: `ty` with every occurrence of `find` replaced by `replace`.
pub fn patch_sub_type(ty: &Type, find: &Type, replace: &Type) -> Type {
    let view = RewriteResult::create(
        View::create("Patcher", find, replace, |_, _| {
            panic!("UnsupportedOperationException: patched types are never evaluated")
        }),
        BitSet::new(),
    );
    let rule = TypeRewriteRule::everywhere(
        TypeRewriteRule::if_same(find, view),
        OptimizationRule::Nop,
        true,
        true,
    );
    ty.all(&rule, true, false).view.new_type().clone()
}

/// `ExtraDataFixUtils.chainAllFilters(fixers...)`.
pub fn chain_all_filters(
    fixers: Vec<Box<dyn Fn(Typed) -> Typed + Send + Sync>>,
) -> impl Fn(Typed) -> Typed + Send + Sync {
    move |mut typed| {
        for fixer in &fixers {
            typed = fixer(typed);
        }
        typed
    }
}

/// `ExtraDataFixUtils.dyeColorIdToName(id)`.
pub fn dye_color_id_to_name(id: i32) -> &'static str {
    match id {
        1 => "orange",
        2 => "magenta",
        3 => "light_blue",
        4 => "yellow",
        5 => "lime",
        6 => "pink",
        7 => "gray",
        8 => "light_gray",
        9 => "cyan",
        10 => "purple",
        11 => "blue",
        12 => "brown",
        13 => "green",
        14 => "red",
        15 => "black",
        _ => "white",
    }
}
