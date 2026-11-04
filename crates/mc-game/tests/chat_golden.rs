//! Replays `tests/fixtures/chat_golden.jsonl` (generator: `ChatGolden.java`, real vanilla classes) against `server::chat`.

use mc_core::ident::Identifier;
use mc_game::server::chat::chat_type::{self, Bound};
use mc_game::server::chat::last_seen_messages::{LastSeenMessagesTracker, LastSeenMessagesValidator};
use mc_game::server::chat::message_signature::MessageSignature;
use mc_game::server::chat::player_chat_message::PlayerChatMessage;
use mc_game::server::chat::signed_message::{Decoder, SignedMessageBody, SignedMessageLink};
use mc_game::server::chat::{FilterMaskExt, LastSeenMessages, MessageSignatureCache, RemoteChatSession};
use mc_protocol::codec::{BitSet, FixedBitSet, Instant};
use mc_protocol::packets::play::{ChatType, FilterMask, LastSeenMessagesUpdate, ProfilePublicKeyData};
use mc_text::{Component, ComponentContents};
use serde_json::Value;
use std::sync::Arc;
use uuid::Uuid;

fn lines(kind: &str) -> Vec<Value> {
    let text = include_str!("fixtures/chat_golden.jsonl");
    text.lines().map(|l| serde_json::from_str::<Value>(l).unwrap()).filter(|v| v["t"] == kind).collect()
}

fn sig(i: i64) -> MessageSignature {
    let mut b = [0x5au8; 256];
    b[0] = i as u8;
    b[1] = (i >> 8) as u8;
    MessageSignature(b)
}

fn id_of(s: &MessageSignature) -> i64 {
    i64::from(s.0[0]) | i64::from(s.0[1]) << 8
}

fn ids(v: &Value) -> Vec<i64> {
    v.as_array().unwrap().iter().map(|x| x.as_i64().unwrap()).collect()
}

fn hex(s: &str) -> Vec<u8> {
    (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap()).collect()
}

#[test]
fn signature_cache_matches_vanilla() {
    let cases = lines("cache");
    assert_eq!(cases.len(), 3);
    for case in cases {
        let cap = case["capacity"].as_u64().unwrap() as usize;
        let mut cache = MessageSignatureCache::new(cap);
        for (n, op) in case["ops"].as_array().unwrap().iter().enumerate() {
            let entries: Vec<_> = ids(op).into_iter().map(sig).collect();
            cache.push(&entries);
            let state: Vec<i64> = (0..cap).map(|i| cache.unpack(i as i32).unwrap().map_or(-1, |s| id_of(&s))).collect();
            assert_eq!(state, ids(&case["states"][n]), "cap {cap} step {n}");
            let packs: Vec<i64> = (0..cap * 2 + 3).map(|i| i64::from(cache.pack(&sig(i as i64)))).collect();
            assert_eq!(packs, ids(&case["packs"][n]), "cap {cap} step {n} pack");
        }
    }
}

#[test]
fn last_seen_window_matches_vanilla() {
    let cases = lines("lastseen");
    assert_eq!(cases.len(), 40);
    let (mut ok, mut err) = (0, 0);
    for (c, case) in cases.iter().enumerate() {
        let mut tracker = LastSeenMessagesTracker::new(20);
        let mut validator = LastSeenMessagesValidator::new(20);
        for (n, step) in case["steps"].as_array().unwrap().iter().enumerate() {
            let at = format!("scenario {c} step {n}");
            match step["k"].as_str().unwrap() {
                "msg" => {
                    let m = sig(step["id"].as_i64().unwrap());
                    validator.add_pending(&m);
                    let added = tracker.add_pending(&m, step["shown"].as_bool().unwrap());
                    assert_eq!(added, step["added"].as_bool().unwrap(), "{at}");
                    assert_eq!(validator.tracked_messages_count() as i64, step["tracked"].as_i64().unwrap(), "{at}");
                    assert_eq!(i64::from(tracker.offset()), step["offset"].as_i64().unwrap(), "{at}");
                }
                "update" => {
                    let u = tracker.generate_and_apply_update();
                    assert_eq!(i64::from(u.update.offset), step["genOffset"].as_i64().unwrap(), "{at}");
                    let ack: i64 = u.update.acknowledged.0.to_longs().first().copied().unwrap_or(0);
                    assert_eq!(ack, step["genAck"].as_i64().unwrap(), "{at}");
                    assert_eq!(i64::from(u.update.checksum), step["genChecksum"].as_i64().unwrap(), "{at}");
                    assert_eq!(u.last_seen.entries.iter().map(id_of).collect::<Vec<_>>(), ids(&step["genLastSeen"]), "{at}");
                    let update = LastSeenMessagesUpdate {
                        offset: step["offset"].as_i64().unwrap() as i32,
                        acknowledged: FixedBitSet(BitSet::from_longs(&[step["ack"].as_i64().unwrap()])),
                        checksum: step["checksum"].as_i64().unwrap() as i8,
                    };
                    match validator.apply_update(&update) {
                        Ok(ls) => {
                            ok += 1;
                            assert_eq!(ls.entries.iter().map(id_of).collect::<Vec<_>>(), ids(&step["ok"]), "{at}");
                        }
                        Err(e) => {
                            err += 1;
                            assert_eq!(e.0, step["err"].as_str().unwrap(), "{at}");
                        }
                    }
                }
                "ackoffset" => match validator.apply_offset(step["offset"].as_i64().unwrap() as i32) {
                    Ok(()) => assert!(step["ok"].as_bool().unwrap(), "{at}"),
                    Err(e) => assert_eq!(e.0, step["err"].as_str().unwrap(), "{at}"),
                },
                other => panic!("{other}"),
            }
            assert_eq!(validator.tracked_messages_count() as i64, step["count"].as_i64().unwrap(), "{at}");
        }
    }
    assert!(ok > 100 && err > 100, "scenario mix too thin: ok {ok} err {err}");
}

fn translation_key(c: &Component) -> String {
    match c.contents() {
        ComponentContents::Translatable(t) => t.key.clone(),
        other => panic!("{other:?}"),
    }
}

#[test]
fn signed_chain_matches_vanilla() {
    let cases = lines("chain");
    assert_eq!(cases.len(), 6);
    for (s, case) in cases.iter().enumerate() {
        let der = hex(case["pubkey"].as_str().unwrap());
        let key = mc_protocol::auth::public_key_from_der(&der).unwrap();
        let data = ProfilePublicKeyData { expires_at: Instant(case["expires_at"].as_i64().unwrap()), key, key_signature: vec![] };
        let profile: Uuid = case["profile"].as_str().unwrap().parse().unwrap();
        let session: Uuid = case["session"].as_str().unwrap().parse().unwrap();
        let remote = RemoteChatSession::new(session, data).unwrap();
        let mut decoder = remote.create_message_decoder(profile);
        for (i, m) in case["messages"].as_array().unwrap().iter().enumerate() {
            let at = format!("scenario {s} message {i}");
            let body = SignedMessageBody {
                content: m["content"].as_str().unwrap().to_owned(),
                time_stamp: Instant(m["ts"].as_i64().unwrap()),
                salt: m["salt"].as_i64().unwrap(),
                last_seen: LastSeenMessages { entries: ids(&m["seen"]).into_iter().map(sig).collect() },
            };
            // the bytes vanilla signed == the bytes we reconstruct for link index i
            let mut payload = Vec::new();
            PlayerChatMessage::update_signature(
                &mut payload,
                &SignedMessageLink { index: i as i32, sender: profile, session_id: session },
                &body,
            );
            assert_eq!(payload, hex(m["payload"].as_str().unwrap()), "{at}");
            if m.get("skip").is_some() {
                continue;
            }
            let mut signature = MessageSignature(hex(m["signature"].as_str().unwrap()).try_into().unwrap());
            let mut delivered = Some(signature.clone());
            let mut body = body;
            match m.get("tamper").and_then(Value::as_str) {
                Some("flipsig") => {
                    signature.0[10] ^= 1;
                    delivered = Some(signature);
                }
                Some("nosig") => delivered = None,
                Some("content") => body.content.push('!'),
                _ => {}
            }
            match decoder.unpack(delivered.as_ref(), body) {
                Ok(message) => {
                    assert!(m["ok"].as_bool().unwrap_or(false), "{at}: accepted but vanilla rejected");
                    assert_eq!(i64::from(message.link.index), m["link_index"].as_i64().unwrap(), "{at}");
                }
                Err(e) => assert_eq!(translation_key(e.component()), m["err"].as_str().unwrap(), "{at}"),
            }
        }
    }
}

#[test]
fn filter_mask_matches_vanilla() {
    let cases = lines("filter");
    assert_eq!(cases.len(), 25);
    for case in cases {
        let text = case["text"].as_str().unwrap();
        let mask = match case["kind"].as_str().unwrap() {
            "pass" => FilterMask::PassThrough,
            "full" => FilterMask::FullyFiltered,
            _ => {
                let mut mask = FilterMask::partial();
                for bit in ids(&case["bits"]) {
                    mask.set_filtered(bit as usize);
                }
                mask
            }
        };
        let applied = mask.apply(text);
        assert_eq!(applied.is_none(), case["applied_null"].as_bool().unwrap());
        if let Some(applied) = applied {
            // a mask cutting a surrogate pair leaves a lone surrogate in Java (printed as '?'); Rust strings hold U+FFFD
            assert_eq!(applied.replace('\u{fffd}', "?"), case["applied"].as_str().unwrap(), "{case}");
        }
        match mask.apply_with_formatting(text) {
            None => assert!(case["formatted"].is_null(), "{case}"),
            Some(c) => {
                let json = c.to_json().unwrap().to_string().replace('\u{fffd}', "?");
                assert_eq!(serde_json::from_str::<Value>(&json).unwrap(), case["formatted"], "{case}")
            }
        }
    }
}

#[test]
fn chat_type_decoration_matches_vanilla() {
    let cases = lines("decorate");
    assert_eq!(cases.len(), 4);
    for case in cases {
        let key = case["key"].as_str().unwrap();
        let decoration = match key {
            "chat.type.text" => chat_type::with_sender(key),
            "commands.message.display.incoming" => chat_type::incoming_direct_message(key),
            "commands.message.display.outgoing" => chat_type::outgoing_direct_message(key),
            "chat.type.team.text" => chat_type::team_message(key),
            other => panic!("{other}"),
        };
        let bound = Bound {
            chat_type_id: 0,
            chat_type: Arc::new(ChatType { chat: decoration.clone(), narration: decoration }),
            name: Component::literal("Steve"),
            target_name: Some(Component::literal("Alex")),
        };
        let decorated = bound.decorate(&Component::literal("hi there"));
        assert_eq!(decorated.to_json().unwrap(), case["chat"]);
        assert_eq!(decorated.get_string(), case["string"].as_str().unwrap());
    }
}

#[test]
fn chat_type_bootstrap_equals_data_pack() {
    for (name, expected) in chat_type::bootstrap() {
        let id = Identifier::try_parse(name).unwrap();
        let json = mc_data::datapack::read_string(&format!("data/{}/chat_type/{}.json", id.namespace(), id.path())).unwrap();
        let parsed = chat_type::chat_type_from_json(&serde_json::from_str(&json).unwrap()).unwrap();
        assert_eq!(parsed, expected, "{name}");
    }
}
