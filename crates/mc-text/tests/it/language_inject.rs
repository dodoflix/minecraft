//! `Language::inject` swaps the translations used by `getString()` (process-wide state, hence the `it` target).

use mc_text::{Component, Language};

#[test]
fn injected_language_is_used() {
    assert_eq!(
        Component::translatable("gui.done", vec![]).get_string(),
        "Done"
    );
    Language::inject(Language::from_entries([
        ("gui.done".to_owned(), "Fertig %d".to_owned()),
        ("x.y".to_owned(), "%2$s-%1$s".to_owned()),
    ]));
    // `%d` is rewritten to `%s` when loading, like Language.loadFromJson
    assert_eq!(
        Component::translatable("gui.done", vec![7.into()]).get_string(),
        "Fertig 7"
    );
    assert_eq!(
        Component::translatable("x.y", vec!["a".into(), "b".into()]).get_string(),
        "b-a"
    );
    // a missing key falls back to the key (or the fallback text)
    assert_eq!(Component::translatable("nope", vec![]).get_string(), "nope");
    assert_eq!(
        Component::translatable_with_fallback("nope", Some("fb".into()), vec![]).get_string(),
        "fb"
    );
}
