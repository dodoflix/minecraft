//! Block state properties. Every `BlockStateProperties` constant is a typed const here with its vanilla name
//! (`props::FACING`, `props::WATERLOGGED`, `props::AGE_3`, ...). Note that several constants share a serialized name but
//! differ in their value set (`FACING` / `HORIZONTAL_FACING` / `FACING_HOPPER` are all `facing`); like vanilla,
//! identity is the constant, not the name.

use mc_core::direction::{Axis, Direction};
use std::marker::PhantomData;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PropertyKind {
    Boolean,
    Integer,
    Enum,
}

/// Untyped property description (`Property<T>`): serialized name and the serialized names of all values in vanilla order.
#[derive(Debug)]
pub struct PropertyDef {
    pub name: &'static str,
    pub kind: PropertyKind,
    /// Serialized value names in `getPossibleValues()` order (booleans: `true`, `false`).
    pub values: &'static [&'static str],
    /// First value of an integer property.
    pub min: i32,
    /// Binary name of the Java enum for enum properties.
    pub enum_class: Option<&'static str>,
}

impl PropertyDef {
    pub fn value_index(&self, serialized: &str) -> Option<usize> {
        self.values.iter().position(|v| *v == serialized)
    }
}

/// A Java enum usable as a property value, identified by its ordinal.
pub trait EnumValue: Copy + 'static {
    fn ordinal(self) -> usize;
    fn from_ordinal(i: usize) -> Self;
}

impl EnumValue for Direction {
    fn ordinal(self) -> usize {
        Direction::ordinal(self)
    }
    fn from_ordinal(i: usize) -> Self {
        Direction::ALL[i]
    }
}

impl EnumValue for Axis {
    fn ordinal(self) -> usize {
        Axis::ordinal(self)
    }
    fn from_ordinal(i: usize) -> Self {
        Axis::ALL[i]
    }
}

/// A typed property constant.
pub trait Property: Copy {
    type Value: Copy + PartialEq + std::fmt::Debug;
    /// Index into [`PROPERTY_DEFS`].
    fn index(self) -> usize;
    fn def(self) -> &'static PropertyDef {
        &PROPERTY_DEFS[self.index()]
    }
    /// Position of `value` in the property's value list. Panics if the property cannot hold it.
    fn value_index(self, value: Self::Value) -> usize;
    fn value(self, index: usize) -> Self::Value;
}

#[derive(Clone, Copy, Debug)]
pub struct BooleanProperty(usize);

impl BooleanProperty {
    pub(crate) const fn new(index: usize) -> Self {
        BooleanProperty(index)
    }
}

impl Property for BooleanProperty {
    type Value = bool;
    fn index(self) -> usize {
        self.0
    }
    fn value_index(self, value: bool) -> usize {
        usize::from(!value) // values are ["true", "false"]
    }
    fn value(self, index: usize) -> bool {
        index == 0
    }
}

#[derive(Clone, Copy, Debug)]
pub struct IntegerProperty(usize, i32);

impl IntegerProperty {
    pub(crate) const fn new(index: usize, min: i32) -> Self {
        IntegerProperty(index, min)
    }
}

impl Property for IntegerProperty {
    type Value = i32;
    fn index(self) -> usize {
        self.0
    }
    fn value_index(self, value: i32) -> usize {
        let i = value.wrapping_sub(self.1);
        assert!(
            i >= 0 && (i as usize) < self.def().values.len(),
            "{} cannot hold {value}",
            self.def().name
        );
        i as usize
    }
    fn value(self, index: usize) -> i32 {
        self.1 + index as i32
    }
}

pub struct EnumProperty<T: EnumValue> {
    index: usize,
    values: &'static [T],
    _p: PhantomData<T>,
}

impl<T: EnumValue> Clone for EnumProperty<T> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<T: EnumValue> Copy for EnumProperty<T> {}
impl<T: EnumValue> std::fmt::Debug for EnumProperty<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "EnumProperty({})", PROPERTY_DEFS[self.index].name)
    }
}

impl<T: EnumValue> EnumProperty<T> {
    pub(crate) const fn new(index: usize, values: &'static [T]) -> Self {
        EnumProperty {
            index,
            values,
            _p: PhantomData,
        }
    }

    /// The possible values in vanilla order (`getPossibleValues()`).
    pub const fn values(self) -> &'static [T] {
        self.values
    }
}

impl<T: EnumValue + PartialEq + std::fmt::Debug> Property for EnumProperty<T> {
    type Value = T;
    fn index(self) -> usize {
        self.index
    }
    fn value_index(self, value: T) -> usize {
        self.values
            .iter()
            .position(|v| v.ordinal() == value.ordinal())
            .unwrap_or_else(|| panic!("{} cannot hold {value:?}", self.def().name))
    }
    fn value(self, index: usize) -> T {
        self.values[index]
    }
}

include!(concat!(env!("OUT_DIR"), "/props.rs"));
