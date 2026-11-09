//! Hand-built component trees: the programmatic API must serialise to what vanilla emits.

use mc_text::util::{copy_on_click_text, format_list};
use mc_text::{
    ChatFormatting, ClickEvent, Component, HoverEvent, Style, TextColor, TranslatableArg,
};
use serde_json::json;

#[test]
fn builder_api_serialises_like_vanilla() {
    let c = Component::literal("a")
        .with_format(ChatFormatting::Red)
        .append(Component::literal("b").with_format(ChatFormatting::Bold))
        .append_str("");
    assert_eq!(
        c.to_json().unwrap(),
        json!({"text": "a", "extra": [{"text": "b", "bold": true}], "color": "red"})
    );
    assert_eq!(c.get_string(), "ab");

    // plain unstyled literals collapse to a bare string
    assert_eq!(Component::literal("x").to_json().unwrap(), json!("x"));
    assert_eq!(Component::empty().to_json().unwrap(), json!(""));

    let t = Component::translatable(
        "commands.give.success.single",
        vec![5.into(), Component::literal("Item").into(), "Steve".into()],
    );
    assert_eq!(t.get_string(), "Gave 5 Item to Steve");
    assert_eq!(
        t.to_json().unwrap(),
        json!({"translate": "commands.give.success.single", "with": [5, "Item", "Steve"]})
    );
    assert_eq!(
        t.to_nbt().unwrap(),
        Component::from_nbt(&t.to_nbt().unwrap())
            .unwrap()
            .to_nbt()
            .unwrap()
    );
}

#[test]
fn copy_on_click_text_matches_vanilla_shape() {
    let c = copy_on_click_text("1 2 3");
    assert_eq!(
        c.to_json().unwrap(),
        json!({
            "translate": "chat.square_brackets",
            "with": [{
                "text": "1 2 3",
                "color": "green",
                "click_event": {"value": "1 2 3", "action": "copy_to_clipboard"},
                "hover_event": {"value": {"translate": "chat.copy.click"}, "action": "show_text"},
                "insertion": "1 2 3"
            }]
        })
    );
    assert_eq!(c.get_string(), "[1 2 3]");
}

#[test]
fn style_semantics() {
    let red = Style::EMPTY.with_color_formatting(Some(ChatFormatting::Red));
    assert_eq!(red.color(), Some(TextColor::RED));
    // equality of colours ignores the name: #FF5555 == red, but they serialise differently
    assert_eq!(TextColor::from_rgb(0xFF5555), TextColor::RED);
    assert_eq!(
        Style::EMPTY.with_color_rgb(0xFF5555).to_json().unwrap(),
        json!({"color": "#FF5555"})
    );
    assert_eq!(red.to_json().unwrap(), json!({"color": "red"}));
    // `self` wins in apply_to
    let blue_bold = Style::EMPTY
        .with_color_formatting(Some(ChatFormatting::Blue))
        .with_bold(Some(true));
    let merged = red.apply_to(&blue_bold);
    assert_eq!(merged.color(), Some(TextColor::RED));
    assert!(merged.is_bold());
    assert!(!Style::EMPTY.with_bold(Some(false)).is_empty());
    assert!(
        Style::EMPTY
            .with_bold(Some(true))
            .with_bold(None)
            .is_empty()
    );
}

#[test]
fn events_roundtrip_in_nbt() {
    let c = Component::literal("x").with_style_fn(|s| {
        s.with_click_event(Some(ClickEvent::RunCommand("/say hi".into())))
            .with_hover_event(Some(HoverEvent::show_text(Component::literal("tip"))))
    });
    let back = Component::from_nbt(&c.to_nbt().unwrap()).unwrap();
    assert_eq!(back, c);
    // open_file can never be serialised
    let bad = Component::literal("x")
        .with_style_fn(|s| s.with_click_event(Some(ClickEvent::OpenFile("/tmp/x".into()))));
    assert!(bad.to_json().is_err() && bad.to_nbt().is_err());
}

#[test]
fn format_list_and_flat_list() {
    let sep = Component::literal(", ");
    let l = format_list(["a", "b", "c"], &sep, Component::literal);
    assert_eq!(l.get_string(), "a, b, c");
    let flat = Component::literal("a")
        .append(Component::literal("b").with_format(ChatFormatting::Bold))
        .to_flat_list();
    assert_eq!(flat.len(), 2);
    assert!(flat[1].style().is_bold());
    let _ = TranslatableArg::from(1.5f32);
}

#[test]
fn component_is_reasonably_small() {
    assert!(
        std::mem::size_of::<Component>() <= 320,
        "Component is {} bytes",
        std::mem::size_of::<Component>()
    );
}

/// `resultOrPartial`: an invalid optional field is dropped, nested through lists, `extra` and hover events; a failure
/// without any partial value (a required field, a non-component) stays an error.
#[test]
fn malformed_fields_decode_to_partial_components() {
    let partial = |v: serde_json::Value| Component::from_json_partial(&v).expect_err("malformed");

    let p = partial(json!({"text": "x", "color": "nope", "bold": true}));
    assert_eq!(
        p.value.unwrap().to_json().unwrap(),
        json!({"text": "x", "bold": true})
    );

    let p = partial(json!([{"text": "a", "color": "nope"}, "b", 5]));
    assert_eq!(
        p.value.unwrap().to_json().unwrap(),
        json!({"text": "a", "extra": ["b"]})
    );

    let p = partial(json!({"text": "a", "extra": [{"text": "b", "italic": 1}]}));
    assert_eq!(
        p.value.unwrap().to_json().unwrap(),
        json!({"text": "a", "extra": ["b"]})
    );

    let p = partial(
        json!({"text": "a", "hover_event": {"action": "show_text", "value": {"text": "h", "color": "nope"}}}),
    );
    assert_eq!(
        p.value.unwrap().to_json().unwrap(),
        json!({"text": "a", "hover_event": {"action": "show_text", "value": "h"}})
    );

    assert!(partial(json!({"color": "nope"})).value.is_none());
    assert!(partial(json!(5)).value.is_none());
    assert!(partial(json!({"text": "a", "extra": []})).value.is_some());
}
