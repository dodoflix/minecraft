//! `net.minecraft.util.datafix.ExtraDataFixUtils` (the `Dynamic`-based helpers are added with the `Dynamic` API).

use crate::rewrite::{BitSet, OptimizationRule, RewriteResult, TypeRewriteRule, View};
use crate::ty::Type;
use crate::typed::Typed;
use crate::value::Value;
use mc_dfu::Ops;

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
        View::create("Patcher", find, replace, |_, _| panic!("UnsupportedOperationException: patched types are never evaluated")),
        BitSet::new(),
    );
    let rule = TypeRewriteRule::everywhere(TypeRewriteRule::if_same(find, view), OptimizationRule::Nop, true, true);
    ty.all(&rule, true, false).view.new_type().clone()
}

/// `ExtraDataFixUtils.chainAllFilters(fixers...)`.
pub fn chain_all_filters(fixers: Vec<Box<dyn Fn(Typed) -> Typed + Send + Sync>>) -> impl Fn(Typed) -> Typed + Send + Sync {
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
