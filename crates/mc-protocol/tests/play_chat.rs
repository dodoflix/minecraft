//! Play packet group `chat` against the real vanilla codecs (`tests/fixtures/play/GenChat.java` -> `chat.golden`).
mod common;
use common::*;

use std::fmt::Debug;

use mc_core::ident::Identifier;
use mc_protocol::auth::{PublicKey, public_key_from_der};
use mc_protocol::codec::{
    BitSet, DecodeError, EnumSet, FixedBitSet, GameProfile, Holder, Instant, MessageSignature,
    Property, from_bytes, read_utf, read_varint, to_bytes,
};
use mc_protocol::packets::play::*;
use mc_protocol::{Decode, Encode};
use mc_text::{ChatFormatting, Component, HoverEvent, Style, TranslatableArg};
use uuid::Uuid;

fn g() -> Golden {
    Golden::load("chat")
}

fn comps() -> Vec<Component> {
    vec![
        Component::empty(),
        Component::literal("hi"),
        Component::literal("h\u{e9}llo \u{1F600} w\u{f6}rld \u{0}\u{2028}"),
        Component::translatable(
            "chat.type.text",
            vec![
                TranslatableArg::String("a".into()),
                TranslatableArg::String("b".into()),
            ],
        ),
        Component::literal("red").with_formats(&[ChatFormatting::Red, ChatFormatting::Bold]),
        Component::literal("a")
            .append(
                Component::literal("b").with_style(
                    &Style::EMPTY
                        .with_color_rgb(0x336699)
                        .with_italic(Some(true)),
                ),
            )
            .append_str("c"),
        Component::literal("tip").with_style(
            &Style::EMPTY
                .with_hover_event(Some(HoverEvent::show_text(Component::literal("hover")))),
        ),
        Component::keybind("key.jump"),
    ]
}

fn sig(seed: i32) -> MessageSignature {
    MessageSignature(std::array::from_fn(|i| (seed * 31 + i as i32 * 7) as u8))
}

fn bits(ones: &[usize]) -> BitSet {
    let mut b = BitSet::new();
    ones.iter().for_each(|&i| b.set(i));
    b
}

fn ms(v: i64) -> Instant {
    Instant(v)
}

fn id(s: &str) -> Identifier {
    Identifier::parse(s).unwrap()
}

/// Vanilla's message for bytes it rejects: `<label>` holds the bytes, `<label>_err` the message (a writeUtf string).
fn vanilla_error(g: &Golden, label: &str) -> String {
    let mut m = g.bytes(&format!("{label}_err"));
    read_utf(&mut m, 1 << 20).unwrap()
}

fn expect_err<T: Decode + Debug>(g: &Golden, label: &str) {
    let mut bytes = g.bytes(label);
    let e = T::decode(&mut bytes).expect_err(label);
    assert_eq!(e.to_string(), vanilla_error(g, label), "{label}");
}

/// Same, for failures whose vanilla text is a Netty buffer message: only that it fails.
fn expect_err_any<T: Decode + Debug>(g: &Golden, label: &str) {
    let mut bytes = g.bytes(label);
    let e = T::decode(&mut bytes).expect_err(label);
    // vanilla: a Netty IndexOutOfBoundsException (which disconnects, like our `Eof`)
    assert!(
        vanilla_error(g, label).starts_with("readerIndex("),
        "{label}"
    );
    assert!(matches!(e, DecodeError::Eof { .. }), "{label}: {e:?}");
}

/// Bytes vanilla accepts and re-encodes differently (`<label>_re`).
fn expect_lossy<T: Encode + Decode + Debug>(g: &Golden, label: &str) -> T {
    let bytes = g.bytes(label);
    let v = from_bytes::<T>(bytes).unwrap_or_else(|e| panic!("{label}: {e}"));
    assert_eq!(
        hex(&to_bytes(&v).unwrap()),
        hex(g.bytes(&format!("{label}_re"))),
        "{label}"
    );
    v
}

fn body_of(packet_bytes: &[u8]) -> &[u8] {
    let mut p = packet_bytes;
    read_varint(&mut p).unwrap();
    p
}

// ---- nested types ---------------------------------------------------------------------------------------------

fn message_signature_packed(g: &Golden) {
    for id in [0, 1, 126, 127, 128, 1000, i32::MAX - 1] {
        g.check_value(
            &format!("sigpacked/cached_{id}"),
            &MessageSignaturePacked::Cached(id),
        );
    }
    g.check_value("sigpacked/full", &MessageSignaturePacked::Full(sig(1)));
    assert_eq!(
        expect_lossy::<MessageSignaturePacked>(g, "sigpacked/negative"),
        MessageSignaturePacked::Cached(-6)
    );
}

fn last_seen_and_update(g: &Golden) {
    g.check_value("lastseen/empty", &LastSeenMessagesPacked::default());
    g.check_value(
        "lastseen/mixed",
        &LastSeenMessagesPacked {
            entries: vec![
                MessageSignaturePacked::Cached(3),
                MessageSignaturePacked::Full(sig(1)),
                MessageSignaturePacked::Cached(127),
            ],
        },
    );
    g.check_value(
        "lastseen/twenty",
        &LastSeenMessagesPacked {
            entries: (0..20)
                .map(|i| MessageSignaturePacked::Cached(i * 6))
                .collect(),
        },
    );
    expect_err::<LastSeenMessagesPacked>(g, "lastseen/twentyone");

    for mask in [0u32, 1, 0x80, 0xfffff, 0x55555, 0x12345] {
        let ones: Vec<usize> = (0..20).filter(|i| mask >> i & 1 != 0).collect();
        for offset in [0, 5, 300, -1] {
            for checksum in [0i8, 1, -1, 127] {
                let u = LastSeenMessagesUpdate {
                    offset,
                    acknowledged: FixedBitSet(bits(&ones)),
                    checksum,
                };
                g.check_value(&format!("update/{mask}_{offset}_{checksum}"), &u);
            }
        }
    }
    // vanilla reads the 4 padding bits too (and throws when encoding such a set back)
    let u = from_bytes::<LastSeenMessagesUpdate>(g.bytes("update/padding")).unwrap();
    assert_eq!(u.acknowledged.0, bits(&[0, 19, 20, 21, 22, 23]));
}

fn filter_mask(g: &Golden) {
    g.check_value("filtermask/pass", &FilterMask::PassThrough);
    g.check_value("filtermask/full", &FilterMask::FullyFiltered);
    g.check_value(
        "filtermask/partial_empty",
        &FilterMask::PartiallyFiltered(BitSet::new()),
    );
    for ones in [
        &[0usize][..],
        &[1, 2, 3],
        &[7, 8],
        &[63, 64],
        &[100, 200, 255],
        &[1000],
    ] {
        g.check_value(
            &format!("filtermask/partial_{}_{}", ones[0], ones.len()),
            &FilterMask::PartiallyFiltered(bits(ones)),
        );
    }
    assert_eq!(
        expect_lossy::<FilterMask>(g, "filtermask/unknown_type"),
        FilterMask::PassThrough
    );
    assert_eq!(
        expect_lossy::<FilterMask>(g, "filtermask/negative_type"),
        FilterMask::PassThrough
    );
}

fn signed_message_body(g: &Golden) {
    g.check_value(
        "body/basic",
        &SignedMessageBodyPacked {
            content: "hello".into(),
            time_stamp: ms(1700000000123),
            salt: 42,
            last_seen: Default::default(),
        },
    );
    g.check_value(
        "body/extreme",
        &SignedMessageBodyPacked {
            content: "\u{1F600}".repeat(128),
            time_stamp: ms(-1),
            salt: i64::MIN,
            last_seen: LastSeenMessagesPacked {
                entries: vec![
                    MessageSignaturePacked::Full(sig(2)),
                    MessageSignaturePacked::Cached(0),
                ],
            },
        },
    );
    g.check_value(
        "body/max_len",
        &SignedMessageBodyPacked {
            content: "x".repeat(256),
            time_stamp: ms(0),
            salt: 0,
            last_seen: Default::default(),
        },
    );
    expect_err::<SignedMessageBodyPacked>(g, "body/too_long");
    expect_err::<SignedMessageBodyPacked>(g, "body/too_long_utf16");
}

fn decorations() -> Vec<(&'static str, ChatTypeDecoration)> {
    use ChatTypeDecorationParameter as P;
    vec![
        (
            "sender",
            ChatTypeDecoration {
                translation_key: "chat.type.text".into(),
                parameters: vec![P::Sender, P::Content],
                style: Style::EMPTY,
            },
        ),
        (
            "team",
            ChatTypeDecoration {
                translation_key: "x.y".into(),
                parameters: vec![P::Target, P::Sender, P::Content],
                style: Style::EMPTY
                    .with_color_formatting(Some(ChatFormatting::Gray))
                    .with_italic(Some(true)),
            },
        ),
        (
            "empty",
            ChatTypeDecoration {
                translation_key: "".into(),
                parameters: vec![],
                style: Style::EMPTY,
            },
        ),
        (
            "unicode",
            ChatTypeDecoration {
                translation_key: "k\u{e9}y.\u{1F600}".into(),
                parameters: vec![P::Content],
                style: Style::EMPTY.with_bold(Some(true)),
            },
        ),
    ]
}

fn chat_type_decoration(g: &Golden) {
    for (label, d) in decorations() {
        g.check_value(&format!("decoration/{label}"), &d);
    }
    let d = expect_lossy::<ChatTypeDecoration>(g, "decoration/unknown_param");
    assert_eq!(d.parameters, vec![ChatTypeDecorationParameter::Sender; 2]);
    let ds = decorations();
    g.check_value(
        "chattype/direct",
        &ChatType {
            chat: ds[0].1.clone(),
            narration: ds[1].1.clone(),
        },
    );
}

// ---- clientbound ------------------------------------------------------------------------------------------------

fn chat_type_ref(g: &Golden, i: usize) -> Holder<ChatType> {
    Holder::Reference(i32::from(g.bytes("chat_type_ids")[i]))
}

fn sender() -> Uuid {
    Uuid::parse_str("00112233-4455-6677-8899-aabbccddeeff").unwrap()
}

fn player_chat(g: &Golden) {
    let c = comps();
    let body = SignedMessageBodyPacked {
        content: "hello world".into(),
        time_stamp: ms(1700000000000),
        salt: 123456789,
        last_seen: LastSeenMessagesPacked {
            entries: vec![
                MessageSignaturePacked::Cached(0),
                MessageSignaturePacked::Full(sig(2)),
            ],
        },
    };
    let body0 = SignedMessageBodyPacked {
        content: "".into(),
        time_stamp: ms(0),
        salt: 0,
        last_seen: Default::default(),
    };
    let packet = |global_index,
                  index,
                  signature,
                  body: &SignedMessageBodyPacked,
                  unsigned_content,
                  filter_mask,
                  chat_type| ClientboundPlayerChatPacket {
        global_index,
        sender: sender(),
        index,
        signature,
        body: body.clone(),
        unsigned_content,
        filter_mask,
        chat_type,
    };
    for i in 0..7 {
        let bound = ChatTypeBound {
            chat_type: chat_type_ref(g, i),
            name: c[i % 8].clone(),
            target_name: (i % 2 != 0).then(|| c[(i + 3) % 8].clone()),
        };
        g.check(
            &format!("player_chat/type_{i}"),
            &packet(
                i as i32,
                i as i32 * 10,
                Some(sig(1)),
                &body,
                None,
                FilterMask::PassThrough,
                bound,
            ),
        );
    }
    let bound0 = ChatTypeBound {
        chat_type: chat_type_ref(g, 0),
        name: Component::literal("Steve"),
        target_name: None,
    };
    g.check(
        "player_chat/unsigned",
        &packet(
            0,
            0,
            None,
            &body0,
            None,
            FilterMask::PassThrough,
            bound0.clone(),
        ),
    );
    let green = Component::literal("unsigned \u{1F600}").with_format(ChatFormatting::Green);
    g.check(
        "player_chat/unsigned_content",
        &packet(
            1,
            1,
            None,
            &body0,
            Some(green),
            FilterMask::PassThrough,
            bound0.clone(),
        ),
    );
    g.check(
        "player_chat/signed_content",
        &packet(
            2,
            2,
            Some(sig(1)),
            &body,
            Some(c[3].clone()),
            FilterMask::PassThrough,
            bound0.clone(),
        ),
    );
    g.check(
        "player_chat/fully_filtered",
        &packet(
            3,
            3,
            Some(sig(2)),
            &body,
            None,
            FilterMask::FullyFiltered,
            bound0.clone(),
        ),
    );
    let part = FilterMask::PartiallyFiltered(bits(&[0, 5, 70]));
    g.check(
        "player_chat/partially_filtered",
        &packet(
            4,
            4,
            Some(sig(2)),
            &body,
            None,
            part.clone(),
            bound0.clone(),
        ),
    );
    g.check(
        "player_chat/partially_filtered_empty",
        &packet(
            4,
            4,
            Some(sig(2)),
            &body,
            None,
            FilterMask::PartiallyFiltered(BitSet::new()),
            bound0.clone(),
        ),
    );
    let extreme = ClientboundPlayerChatPacket {
        sender: Uuid::from_u64_pair(1 << 63, i64::MAX as u64),
        ..packet(
            i32::MAX,
            -1,
            Some(sig(1)),
            &body,
            None,
            FilterMask::PassThrough,
            bound0.clone(),
        )
    };
    g.check("player_chat/extreme_indices", &extreme);
    g.check(
        "player_chat/negative_global",
        &packet(
            i32::MIN,
            i32::MIN,
            None,
            &body0,
            None,
            FilterMask::PassThrough,
            bound0.clone(),
        ),
    );
    let ds = decorations();
    let direct = ChatTypeBound {
        chat_type: Holder::Direct(ChatType {
            chat: ds[0].1.clone(),
            narration: ds[1].1.clone(),
        }),
        name: c[2].clone(),
        target_name: Some(c[4].clone()),
    };
    g.check(
        "player_chat/direct_chat_type",
        &packet(5, 5, Some(sig(1)), &body, Some(c[5].clone()), part, direct),
    );
    let unicode_body = SignedMessageBodyPacked {
        content: "\u{1F600} caf\u{e9} \u{4e2d}\u{6587}".into(),
        time_stamp: ms(1),
        salt: -7,
        last_seen: Default::default(),
    };
    let emote = ChatTypeBound {
        chat_type: chat_type_ref(g, 6),
        name: c[2].clone(),
        target_name: Some(c[2].clone()),
    };
    g.check(
        "player_chat/unicode",
        &packet(
            6,
            6,
            Some(sig(1)),
            &unicode_body,
            Some(c[2].clone()),
            FilterMask::PassThrough,
            emote,
        ),
    );
}

fn disguised_and_delete_chat(g: &Golden) {
    let c = comps();
    for i in 0..8 {
        let bound = ChatTypeBound {
            chat_type: chat_type_ref(g, i % 7),
            name: c[(i + 1) % 8].clone(),
            target_name: (i % 2 != 0).then(|| c[i].clone()),
        };
        g.check(
            &format!("disguised_chat/{i}"),
            &ClientboundDisguisedChatPacket {
                message: c[i].clone(),
                chat_type: bound,
            },
        );
    }
    let ds = decorations();
    let direct = ChatTypeBound {
        chat_type: Holder::Direct(ChatType {
            chat: ds[0].1.clone(),
            narration: ds[1].1.clone(),
        }),
        name: c[2].clone(),
        target_name: Some(c[4].clone()),
    };
    g.check(
        "disguised_chat/direct",
        &ClientboundDisguisedChatPacket {
            message: c[3].clone(),
            chat_type: direct,
        },
    );

    for id in [0, 1, 127, 128, 16383] {
        g.check(
            &format!("delete_chat/cached_{id}"),
            &ClientboundDeleteChatPacket {
                message_signature: MessageSignaturePacked::Cached(id),
            },
        );
    }
    g.check(
        "delete_chat/full",
        &ClientboundDeleteChatPacket {
            message_signature: MessageSignaturePacked::Full(sig(1)),
        },
    );
}

fn system_chat_titles_and_tab_list(g: &Golden) {
    let c = comps();
    for (i, comp) in c.iter().enumerate() {
        g.check(
            &format!("system_chat/{i}_chat"),
            &ClientboundSystemChatPacket {
                content: comp.clone(),
                overlay: false,
            },
        );
        g.check(
            &format!("system_chat/{i}_overlay"),
            &ClientboundSystemChatPacket {
                content: comp.clone(),
                overlay: true,
            },
        );
        g.check(
            &format!("set_title_text/{i}"),
            &ClientboundSetTitleTextPacket { text: comp.clone() },
        );
        g.check(
            &format!("set_subtitle_text/{i}"),
            &ClientboundSetSubtitleTextPacket { text: comp.clone() },
        );
        g.check(
            &format!("set_action_bar_text/{i}"),
            &ClientboundSetActionBarTextPacket { text: comp.clone() },
        );
    }
    g.check(
        "tab_list/empty",
        &ClientboundTabListPacket {
            header: Component::empty(),
            footer: Component::empty(),
        },
    );
    g.check(
        "tab_list/both",
        &ClientboundTabListPacket {
            header: c[4].clone(),
            footer: c[5].clone(),
        },
    );
    g.check(
        "tab_list/unicode",
        &ClientboundTabListPacket {
            header: c[2].clone(),
            footer: c[3].clone(),
        },
    );
    g.check(
        "clear_titles/keep",
        &ClientboundClearTitlesPacket { reset_times: false },
    );
    g.check(
        "clear_titles/reset",
        &ClientboundClearTitlesPacket { reset_times: true },
    );
    g.check(
        "set_titles_animation/default",
        &ClientboundSetTitlesAnimationPacket {
            fade_in: 10,
            stay: 70,
            fade_out: 20,
        },
    );
    g.check(
        "set_titles_animation/zero",
        &ClientboundSetTitlesAnimationPacket {
            fade_in: 0,
            stay: 0,
            fade_out: 0,
        },
    );
    g.check(
        "set_titles_animation/extreme",
        &ClientboundSetTitlesAnimationPacket {
            fade_in: i32::MAX,
            stay: -1,
            fade_out: i32::MIN,
        },
    );
}

fn command_suggestions_and_custom_completions(g: &Golden) {
    let c = comps();
    let e = |text: &str, tooltip: Option<Component>| ClientboundCommandSuggestionsEntry {
        text: text.into(),
        tooltip,
    };
    g.check(
        "command_suggestions/empty",
        &ClientboundCommandSuggestionsPacket {
            id: 0,
            start: 0,
            length: 0,
            suggestions: vec![],
        },
    );
    g.check(
        "command_suggestions/entries",
        &ClientboundCommandSuggestionsPacket {
            id: 7,
            start: 4,
            length: 3,
            suggestions: vec![
                e("say", None),
                e("tellraw", Some(c[3].clone())),
                e("caf\u{e9} \u{1F600}", Some(c[6].clone())),
                e("", None),
            ],
        },
    );
    g.check(
        "command_suggestions/extreme",
        &ClientboundCommandSuggestionsPacket {
            id: i32::MAX,
            start: -1,
            length: i32::MIN,
            suggestions: vec![e(&"a".repeat(1000), None)],
        },
    );

    use ClientboundCustomChatCompletionsAction as A;
    for (name, action) in [("ADD", A::Add), ("REMOVE", A::Remove), ("SET", A::Set)] {
        g.check(
            &format!("custom_chat_completions/{name}_empty"),
            &ClientboundCustomChatCompletionsPacket {
                action,
                entries: vec![],
            },
        );
        let entries = ["alpha", "b\u{e9}ta", "\u{1F600}", ""]
            .map(String::from)
            .to_vec();
        g.check(
            &format!("custom_chat_completions/{name}_some"),
            &ClientboundCustomChatCompletionsPacket { action, entries },
        );
    }
    g.check(
        "custom_chat_completions/many",
        &ClientboundCustomChatCompletionsPacket {
            action: A::Set,
            entries: (0..300).map(|i| format!("entry{i}")).collect(),
        },
    );
    let p = expect_lossy::<ClientboundCustomChatCompletionsPacket>(
        g,
        "custom_chat_completions/unknown_action",
    );
    assert_eq!(
        p,
        ClientboundCustomChatCompletionsPacket {
            action: A::Add,
            entries: vec!["a".into()]
        }
    );
}

// ---- player info ------------------------------------------------------------------------------------------------

fn session_data(g: &Golden) -> (RemoteChatSessionData, RemoteChatSessionData) {
    let key = || -> PublicKey { public_key_from_der(g.bytes("pubkey_der")).unwrap() };
    let d1 = RemoteChatSessionData {
        session_id: Uuid::parse_str("a0a1a2a3-b0b1-c0c1-d0d1-e0e1e2e3e4e5").unwrap(),
        profile_public_key: ProfilePublicKeyData {
            expires_at: ms(1800000000000),
            key: key(),
            key_signature: (0..256).map(|i| (i * 3) as u8).collect(),
        },
    };
    let d2 = RemoteChatSessionData {
        session_id: Uuid::from_u64_pair(5, -6i64 as u64),
        profile_public_key: ProfilePublicKeyData {
            expires_at: ms(0),
            key: key(),
            key_signature: vec![0; 4096],
        },
    };
    (d1, d2)
}

fn entry(id: Uuid, profile: Option<GameProfile>) -> ClientboundPlayerInfoUpdateEntry {
    ClientboundPlayerInfoUpdateEntry {
        profile,
        ..ClientboundPlayerInfoUpdateEntry::new(id)
    }
}

fn profile(id: Uuid, name: &str, properties: Vec<Property>) -> GameProfile {
    GameProfile {
        id,
        name: name.into(),
        properties,
    }
}

fn prop(name: &str, value: &str, signature: Option<&str>) -> Property {
    Property {
        name: name.into(),
        value: value.into(),
        signature: signature.map(String::from),
    }
}

/// What decoding yields: only the fields of the packet's actions, the rest at the builder defaults.
fn project(
    full: &ClientboundPlayerInfoUpdateEntry,
    actions: &EnumSet<ClientboundPlayerInfoUpdateAction>,
) -> ClientboundPlayerInfoUpdateEntry {
    use ClientboundPlayerInfoUpdateAction as A;
    let mut e = ClientboundPlayerInfoUpdateEntry::new(full.profile_id);
    for a in actions.iter() {
        match a {
            A::AddPlayer => e.profile = full.profile.clone(),
            A::InitializeChat => e.chat_session = full.chat_session.clone(),
            A::UpdateGameMode => e.game_mode = full.game_mode,
            A::UpdateListed => e.listed = full.listed,
            A::UpdateLatency => e.latency = full.latency,
            A::UpdateDisplayName => e.display_name = full.display_name.clone(),
            A::UpdateListOrder => e.list_order = full.list_order,
            A::UpdateHat => e.show_hat = full.show_hat,
        }
    }
    e
}

fn info(
    actions: EnumSet<ClientboundPlayerInfoUpdateAction>,
    full: &[ClientboundPlayerInfoUpdateEntry],
) -> ClientboundPlayerInfoUpdatePacket {
    let entries = full.iter().map(|e| project(e, &actions)).collect();
    ClientboundPlayerInfoUpdatePacket { actions, entries }
}

fn action_set(mask: u32) -> EnumSet<ClientboundPlayerInfoUpdateAction> {
    use ClientboundPlayerInfoUpdateAction as A;
    const ALL: [A; 8] = [
        A::AddPlayer,
        A::InitializeChat,
        A::UpdateGameMode,
        A::UpdateListed,
        A::UpdateLatency,
        A::UpdateDisplayName,
        A::UpdateListOrder,
        A::UpdateHat,
    ];
    (0..8)
        .filter(|i| mask >> i & 1 != 0)
        .map(|i| ALL[i])
        .collect()
}

fn player_info_update(g: &Golden) {
    let c = comps();
    let (d1, d2) = session_data(g);
    let u1 = Uuid::parse_str("11111111-2222-3333-4444-555555555555").unwrap();
    let u2 = Uuid::from_u64_pair(u64::MAX, 0);
    let u3 = Uuid::from_u64_pair(0, 1 << 63);
    let e1 = ClientboundPlayerInfoUpdateEntry {
        profile_id: u1,
        profile: Some(profile(
            u1,
            "Steve",
            vec![
                prop("textures", "dGV4dHVyZXM=", Some("c2ln")),
                prop("plain", "v", None),
            ],
        )),
        listed: true,
        latency: 42,
        game_mode: GameType::Creative,
        display_name: Some(c[4].clone()),
        show_hat: true,
        list_order: 3,
        chat_session: Some(d1),
    };
    let e2 = ClientboundPlayerInfoUpdateEntry {
        profile_id: u2,
        profile: Some(profile(u2, "A\u{1F600}c", vec![])),
        game_mode: GameType::Adventure,
        ..ClientboundPlayerInfoUpdateEntry::new(u2)
    };
    let e3 = ClientboundPlayerInfoUpdateEntry {
        profile_id: u3,
        profile: Some(profile(u3, "Sixteen_chars_ok", vec![])),
        listed: true,
        latency: i32::MAX,
        game_mode: GameType::Spectator,
        display_name: Some(c[2].clone()),
        show_hat: true,
        list_order: -1,
        chat_session: Some(d2),
    };
    let three = [e1, e2.clone(), e3];
    for mask in 0..256 {
        g.check(
            &format!("player_info_update/mask_{mask}"),
            &info(action_set(mask), &three),
        );
    }
    g.check(
        "player_info_update/none_entries",
        &info(action_set(255), &[]),
    );
    g.check(
        "player_info_update/one_entry",
        &info(action_set(255), &[e2]),
    );
    g.check(
        "player_info_update/latency_negative",
        &info(
            action_set(1 << 4 | 1 << 6),
            &[ClientboundPlayerInfoUpdateEntry {
                latency: -1,
                list_order: i32::MIN,
                ..ClientboundPlayerInfoUpdateEntry::new(u1)
            }],
        ),
    );
    let props: Vec<Property> = (0..16)
        .map(|i| {
            prop(
                &format!("p{i}"),
                &"v".repeat(100 * i),
                (i % 2 != 0).then(|| "s".repeat(1024)).as_deref(),
            )
        })
        .collect();
    g.check(
        "player_info_update/sixteen_properties",
        &info(
            action_set(1),
            &[entry(u1, Some(profile(u1, "Steve", props)))],
        ),
    );
    g.check(
        "player_info_update/name_16_units_non_bmp",
        &info(
            action_set(1),
            &[entry(u1, Some(profile(u1, &"\u{1F600}".repeat(8), vec![])))],
        ),
    );
    let hundred: Vec<_> = (0..100u64)
        .map(|i| {
            let id = Uuid::from_u64_pair(i, !i);
            ClientboundPlayerInfoUpdateEntry {
                profile_id: id,
                profile: Some(profile(id, &format!("P{i}"), vec![])),
                listed: i % 2 == 0,
                latency: i as i32 * 7,
                game_mode: GameType::by_id(i as i32 % 4),
                display_name: (i % 3 == 0).then(|| Component::literal(format!("n{i}"))),
                show_hat: i % 5 == 0,
                list_order: i as i32,
                chat_session: None,
            }
        })
        .collect();
    g.check(
        "player_info_update/hundred",
        &info(action_set(0b1111_1101), &hundred),
    );

    let ids = [
        Uuid::from_u64_pair(0x1111_1111_2222_3333, 0x4444_5555_5555_5555),
        u2,
        u3,
    ];
    g.check(
        "player_info_remove/empty",
        &ClientboundPlayerInfoRemovePacket {
            profile_ids: vec![],
        },
    );
    g.check(
        "player_info_remove/one",
        &ClientboundPlayerInfoRemovePacket {
            profile_ids: vec![u1],
        },
    );
    g.check(
        "player_info_remove/three",
        &ClientboundPlayerInfoRemovePacket {
            profile_ids: vec![u1, ids[1], ids[2]],
        },
    );
    assert_eq!(u1, ids[0]);
}

fn player_info_update_decode_limits(g: &Golden) {
    for label in [
        "name_17",
        "name_17_utf16",
        "properties_17",
        "property_name_65",
        "property_value_32768",
        "property_signature_1025",
        "chat_session_bad_key",
    ] {
        expect_err::<ClientboundPlayerInfoUpdatePacket>(g, &format!("player_info_update/{label}"));
    }
    expect_err_any::<ClientboundPlayerInfoUpdatePacket>(g, "player_info_update/truncated");
    for label in [
        "player_info_update/unknown_game_mode",
        "player_info_update/negative_game_mode",
    ] {
        let p = expect_lossy::<ClientboundPlayerInfoUpdatePacket>(g, label);
        assert_eq!(p.entries[0].game_mode, GameType::Survival, "{label}");
    }
    assert!(
        ClientboundPlayerInfoUpdateAction::initializing()
            .contains(ClientboundPlayerInfoUpdateAction::UpdateHat)
    );
}

// ---- serverbound ------------------------------------------------------------------------------------------------

fn update0() -> LastSeenMessagesUpdate {
    LastSeenMessagesUpdate::default()
}

fn update1() -> LastSeenMessagesUpdate {
    LastSeenMessagesUpdate {
        offset: 17,
        acknowledged: FixedBitSet(bits(&[0, 3, 19])),
        checksum: -77,
    }
}

fn chat_ack_and_commands(g: &Golden) {
    for offset in [0, 1, 127, 128, 16384, -1, i32::MAX, i32::MIN] {
        g.check(
            &format!("chat_ack/{offset}"),
            &ServerboundChatAckPacket { offset },
        );
    }
    let texts = [
        "".to_owned(),
        "say hi".into(),
        "tp @s ~ ~ ~".into(),
        "say \u{1F600} caf\u{e9}".into(),
        "a".repeat(32767),
    ];
    for (i, command) in texts.into_iter().enumerate() {
        g.check(
            &format!("chat_command/{i}"),
            &ServerboundChatCommandPacket { command },
        );
    }
    expect_err::<ServerboundChatCommandPacket>(g, "chat_command/too_long");

    let t0 = ms(1700000000000);
    let signed = |command: &str, time_stamp, salt, sigs: Vec<ArgumentSignaturesEntry>, u| {
        ServerboundChatCommandSignedPacket {
            command: command.into(),
            time_stamp,
            salt,
            argument_signatures: ArgumentSignatures { entries: sigs },
            last_seen_messages: u,
        }
    };
    let arg = |name: &str, s| ArgumentSignaturesEntry {
        name: name.into(),
        signature: sig(s),
    };
    g.check(
        "chat_command_signed/empty",
        &signed("say hi", t0, 99, vec![], update0()),
    );
    g.check(
        "chat_command_signed/one",
        &signed("msg a hi", ms(1), -1, vec![arg("message", 1)], update1()),
    );
    let eight = (0..8)
        .map(|i| arg(&"n".repeat(i as usize + 9), i + 10))
        .collect();
    g.check(
        "chat_command_signed/eight",
        &signed(
            "x \u{1F600}",
            ms(i64::MIN / 1000000),
            i64::MAX,
            eight,
            update1(),
        ),
    );
    g.check(
        "chat_command_signed/name16",
        &signed(
            "",
            ms(0),
            0,
            vec![arg(&"\u{1F600}".repeat(8), 2)],
            update0(),
        ),
    );
    for label in ["nine", "name17", "name17_utf16"] {
        expect_err::<ArgumentSignatures>(g, &format!("argsigs/{label}"));
    }
    expect_err_any::<ArgumentSignatures>(g, "argsigs/short_signature");
}

fn chat(g: &Golden) {
    let t0 = ms(1700000000000);
    let packet = |message: &str, time_stamp, salt, signature, u| ServerboundChatPacket {
        message: message.into(),
        time_stamp,
        salt,
        signature,
        last_seen_messages: u,
    };
    g.check("chat/unsigned", &packet("hello", t0, 0, None, update0()));
    g.check(
        "chat/signed",
        &packet("hello", t0, 5, Some(sig(1)), update1()),
    );
    g.check("chat/empty", &packet("", ms(0), i64::MIN, None, update0()));
    g.check(
        "chat/max_len",
        &packet(&"m".repeat(256), t0, i64::MAX, Some(sig(2)), update1()),
    );
    g.check(
        "chat/unicode",
        &packet(
            &format!("\u{1F600} caf\u{e9} \u{4e2d}{}", "\u{1F600}".repeat(100)),
            t0,
            -9,
            None,
            update1(),
        ),
    );
    for label in ["too_long", "too_long_utf16", "too_long_bytes"] {
        expect_err::<ServerboundChatPacket>(g, &format!("chat/{label}"));
    }
}

fn chat_session_update(g: &Golden) {
    let (d1, _) = session_data(g);
    let key = d1.profile_public_key.key.clone();
    let sid = d1.session_id;
    // the session data on its own (nested codec) and inside the packet
    g.check_value("session_data", &d1);
    g.check(
        "chat_session_update/basic",
        &ServerboundChatSessionUpdatePacket { chat_session: d1 },
    );
    let data = |session_id, expires_at, key_signature| RemoteChatSessionData {
        session_id,
        profile_public_key: ProfilePublicKeyData {
            expires_at: ms(expires_at),
            key: key.clone(),
            key_signature,
        },
    };
    g.check(
        "chat_session_update/empty_signature",
        &ServerboundChatSessionUpdatePacket {
            chat_session: data(Uuid::nil(), -5, vec![]),
        },
    );
    g.check(
        "chat_session_update/max_signature",
        &ServerboundChatSessionUpdatePacket {
            chat_session: data(sid, 0, vec![0; 4096]),
        },
    );
    for label in ["signature_4097", "key_513", "key_garbage"] {
        expect_err::<ServerboundChatSessionUpdatePacket>(
            g,
            &format!("chat_session_update/{label}"),
        );
    }
    assert!(g.bytes("pubkey_der").len() <= 512);
}

fn command_suggestion(g: &Golden) {
    let p = |id, command: &str| ServerboundCommandSuggestionPacket {
        id,
        command: command.into(),
    };
    let cases = [
        ("basic", p(3, "/say h")),
        ("empty", p(0, "")),
        ("unicode", p(-1, "\u{1F600} caf\u{e9}")),
        ("len256", p(i32::MAX, &"c".repeat(256))),
    ];
    for (name, packet) in &cases {
        // vanilla encodes both bindings alike; each decodes with both codecs (all of these fit the 256 limit)
        for prefix in ["blocks", "chat"] {
            let label = format!("command_suggestion/{prefix}_{name}");
            g.check(&label, packet);
            let decoded =
                ServerboundCommandSuggestionPacket::decode_chat_only(&mut body_of(g.bytes(&label)))
                    .unwrap();
            assert_eq!(&decoded, packet, "{label}");
        }
    }
    let long = p(1, &"c".repeat(32500));
    g.check("command_suggestion/len32500", &long);
    expect_err::<ServerboundCommandSuggestionPacket>(g, "command_suggestion/blocks_32501");

    // 257 characters: fine for command-block users, rejected (and skipped) by the chat-only codec
    let p257 = p(1, &"c".repeat(257));
    let mut b = g.bytes("command_suggestion/ok_257_blocks");
    assert_eq!(
        ServerboundCommandSuggestionPacket::decode(&mut b).unwrap(),
        p257
    );
    let chat_only = |label: &str| {
        ServerboundCommandSuggestionPacket::decode_chat_only(&mut g.bytes(label))
            .expect_err(label)
            .to_string()
    };
    // vanilla wraps the DecoderException (-> skip) and its message becomes `Throwable.toString()` of the cause
    let wrapped = |label: &str| {
        vanilla_error(g, label)
            .strip_prefix("io.netty.handler.codec.DecoderException: ")
            .expect("wrapped")
            .to_owned()
    };
    assert_eq!(
        chat_only("command_suggestion/ok_257_blocks"),
        wrapped("command_suggestion/chat_only_257")
    );
    for label in ["chat_only_257", "chat_only_257_utf16", "chat_only_32501"] {
        let label = format!("command_suggestion/{label}");
        assert_eq!(chat_only(&label), wrapped(&label), "{label}");
    }
    // `VarInt too big` is a plain RuntimeException in vanilla: not wrapped, not skipped
    let label = "command_suggestion/chat_only_bad_varint";
    let e = ServerboundCommandSuggestionPacket::decode_chat_only(&mut g.bytes(label)).unwrap_err();
    assert_eq!(e, DecodeError::VarIntTooBig);
    assert_eq!(e.to_string(), vanilla_error(g, label));
    // a truncated frame is not a skip (IndexOutOfBoundsException in vanilla)
    assert!(matches!(
        ServerboundCommandSuggestionPacket::decode_chat_only(&mut &[1u8][..]),
        Err(DecodeError::Eof { .. })
    ));
    // the 257-character bodies without the id prefix are what the chat-only codec sees
    for label in [
        "command_suggestion/body_len257",
        "command_suggestion/body_len257_utf16",
    ] {
        let mut bytes = g.bytes(label);
        assert!(
            ServerboundCommandSuggestionPacket::decode_chat_only(&mut bytes).is_err(),
            "{label}"
        );
        assert!(
            ServerboundCommandSuggestionPacket::decode(&mut g.bytes(label)).is_ok(),
            "{label}"
        );
    }
}

// ---- commands -----------------------------------------------------------------------------------------------------

use ArgumentTypeTemplate as T;

fn registry(name: &str) -> Identifier {
    id(name)
}

/// (label, template) for every `ArgumentTypeInfo` of `ArgumentTypeInfos.bootstrap`, with the property variants of GenChat.
fn argument_cases() -> Vec<(&'static str, ArgumentTypeTemplate)> {
    let f = f32::MAX;
    let d = f64::MAX;
    vec![
        ("bool", T::Bool),
        ("float_unbounded", T::Float { min: -f, max: f }),
        ("float_min", T::Float { min: -1.5, max: f }),
        (
            "float_both",
            T::Float {
                min: -1.5,
                max: 2.5,
            },
        ),
        ("float_max_only", T::Float { min: -f, max: 3.0 }),
        (
            "float_nan_edge",
            T::Float {
                min: f32::from_bits(1),
                max: f32::INFINITY,
            },
        ),
        ("double_unbounded", T::Double { min: -d, max: d }),
        ("double_min", T::Double { min: -3e10, max: d }),
        (
            "double_both",
            T::Double {
                min: 0.1,
                max: 1e300,
            },
        ),
        ("double_max_only", T::Double { min: -d, max: -0.5 }),
        (
            "integer_unbounded",
            T::Integer {
                min: i32::MIN,
                max: i32::MAX,
            },
        ),
        (
            "integer_min",
            T::Integer {
                min: -5,
                max: i32::MAX,
            },
        ),
        (
            "integer_both",
            T::Integer {
                min: -2147483647,
                max: 2147483646,
            },
        ),
        (
            "integer_max_only",
            T::Integer {
                min: i32::MIN,
                max: 100,
            },
        ),
        (
            "long_unbounded",
            T::Long {
                min: i64::MIN,
                max: i64::MAX,
            },
        ),
        (
            "long_min",
            T::Long {
                min: -9000000000,
                max: i64::MAX,
            },
        ),
        (
            "long_both",
            T::Long {
                min: 1,
                max: i64::MAX - 1,
            },
        ),
        (
            "long_max_only",
            T::Long {
                min: i64::MIN,
                max: 77,
            },
        ),
        ("string_word", T::String(StringArgumentType::SingleWord)),
        (
            "string_phrase",
            T::String(StringArgumentType::QuotablePhrase),
        ),
        ("string_greedy", T::String(StringArgumentType::GreedyPhrase)),
        (
            "entity_entity",
            T::Entity {
                single: true,
                players_only: false,
            },
        ),
        (
            "entity_entities",
            T::Entity {
                single: false,
                players_only: false,
            },
        ),
        (
            "entity_player",
            T::Entity {
                single: true,
                players_only: true,
            },
        ),
        (
            "entity_players",
            T::Entity {
                single: false,
                players_only: true,
            },
        ),
        ("game_profile", T::GameProfile),
        ("block_pos", T::BlockPos),
        ("column_pos", T::ColumnPos),
        ("vec3", T::Vec3),
        ("vec2", T::Vec2),
        ("block_state", T::BlockState),
        ("block_predicate", T::BlockPredicate),
        ("item_stack", T::ItemStack),
        ("item_predicate", T::ItemPredicate),
        ("team_color", T::TeamColor),
        ("hex_color", T::HexColor),
        ("component", T::Component),
        ("style", T::Style),
        ("message", T::Message),
        ("nbt_compound_tag", T::NbtCompoundTag),
        ("nbt_tag", T::NbtTag),
        ("nbt_path", T::NbtPath),
        ("objective", T::Objective),
        ("objective_criteria", T::ObjectiveCriteria),
        ("operation", T::Operation),
        ("particle", T::Particle),
        ("angle", T::Angle),
        ("rotation", T::Rotation),
        ("scoreboard_slot", T::ScoreboardSlot),
        ("score_holder_single", T::ScoreHolder { multiple: false }),
        ("score_holder_multiple", T::ScoreHolder { multiple: true }),
        ("swizzle", T::Swizzle),
        ("team", T::Team),
        ("item_slot", T::ItemSlot),
        ("item_slots", T::ItemSlots),
        ("resource_location", T::ResourceLocation),
        ("function", T::Function),
        ("entity_anchor", T::EntityAnchor),
        ("int_range", T::IntRange),
        ("float_range", T::FloatRange),
        ("dimension", T::Dimension),
        ("gamemode", T::Gamemode),
        ("time_default", T::Time { min: 0 }),
        ("time_min", T::Time { min: 100 }),
        ("time_negative", T::Time { min: i32::MIN }),
        ("resource_or_tag_item", T::ResourceOrTag(registry("item"))),
        (
            "resource_or_tag_biome",
            T::ResourceOrTag(registry("worldgen/biome")),
        ),
        (
            "resource_or_tag_key_block",
            T::ResourceOrTagKey(registry("block")),
        ),
        (
            "resource_or_tag_key_structure",
            T::ResourceOrTagKey(registry("worldgen/structure")),
        ),
        ("resource_enchantment", T::Resource(registry("enchantment"))),
        ("resource_entity_type", T::Resource(registry("entity_type"))),
        (
            "resource_key_advancement",
            T::ResourceKey(registry("advancement")),
        ),
        ("resource_key_recipe", T::ResourceKey(registry("recipe"))),
        (
            "resource_selector_biome",
            T::ResourceSelector(registry("worldgen/biome")),
        ),
        (
            "resource_selector_item",
            T::ResourceSelector(registry("item")),
        ),
        ("template_mirror", T::TemplateMirror),
        ("template_rotation", T::TemplateRotation),
        ("heightmap", T::Heightmap),
        ("loot_table", T::LootTable),
        ("loot_predicate", T::LootPredicate),
        ("loot_modifier", T::LootModifier),
        ("context_float_provider", T::ContextFloatProvider),
        ("context_int_provider", T::ContextIntProvider),
        ("slot_source", T::SlotSource),
        ("dialog", T::Dialog),
        ("feature", T::Feature),
        ("swing_animation", T::SwingAnimation),
        ("uuid", T::Uuid),
    ]
}

type Entry = ClientboundCommandsEntry;

fn tree(entries: Vec<Entry>) -> ClientboundCommandsPacket {
    ClientboundCommandsPacket {
        entries,
        root_index: 0,
    }
}

fn commands_every_argument_type(g: &Golden) {
    let cases = argument_cases();
    // every registry entry is covered: the registry's 62 entries by the bare/property types used
    let mut seen: Vec<u32> = cases.iter().map(|(_, t)| t.argument_type().id()).collect();
    seen.sort_unstable();
    seen.dedup();
    assert_eq!(seen, (0..62).collect::<Vec<_>>());
    for (label, template) in cases {
        let p = tree(vec![
            Entry::root(vec![1]),
            Entry::literal("t", vec![2]),
            Entry::argument("a", template, None, vec![]),
        ]);
        g.check(&format!("commands/arg_{label}"), &p);
    }
}

fn commands_suggestions_flags_and_trees(g: &Golden) {
    for (label, s) in [
        ("minecraft_ask_server", "minecraft:ask_server"),
        ("minecraft_available_sounds", "minecraft:available_sounds"),
        (
            "minecraft_summonable_entities",
            "minecraft:summonable_entities",
        ),
        ("minecraft_all_recipes", "minecraft:all_recipes"),
        ("mod_custom_provider", "mod:custom/provider"),
    ] {
        let a = Entry::argument(
            "a",
            T::String(StringArgumentType::SingleWord),
            Some(id(s)),
            vec![],
        )
        .executable();
        g.check(
            &format!("commands/suggest_{label}"),
            &tree(vec![Entry::root(vec![1]), Entry::literal("t", vec![2]), a]),
        );
    }
    g.check("commands/root_only", &tree(vec![Entry::root(vec![])]));
    for f in 0..4 {
        let (mut l, mut a) = (
            Entry::literal("lit", vec![2]),
            Entry::argument(
                "arg",
                T::Integer {
                    min: 0,
                    max: i32::MAX,
                },
                (f & 2 != 0).then(|| id("minecraft:ask_server")),
                vec![],
            ),
        );
        if f & 1 != 0 {
            l = l.executable();
            a = a.executable();
        }
        if f & 2 != 0 {
            l = l.restricted();
            a = a.restricted();
        }
        g.check(
            &format!("commands/flags_{f}"),
            &tree(vec![Entry::root(vec![1]), l, a]),
        );
    }

    // breadth-first numbering of GenChat's `commands/tree`
    let player_entities = T::Entity {
        single: false,
        players_only: false,
    };
    let nodes = vec![
        Entry::root(vec![1, 2, 3, 4, 5, 6]),                      // 0
        Entry::literal("execute", vec![7, 8, 9]),                 // 1
        Entry::literal("say", vec![10]),                          // 2
        Entry::literal("stop", vec![]).executable().restricted(), // 3
        Entry::literal("give", vec![11]).restricted(),            // 4
        Entry::literal("tp", vec![12, 13]),                       // 5
        Entry::literal("playsound", vec![14]),                    // 6
        Entry::literal("run", vec![]).redirect_to(0),             // 7
        Entry::literal("as", vec![15]),                           // 8
        Entry::literal("if", vec![16]),                           // 9
        Entry::argument("message", T::Message, None, vec![]).executable(), // 10
        Entry::argument(
            "targets",
            T::Entity {
                single: false,
                players_only: true,
            },
            None,
            vec![17],
        ), // 11
        Entry::argument(
            "destination",
            T::Entity {
                single: true,
                players_only: false,
            },
            None,
            vec![],
        )
        .executable(), // 12
        Entry::argument("location", T::Vec3, None, vec![]).executable(), // 13
        Entry::argument(
            "sound",
            T::ResourceLocation,
            Some(id("minecraft:available_sounds")),
            vec![],
        ), // 14
        Entry::argument("targets", player_entities, None, vec![]).redirect_to(1), // 15
        Entry::literal("block", vec![18]),                        // 16
        Entry::argument("item", T::ItemStack, None, vec![19]).executable(), // 17
        Entry::argument("pos", T::BlockPos, None, vec![20]),      // 18
        Entry::argument(
            "count",
            T::Integer {
                min: 1,
                max: i32::MAX,
            },
            None,
            vec![],
        )
        .executable(), // 19
        Entry::argument("block", T::BlockPredicate, None, vec![])
            .executable()
            .redirect_to(1), // 20
    ];
    g.check("commands/tree", &tree(nodes));

    let many: Vec<Entry> = std::iter::once(Entry::root((1..=300).collect()))
        .chain((0..300).map(|i| Entry::literal(format!("cmd{i}"), vec![301 + i]).executable()))
        .chain((0..300).map(|i| {
            Entry::argument(
                "x",
                T::Double {
                    min: f64::from(i),
                    max: f64::MAX,
                },
                None,
                vec![],
            )
        }))
        .collect();
    g.check("commands/many", &tree(many));
}

fn commands_decode_validation_and_leniency(g: &Golden) {
    type P = ClientboundCommandsPacket;
    // valid shapes
    let p = expect_lossy::<P>(g, "commands_bad/valid_two_nodes");
    assert_eq!(p.entries.len(), 2);
    expect_lossy::<P>(g, "commands_bad/child_and_redirect_loop");
    // the loop checks
    for label in [
        "self_redirect",
        "redirect_cycle",
        "self_child",
        "child_cycle",
    ] {
        let label = format!("commands_bad/{label}");
        expect_err::<P>(g, &label);
        assert_eq!(
            vanilla_error(g, &label),
            "Server sent an impossible command tree"
        );
    }
    // indices out of range / negative are accepted
    let p = expect_lossy::<P>(g, "commands_bad/out_of_range");
    assert_eq!(p.entries[0].children, vec![5, -1, 99]);
    assert_eq!(p.entries[0].redirect, 77);
    expect_lossy::<P>(g, "commands_bad/negative_redirect");
    expect_lossy::<P>(g, "commands_bad/root_index_out_of_range");
    // unreadable node kinds become stub-less nodes
    let p = expect_lossy::<P>(g, "commands_bad/type3");
    assert_eq!(
        p.entries[0],
        Entry {
            stub: None,
            flags: 3 | 4,
            redirect: 0,
            children: vec![]
        }
    );
    for label in ["unknown_argument_type", "negative_argument_type"] {
        let p = expect_lossy::<P>(g, &format!("commands_bad/{label}"));
        assert_eq!(p.entries[0].stub, None, "{label}");
    }
    let p = expect_lossy::<P>(g, "commands_bad/stray_flags");
    assert_eq!(p.entries[0].flags, 1 | 16 | 64 | 128 | 32);
    assert_eq!(
        p.entries[0].stub,
        Some(ClientboundCommandsNodeStub::Literal("a".into()))
    );
    // argument property reads
    for label in [
        "string_type_3",
        "string_type_negative",
        "bad_suggestion_id",
        "bad_resource_registry",
    ] {
        expect_err::<P>(g, &format!("commands_bad/{label}"));
    }
    let arg = |p: P| match p.entries[0].stub.clone() {
        Some(ClientboundCommandsNodeStub::Argument { argument_type, .. }) => argument_type,
        other => panic!("{other:?}"),
    };
    assert_eq!(
        arg(expect_lossy::<P>(g, "commands_bad/number_flags")),
        T::Integer {
            min: i32::MIN,
            max: i32::MAX
        }
    );
    assert_eq!(
        arg(expect_lossy::<P>(g, "commands_bad/number_extreme_bounds")),
        T::Integer {
            min: i32::MIN,
            max: i32::MAX
        }
    );
    assert_eq!(
        arg(expect_lossy::<P>(g, "commands_bad/entity_flags")),
        T::Entity {
            single: false,
            players_only: false
        }
    );
    assert_eq!(
        arg(expect_lossy::<P>(g, "commands_bad/score_holder_flags")),
        T::ScoreHolder { multiple: true }
    );
    // size limits of the children array, truncation, negative counts
    expect_err::<P>(g, "commands_bad/children_too_many");
    expect_err::<P>(g, "commands_bad/children_negative");
    expect_err_any::<P>(g, "commands_bad/truncated");
    expect_err::<P>(g, "commands_bad/negative_count");
}

#[test]
fn chat_packets_match_vanilla() {
    let g = g();
    message_signature_packed(&g);
    last_seen_and_update(&g);
    filter_mask(&g);
    signed_message_body(&g);
    chat_type_decoration(&g);
    player_chat(&g);
    disguised_and_delete_chat(&g);
    system_chat_titles_and_tab_list(&g);
    command_suggestions_and_custom_completions(&g);
    player_info_update(&g);
    player_info_update_decode_limits(&g);
    chat_ack_and_commands(&g);
    chat(&g);
    chat_session_update(&g);
    command_suggestion(&g);
    commands_every_argument_type(&g);
    commands_suggestions_flags_and_trees(&g);
    commands_decode_validation_and_leniency(&g);
    g.assert_all_used();
}
