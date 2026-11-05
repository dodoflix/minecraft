//! The `net.minecraft.util.Util` helpers data fixes use to round-trip typed values through `Dynamic`s.

use crate::ty::Type;
use crate::typed::Typed;
use mc_dfu::Dynamic;

/// `Util.readTypedOrThrow(type, dynamic, acceptPartial)`; panics with the codec error (Java: a `ReportedException`).
#[track_caller]
pub fn read_typed_or_throw(ty: &Type, dynamic: &Dynamic, accept_partial: bool) -> Typed {
    let result = ty.read_typed(dynamic).map(|(typed, _)| typed);
    let r = if accept_partial { result.into_partial_result() } else { result.into_result() };
    r.unwrap_or_else(|e| panic!("Reading type: {e}\nData: {}\nType: {ty}", dynamic.value))
}

/// `Util.writeAndReadTypedOrThrow(typed, newType, function)`: write `typed`, transform the `Dynamic`, read it back as
/// `new_type` (partial results accepted).
#[track_caller]
pub fn write_and_read_typed_or_throw(typed: &Typed, new_type: &Type, f: impl FnOnce(Dynamic) -> Dynamic) -> Typed {
    let dynamic = typed.write().get_or_throw();
    read_typed_or_throw(new_type, &f(dynamic), true)
}
