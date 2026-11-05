//! `OpticFinder` (`FieldFinder`, `NamedChoiceFinder`, `inField`): names a focus inside a type; resolved against a
//! concrete container type into a [`TypedOptic`] by [`OpticFinder::find_type`] (Java `findType(type, resultType, recurse)`).

use crate::optic::TypedOptic;
use crate::ty::{FindError, Matcher, Type};
use std::sync::{Arc, LazyLock};

enum FinderKind {
    /// `FieldFinder(name, type)`; `name == null` is `DSL.typeFinder(type)`.
    Field { name: Option<String>, ty: Type },
    /// `NamedChoiceFinder(name, type)`.
    NamedChoice { name: String, ty: Type },
    /// `outer.inField(name, type)`.
    InField { outer: OpticFinder, name: Option<String>, ty: Type },
}

/// `OpticFinder<FT>`; `ty()` is the focused type `FT`.
#[derive(Clone)]
pub struct OpticFinder(Arc<FinderKind>);

impl OpticFinder {
    /// `DSL.fieldFinder(name, type)` / `DSL.typeFinder(type)` (`name = None`).
    pub fn field(name: Option<&str>, ty: &Type) -> OpticFinder {
        OpticFinder(Arc::new(FinderKind::Field { name: name.map(str::to_string), ty: ty.clone() }))
    }

    /// `DSL.namedChoice(name, type)`.
    pub fn named_choice(name: &str, ty: &Type) -> OpticFinder {
        OpticFinder(Arc::new(FinderKind::NamedChoice { name: name.to_string(), ty: ty.clone() }))
    }

    /// `finder.type()`.
    pub fn ty(&self) -> &Type {
        match &*self.0 {
            FinderKind::Field { ty, .. } | FinderKind::NamedChoice { ty, .. } => ty,
            FinderKind::InField { outer, .. } => outer.ty(),
        }
    }

    /// `finder.inField(name, type)`: this finder, searched inside field `name` of type `ty`.
    pub fn in_field(&self, name: Option<&str>, ty: &Type) -> OpticFinder {
        OpticFinder(Arc::new(FinderKind::InField { outer: self.clone(), name: name.map(str::to_string), ty: ty.clone() }))
    }

    /// `finder.findType(containerType, resultType, recurse)`.
    pub fn find_type(&self, container: &Type, result: &Type, recurse: bool) -> Result<TypedOptic, FindError> {
        match &*self.0 {
            FinderKind::Field { name, ty } => {
                container.find_type(&Matcher::Field { name: name.clone(), ty: ty.clone(), result: result.clone() }, recurse)
            }
            FinderKind::NamedChoice { name, ty } => {
                container.find_type(&Matcher::NamedChoice { name: name.clone(), ty: ty.clone(), result: result.clone() }, recurse)
            }
            FinderKind::InField { outer, name, ty } => {
                let l1 = outer.find_type(ty, result, recurse)?;
                let first = OpticFinder::field(name.as_deref(), ty).find_type(container, l1.t_type(), recurse)?;
                Ok(first.compose(&l1))
            }
        }
    }

    /// `finder.findType(containerType, recurse)` (result type = the focused type).
    pub fn find_type_same(&self, container: &Type, recurse: bool) -> Result<TypedOptic, FindError> {
        self.find_type(container, self.ty(), recurse)
    }
}

/// `DSL.remainderFinder()`.
pub fn remainder_finder() -> OpticFinder {
    static REMAINDER_FINDER: LazyLock<OpticFinder> = LazyLock::new(|| Type::remainder().finder());
    REMAINDER_FINDER.clone()
}
