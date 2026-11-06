//! `CollectionPredicate`, `CollectionContentsPredicate`, `CollectionCountsPredicate`.
//!
//! Vanilla specialises the contents/counts lists into `Zero`/`Single`/`Multiple` classes; they only differ in how
//! `test` is evaluated (all three are `List`-equivalent for the codecs), so one `Vec` is kept here.

use super::bounds::Ints;
use mc_dfu::Codec;

/// `CollectionCountsPredicate.Entry`: how often `test` must match.
#[derive(Clone, Debug, PartialEq)]
pub struct CountEntry<P> {
    pub test: P,
    pub count: Ints,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CollectionPredicate<P> {
    pub contains: Option<Vec<P>>,
    pub counts: Option<Vec<CountEntry<P>>>,
    pub size: Option<Ints>,
}

impl<P: Clone + PartialEq + Send + Sync + 'static> CollectionPredicate<P> {
    /// `CollectionPredicate.codec(elementCodec)`.
    pub fn codec(element: &Codec<P>) -> Codec<CollectionPredicate<P>> {
        let entry = mc_dfu::record!(CountEntry<P> { test: element.field_of("test"), count: Ints::codec().field_of("count") }).codec();
        mc_dfu::record!(CollectionPredicate<P> {
            contains: element.list_of().optional_field_of("contains"),
            counts: entry.list_of().optional_field_of("count"),
            size: Ints::codec().optional_field_of("size"),
        })
        .codec()
    }

    /// `test(Iterable)` with the element predicate evaluated by `matches`.
    pub fn test<T>(&self, values: &[T], matches: impl Fn(&P, &T) -> bool) -> bool {
        if let Some(contains) = &self.contains {
            let ok = match contains.len() {
                0 => true,
                1 => values.iter().any(|v| matches(&contains[0], v)),
                _ => {
                    let mut remaining: Vec<&P> = contains.iter().collect();
                    let mut ok = false;
                    for v in values {
                        remaining.retain(|p| !matches(p, v));
                        if remaining.is_empty() {
                            ok = true;
                            break;
                        }
                    }
                    ok
                }
            };
            if !ok {
                return false;
            }
        }
        if let Some(counts) = &self.counts {
            for e in counts {
                let n = values.iter().filter(|v| matches(&e.test, v)).count();
                if !e.count.matches(n as i32) {
                    return false;
                }
            }
        }
        self.size
            .as_ref()
            .is_none_or(|s| s.matches(values.len() as i32))
    }
}
