//! `TextFilter`, `Filterable`: the chat/sign/book text filtering model. A player's filter is the dummy one (every
//! text passes) unless `text-filtering-config` names a moderation service (`ServerTextFilter`).

pub use crate::server::chat::FilteredText;
use crate::util::completion::Completion;

/// `TextFilter`. Like vanilla's futures, a result is complete at once (the dummy filter) or is completed by a worker
/// thread later.
pub trait TextFilter: Send + Sync {
    fn join(&self) {}
    fn leave(&self) {}
    fn process_stream_message(&self, message: &str) -> Completion<FilteredText>;
    fn process_message_bundle(&self, messages: &[String]) -> Completion<Vec<FilteredText>>;
}

/// `TextFilter.DUMMY`.
pub struct DummyTextFilter;

impl TextFilter for DummyTextFilter {
    fn process_stream_message(&self, message: &str) -> Completion<FilteredText> {
        Completion::completed(FilteredText::pass_through(message))
    }

    fn process_message_bundle(&self, messages: &[String]) -> Completion<Vec<FilteredText>> {
        Completion::completed(
            messages
                .iter()
                .map(|m| FilteredText::pass_through(m.as_str()))
                .collect(),
        )
    }
}

/// `Filterable<T>`: a value with the raw form and, when the filter changed it, the filtered form.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Filterable<T> {
    pub raw: T,
    pub filtered: Option<T>,
}

impl<T> Filterable<T> {
    pub fn pass_through(value: T) -> Self {
        Self {
            raw: value,
            filtered: None,
        }
    }

    /// `get(filterEnabled)`.
    pub fn get(&self, filter_enabled: bool) -> &T {
        if filter_enabled {
            self.filtered.as_ref().unwrap_or(&self.raw)
        } else {
            &self.raw
        }
    }

    pub fn map<U>(self, f: impl Fn(T) -> U) -> Filterable<U> {
        Filterable {
            raw: f(self.raw),
            filtered: self.filtered.map(f),
        }
    }

    /// `resolve`: `None` unless the function accepts the raw value (and the filtered one, if present).
    pub fn resolve<U>(&self, f: impl Fn(&T) -> Option<U>) -> Option<Filterable<U>> {
        let raw = f(&self.raw)?;
        match &self.filtered {
            Some(filtered) => Some(Filterable {
                raw,
                filtered: Some(f(filtered)?),
            }),
            None => Some(Filterable {
                raw,
                filtered: None,
            }),
        }
    }
}

impl Filterable<String> {
    /// `Filterable.from(FilteredText)`.
    pub fn from_text(text: &FilteredText) -> Self {
        Self {
            raw: text.raw.clone(),
            filtered: text
                .is_filtered()
                .then(|| text.filtered_or_empty().as_lossy_str().to_owned()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mc_protocol::codec::BitSet;
    use mc_protocol::packets::play::FilterMask;

    #[test]
    fn masks_apply_like_vanilla() {
        let mut bits = BitSet::new();
        bits.set(1);
        bits.set(2);
        bits.set(40); // beyond the text: ignored
        let text = FilteredText {
            raw: "abcd".into(),
            mask: FilterMask::PartiallyFiltered(bits),
        };
        assert_eq!(text.filtered().unwrap().as_lossy_str(), "a##d");
        assert!(text.is_filtered());
        assert_eq!(FilteredText::fully_filtered("x").filtered(), None);
        assert_eq!(
            FilteredText::fully_filtered("x")
                .filtered_or_empty()
                .as_lossy_str(),
            ""
        );
        assert!(!FilteredText::pass_through("x").is_filtered());
        assert_eq!(
            Filterable::from_text(&text),
            Filterable {
                raw: "abcd".into(),
                filtered: Some("a##d".into())
            }
        );
    }

    #[test]
    fn filterable_get_map_resolve() {
        let f = Filterable {
            raw: 1,
            filtered: Some(2),
        };
        assert_eq!((*f.get(true), *f.get(false)), (2, 1));
        assert_eq!(
            f.clone().map(|v| v * 10),
            Filterable {
                raw: 10,
                filtered: Some(20)
            }
        );
        assert_eq!(f.resolve(|v| (*v < 2).then_some(*v)), None);
        assert_eq!(
            Filterable::pass_through(1).resolve(|v| Some(*v)),
            Some(Filterable {
                raw: 1,
                filtered: None
            })
        );
    }

    #[test]
    fn dummy_filter_passes_everything() {
        let messages = vec!["a".to_owned(), String::new()];
        assert_eq!(
            DummyTextFilter.process_message_bundle(&messages).get(),
            Some(vec![FilteredText::pass_through("a"), FilteredText::empty()])
        );
    }
}
