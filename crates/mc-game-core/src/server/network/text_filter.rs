//! `TextFilter`, `FilteredText`, `Filterable`: the chat/sign/book text filtering model. The dedicated server uses
//! [`DummyTextFilter`] unless `text-filtering-config` names a moderation endpoint (the HTTP-backed
//! `ServerTextFilter` implementations are not ported, see STATUS.md).

use mc_protocol::packets::play::FilterMask;

/// `FilterMask.apply`: `None` = fully filtered.
pub fn apply_filter_mask(mask: &FilterMask, text: &str) -> Option<String> {
    match mask {
        FilterMask::PassThrough => Some(text.to_owned()),
        FilterMask::FullyFiltered => None,
        FilterMask::PartiallyFiltered(bits) => {
            let mut units: Vec<u16> = text.encode_utf16().collect();
            for (i, unit) in units.iter_mut().enumerate().take(bits.length()) {
                if bits.get(i) {
                    *unit = u16::from(b'#');
                }
            }
            Some(String::from_utf16_lossy(&units))
        }
    }
}

/// `FilteredText`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FilteredText {
    pub raw: String,
    pub mask: FilterMask,
}

impl FilteredText {
    pub fn empty() -> Self {
        Self::pass_through("")
    }

    pub fn pass_through(message: impl Into<String>) -> Self {
        Self { raw: message.into(), mask: FilterMask::PassThrough }
    }

    pub fn fully_filtered(message: impl Into<String>) -> Self {
        Self { raw: message.into(), mask: FilterMask::FullyFiltered }
    }

    pub fn filtered(&self) -> Option<String> {
        apply_filter_mask(&self.mask, &self.raw)
    }

    pub fn filtered_or_empty(&self) -> String {
        self.filtered().unwrap_or_default()
    }

    /// `isFiltered`: the mask is not `PASS_THROUGH`.
    pub fn is_filtered(&self) -> bool {
        self.mask != FilterMask::PassThrough
    }
}

/// `TextFilter`. Vanilla returns `CompletableFuture`s; the implementations offered here complete immediately, a
/// remote filter returns its result from a worker.
pub trait TextFilter: Send + Sync {
    fn join(&self) {}
    fn leave(&self) {}
    fn process_stream_message(&self, message: &str) -> FilteredText;
    fn process_message_bundle(&self, messages: &[String]) -> Vec<FilteredText>;
}

/// `TextFilter.DUMMY`.
pub struct DummyTextFilter;

impl TextFilter for DummyTextFilter {
    fn process_stream_message(&self, message: &str) -> FilteredText {
        FilteredText::pass_through(message)
    }

    fn process_message_bundle(&self, messages: &[String]) -> Vec<FilteredText> {
        messages.iter().map(|m| FilteredText::pass_through(m.as_str())).collect()
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
        Self { raw: value, filtered: None }
    }

    /// `get(filterEnabled)`.
    pub fn get(&self, filter_enabled: bool) -> &T {
        if filter_enabled { self.filtered.as_ref().unwrap_or(&self.raw) } else { &self.raw }
    }

    pub fn map<U>(self, f: impl Fn(T) -> U) -> Filterable<U> {
        Filterable { raw: f(self.raw), filtered: self.filtered.map(f) }
    }

    /// `resolve`: `None` unless the function accepts the raw value (and the filtered one, if present).
    pub fn resolve<U>(&self, f: impl Fn(&T) -> Option<U>) -> Option<Filterable<U>> {
        let raw = f(&self.raw)?;
        match &self.filtered {
            Some(filtered) => Some(Filterable { raw, filtered: Some(f(filtered)?) }),
            None => Some(Filterable { raw, filtered: None }),
        }
    }
}

impl Filterable<String> {
    /// `Filterable.from(FilteredText)`.
    pub fn from_text(text: &FilteredText) -> Self {
        Self { raw: text.raw.clone(), filtered: text.is_filtered().then(|| text.filtered_or_empty()) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mc_protocol::codec::BitSet;

    #[test]
    fn masks_apply_like_vanilla() {
        let mut bits = BitSet::new();
        bits.set(1);
        bits.set(2);
        bits.set(40); // beyond the text: ignored
        let text = FilteredText { raw: "abcd".into(), mask: FilterMask::PartiallyFiltered(bits) };
        assert_eq!(text.filtered().as_deref(), Some("a##d"));
        assert!(text.is_filtered());
        assert_eq!(FilteredText::fully_filtered("x").filtered(), None);
        assert_eq!(FilteredText::fully_filtered("x").filtered_or_empty(), "");
        assert!(!FilteredText::pass_through("x").is_filtered());
        assert_eq!(Filterable::from_text(&text), Filterable { raw: "abcd".into(), filtered: Some("a##d".into()) });
    }

    #[test]
    fn filterable_get_map_resolve() {
        let f = Filterable { raw: 1, filtered: Some(2) };
        assert_eq!((*f.get(true), *f.get(false)), (2, 1));
        assert_eq!(f.clone().map(|v| v * 10), Filterable { raw: 10, filtered: Some(20) });
        assert_eq!(f.resolve(|v| (*v < 2).then_some(*v)), None);
        assert_eq!(Filterable::pass_through(1).resolve(|v| Some(*v)), Some(Filterable { raw: 1, filtered: None }));
    }

    #[test]
    fn dummy_filter_passes_everything() {
        let messages = vec!["a".to_owned(), String::new()];
        assert_eq!(DummyTextFilter.process_message_bundle(&messages), vec![FilteredText::pass_through("a"), FilteredText::empty()]);
    }
}
