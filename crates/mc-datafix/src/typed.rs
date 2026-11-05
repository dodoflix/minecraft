//! `Typed<A>`: a value with its type and ops. Immutable in Java; here the updating methods consume `self` (use
//! `.clone()` when the original is still needed) and the reading methods borrow it.
//!
//! Every method that resolves an [`OpticFinder`] panics when the finder does not match the type (Java:
//! `Either.orThrow()` -> `IllegalArgumentException`), exactly like vanilla fixes that mis-name a field.

use crate::finder::{OpticFinder, remainder_finder};
use crate::optic::TypedOptic;
use crate::ty::Type;
use crate::value::Value;
use mc_dfu::{DataResult, Dynamic, Ops};

#[derive(Clone, Debug)]
pub struct Typed {
    pub ty: Type,
    pub ops: Ops,
    pub value: Value,
}

impl Typed {
    pub fn new(ty: Type, ops: Ops, value: Value) -> Typed {
        Typed { ty, ops, value }
    }

    pub fn get_type(&self) -> &Type {
        &self.ty
    }
    pub fn get_ops(&self) -> &Ops {
        &self.ops
    }
    pub fn get_value(&self) -> &Value {
        &self.value
    }
    pub fn into_value(self) -> Value {
        self.value
    }

    #[track_caller]
    fn find(&self, f: &OpticFinder, result: &Type, recurse: bool) -> TypedOptic {
        match f.find_type(&self.ty, result, recurse) {
            Ok(o) => o,
            Err(e) => panic!("{e}"),
        }
    }

    // ---- reading ------------------------------------------------------------------------------------------------

    /// `typed.get(optic)`.
    #[track_caller]
    pub fn get(&self, f: &OpticFinder) -> Value {
        self.find(f, f.ty(), false)
            .first(&self.value)
            .unwrap_or_else(|| panic!("typed.get: no value found for a finder of type {}", f.ty()))
    }

    /// `typed.getTyped(optic)`.
    #[track_caller]
    pub fn get_typed(&self, f: &OpticFinder) -> Typed {
        let o = self.find(f, f.ty(), false);
        let v = o.first(&self.value).unwrap_or_else(|| panic!("typed.getTyped: no value found for a finder of type {}", f.ty()));
        Typed::new(o.a_type().clone(), self.ops, v)
    }

    /// `typed.getOptional(optic)`.
    #[track_caller]
    pub fn get_optional(&self, f: &OpticFinder) -> Option<Value> {
        self.find(f, f.ty(), false).first(&self.value)
    }

    /// `typed.getOrCreate(optic)`.
    #[track_caller]
    pub fn get_or_create(&self, f: &OpticFinder) -> Value {
        self.get_optional(f)
            .or_else(|| f.ty().point(&self.ops))
            .unwrap_or_else(|| panic!("Could not create default value for type: {}", f.ty()))
    }

    /// `typed.getOrDefault(optic, def)`.
    #[track_caller]
    pub fn get_or_default(&self, f: &OpticFinder, def: Value) -> Value {
        self.get_optional(f).unwrap_or(def)
    }

    /// `typed.getOptionalTyped(optic)`.
    #[track_caller]
    pub fn get_optional_typed(&self, f: &OpticFinder) -> Option<Typed> {
        let o = self.find(f, f.ty(), false);
        o.first(&self.value).map(|v| Typed::new(o.a_type().clone(), self.ops, v))
    }

    /// `typed.getOrCreateTyped(optic)`.
    #[track_caller]
    pub fn get_or_create_typed(&self, f: &OpticFinder) -> Typed {
        self.get_optional_typed(f)
            .or_else(|| f.ty().point_typed(&self.ops))
            .unwrap_or_else(|| panic!("Could not create default value for type: {}", f.ty()))
    }

    /// `typed.getAllTyped(optic)`.
    #[track_caller]
    pub fn get_all_typed(&self, f: &OpticFinder) -> Vec<Typed> {
        let o = self.find(f, f.ty(), false);
        o.all(&self.value).into_iter().map(|v| Typed::new(f.ty().clone(), self.ops, v)).collect()
    }

    /// `typed.getAll(optic)` for an optic already resolved against this type.
    pub fn get_all(&self, optic: &TypedOptic) -> Vec<Value> {
        optic.all(&self.value)
    }

    // ---- updating -----------------------------------------------------------------------------------------------

    fn update_cap(self, field: &TypedOptic, mut updater: impl FnMut(Value) -> Value) -> Typed {
        let b = field.update(self.value, &mut updater);
        Typed::new(field.t_type().clone(), self.ops, b)
    }

    /// `typed.set(optic, newValue)` (keeps the focused type).
    #[track_caller]
    pub fn set(self, f: &OpticFinder, new_value: impl Into<Value>) -> Typed {
        let nv = Typed::new(f.ty().clone(), self.ops, new_value.into());
        self.set_typed(f, nv)
    }

    /// `typed.set(optic, newType, newValue)`.
    #[track_caller]
    pub fn set_with_type(self, f: &OpticFinder, new_type: &Type, new_value: impl Into<Value>) -> Typed {
        let nv = Typed::new(new_type.clone(), self.ops, new_value.into());
        self.set_typed(f, nv)
    }

    /// `typed.set(optic, Typed)`.
    #[track_caller]
    pub fn set_typed(self, f: &OpticFinder, new_value: Typed) -> Typed {
        let field = self.find(f, &new_value.ty, false);
        let nv = new_value.value;
        self.update_cap(&field, move |_| nv.clone())
    }

    /// `typed.updateTyped(optic, updater)`.
    #[track_caller]
    pub fn update_typed(self, f: &OpticFinder, updater: impl FnMut(Typed) -> Typed) -> Typed {
        let t = f.ty().clone();
        self.update_typed_as(f, &t, updater)
    }

    /// `typed.updateTyped(optic, newType, updater)`.
    #[track_caller]
    pub fn update_typed_as(self, f: &OpticFinder, new_type: &Type, updater: impl FnMut(Typed) -> Typed) -> Typed {
        let field = self.find(f, new_type, false);
        self.update_typed_with(f, &field, updater)
    }

    fn update_typed_with(self, f: &OpticFinder, field: &TypedOptic, mut updater: impl FnMut(Typed) -> Typed) -> Typed {
        let (ops, ft) = (self.ops, f.ty().clone());
        let b_type = field.b_type().clone();
        self.update_cap(field, move |v| {
            let new_value = updater(Typed::new(ft.clone(), ops, v));
            b_type.if_same(&new_value).unwrap_or_else(|| panic!("Function didn't update to the expected type"))
        })
    }

    /// `typed.update(optic, updater)`.
    #[track_caller]
    pub fn update(self, f: &OpticFinder, updater: impl FnMut(Value) -> Value) -> Typed {
        let field = self.find(f, f.ty(), false);
        self.update_cap(&field, updater)
    }

    /// `typed.update(optic, newType, updater)`.
    #[track_caller]
    pub fn update_as(self, f: &OpticFinder, new_type: &Type, updater: impl FnMut(Value) -> Value) -> Typed {
        let field = self.find(f, new_type, false);
        self.update_cap(&field, updater)
    }

    /// `typed.updateRecursiveTyped(optic, updater)`.
    #[track_caller]
    pub fn update_recursive_typed(self, f: &OpticFinder, updater: impl FnMut(Typed) -> Typed) -> Typed {
        let field = self.find(f, f.ty(), true);
        self.update_typed_with(f, &field, updater)
    }

    /// `typed.updateRecursive(optic, updater)`.
    #[track_caller]
    pub fn update_recursive(self, f: &OpticFinder, updater: impl FnMut(Value) -> Value) -> Typed {
        let field = self.find(f, f.ty(), true);
        self.update_cap(&field, updater)
    }

    // ---- remainder sugar (`DSL.remainderFinder()`) --------------------------------------------------------------

    /// `typed.getOrCreate(DSL.remainderFinder())`.
    #[track_caller]
    pub fn get_remainder(&self) -> Dynamic {
        self.get_or_create(&remainder_finder()).into_dynamic()
    }

    /// `typed.update(DSL.remainderFinder(), f)`.
    #[track_caller]
    pub fn update_remainder(self, mut f: impl FnMut(Dynamic) -> Dynamic) -> Typed {
        self.update(&remainder_finder(), move |v| Value::Dynamic(f(v.into_dynamic())))
    }

    /// `typed.set(DSL.remainderFinder(), dynamic)`.
    #[track_caller]
    pub fn set_remainder(self, d: Dynamic) -> Typed {
        self.set(&remainder_finder(), Value::Dynamic(d))
    }

    // ---- structure ----------------------------------------------------------------------------------------------

    /// `typed.out()`: unfold a recursion point.
    #[track_caller]
    pub fn out(&self) -> Typed {
        assert!(self.ty.is_recursive_point(), "Not recursive");
        Typed::new(self.ty.unfold(), self.ops, self.value.clone())
    }

    /// `typed.inj1(type)`.
    pub fn inj1(self, ty: &Type) -> Typed {
        Typed::new(Type::sum(&self.ty, ty), self.ops, Value::left(self.value))
    }

    /// `typed.inj2(type)`.
    pub fn inj2(self, ty: &Type) -> Typed {
        Typed::new(Type::sum(ty, &self.ty), self.ops, Value::right(self.value))
    }

    /// `Typed.pair(first, second)`.
    pub fn pair(first: Typed, second: Typed) -> Typed {
        Typed::new(Type::product(&first.ty, &second.ty), first.ops, Value::pair(first.value, second.value))
    }

    /// `typed.write()`.
    pub fn write(&self) -> DataResult<Dynamic> {
        self.ty.write_dynamic(&self.ops, &self.value)
    }
}
