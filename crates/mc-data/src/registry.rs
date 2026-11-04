//! Name ↔ protocol-id tables for every `BuiltInRegistries` registry, and the `registry_id!` macro behind the typed ids.

/// One built-in registry: entry names in protocol-id order.
pub struct Registry {
    name: &'static str,
    default: Option<&'static str>,
    entries: &'static [&'static str],
    by_name: &'static [(&'static str, u16)],
}

impl Registry {
    pub(crate) const fn new(
        name: &'static str,
        default: Option<&'static str>,
        entries: &'static [&'static str],
        by_name: &'static [(&'static str, u16)],
    ) -> Self {
        Registry { name, default, entries, by_name }
    }

    /// Registry key, e.g. `minecraft:block`.
    pub const fn name(&self) -> &'static str {
        self.name
    }

    /// Default entry name of a `DefaultedRegistry` (e.g. `minecraft:air` for blocks).
    pub const fn default_name(&self) -> Option<&'static str> {
        self.default
    }

    pub const fn len(&self) -> usize {
        self.entries.len()
    }

    pub const fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Entry names ordered by protocol id.
    pub const fn entries(&self) -> &'static [&'static str] {
        self.entries
    }

    /// Protocol id of an entry; a bare name (no namespace) is looked up in `minecraft`.
    pub fn id_of(&self, name: &str) -> Option<u32> {
        let find = |n: &str| self.by_name.binary_search_by(|(e, _)| (*e).cmp(n)).ok().map(|i| self.by_name[i].1 as u32);
        if name.contains(':') { find(name) } else { find(&format!("minecraft:{name}")) }
    }

    pub fn name_of(&self, id: u32) -> Option<&'static str> {
        self.entries.get(id as usize).copied()
    }

    /// Directory name of this registry's tags in a data pack (`block`, `worldgen/biome`, ...).
    pub fn tag_dir(&self) -> &'static str {
        self.name.strip_prefix("minecraft:").unwrap_or(self.name)
    }
}

/// Anything that is a typed id into one built-in registry (`Block`, `Item`, `EntityType`, ...).
pub trait RegistryId: Copy + Ord + std::hash::Hash + Send + Sync + 'static {
    fn registry() -> &'static Registry;
    fn from_id(id: u32) -> Option<Self>;
    fn id(self) -> u32;
}

macro_rules! registry_id {
    ($ty:ident, $reg:ident) => {
        impl $ty {
            /// The registry this id indexes.
            pub fn registry() -> &'static $crate::registry::Registry {
                &$crate::registry::registries::$reg
            }

            /// Protocol (raw registry) id.
            pub const fn id(self) -> u32 {
                self.0 as u32
            }

            pub fn from_id(id: u32) -> Option<Self> {
                if (id as usize) < $crate::registry::registries::$reg.len() { Some($ty(id as u16)) } else { None }
            }

            /// Registry key, e.g. `minecraft:stone`.
            pub fn name(self) -> &'static str {
                $crate::registry::registries::$reg.entries()[self.0 as usize]
            }

            pub fn identifier(self) -> mc_core::ident::Identifier {
                mc_core::ident::Identifier::parse(self.name()).expect("registry names are valid identifiers")
            }

            /// Lookup by full name (`minecraft:stone`) or bare name (`stone`).
            pub fn by_name(name: &str) -> Option<Self> {
                $crate::registry::registries::$reg.id_of(name).map(|i| $ty(i as u16))
            }

            /// All entries in protocol-id order.
            pub fn iter() -> impl ExactSizeIterator<Item = Self> + Clone {
                (0..$crate::registry::registries::$reg.len() as u16).map($ty)
            }
        }
        impl $crate::registry::RegistryId for $ty {
            fn registry() -> &'static $crate::registry::Registry {
                &$crate::registry::registries::$reg
            }
            fn from_id(id: u32) -> Option<Self> {
                $ty::from_id(id)
            }
            fn id(self) -> u32 {
                self.0 as u32
            }
        }
        impl std::fmt::Debug for $ty {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(f, "{}({})", stringify!($ty), self.name())
            }
        }
        impl std::fmt::Display for $ty {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str(self.name())
            }
        }
    };
}

/// One `static` per registry (`registries::BLOCK`, `registries::WORLDGEN_BIOME_SOURCE`, ...) and [`registries::ALL`].
pub mod registries {
    use super::Registry;
    include!(concat!(env!("OUT_DIR"), "/registries.rs"));

    /// Registry by key (`minecraft:block`), `ALL` is sorted by key.
    pub fn by_name(name: &str) -> Option<&'static Registry> {
        ALL.binary_search_by(|r| r.name().cmp(name)).ok().map(|i| ALL[i])
    }
}

pub use registries::by_name;
