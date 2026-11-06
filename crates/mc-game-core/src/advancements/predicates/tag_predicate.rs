//! `net.minecraft.advancements.predicates.TagPredicate` of a data-driven registry (`damage_type`, ...).

use crate::server::level::ServerLevel;
use mc_core::ident::Identifier;
use mc_dfu::{BOOL, Codec};
use mc_protocol::components::{
    Holder, HolderSet, NoDirect, data_driven_set_codec, holder_set_contains,
};

/// `TagPredicate<T>`: whether the holder is in `tag` must equal `expected`. `registry` is the registry's data pack
/// directory (`damage_type`).
#[derive(Clone, Debug, PartialEq)]
pub struct TagPredicate {
    pub registry: &'static str,
    pub tag: HolderSet<NoDirect>,
    pub expected: bool,
}

#[derive(Clone)]
struct Raw {
    tag: HolderSet<NoDirect>,
    expected: bool,
}

impl TagPredicate {
    /// `TagPredicate.codec(registryKey)`.
    pub fn codec(registry: &'static str) -> Codec<TagPredicate> {
        mc_dfu::record!(Raw {
            tag: data_driven_set_codec(registry).field_of("id"),
            expected: BOOL.field_of("expected")
        })
        .codec()
        .xmap(
            move |r| TagPredicate {
                registry,
                tag: r.tag,
                expected: r.expected,
            },
            |p: &TagPredicate| Raw {
                tag: p.tag.clone(),
                expected: p.expected,
            },
        )
    }

    /// `is(tag)`.
    pub fn is(registry: &'static str, tag: HolderSet<NoDirect>) -> TagPredicate {
        TagPredicate {
            registry,
            tag,
            expected: true,
        }
    }

    /// `isNot(tag)`.
    pub fn is_not(registry: &'static str, tag: HolderSet<NoDirect>) -> TagPredicate {
        TagPredicate {
            registry,
            tag,
            expected: false,
        }
    }

    /// `matches(holder)` for the registry entry named `id`; tags resolve against the level's registries (data pack
    /// tags), or the vanilla data pack for a level without registries.
    pub fn matches(&self, level: &ServerLevel, id: &Identifier) -> bool {
        let contains = match &self.tag {
            HolderSet::Named(tag) if level.registries.registry(self.registry).is_some() => {
                level.registries.is_in_tag(self.registry, id, tag)
            }
            set => holder_set_contains(
                set,
                self.registry,
                &Holder::<NoDirect>::Reference(id.clone()),
            ),
        };
        contains == self.expected
    }
}
