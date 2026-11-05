//! Differential test: the JSON files the real vanilla server wrote for a sequence of `op` / `whitelist` / `ban` /
//! `ban-ip` console commands (offline mode; `fixtures/dedicated/lists/generate.sh`) vs ours for the same operations.
//! Timestamps are masked (they depend on the run and the local time zone).

use std::sync::Arc;

use mc_game::server::notifications::EmptyNotificationService;
use mc_game::server::permissions::LevelBasedPermissionSet;
use mc_game::server::players::ban_list_entry::{IpBanListEntry, UserBanListEntry};
use mc_game::server::players::name_resolver::{CachedUserNameToIdResolver, NoProfileLookup};
use mc_game::server::players::gson;
use mc_game::server::players::{IpBanList, NameAndId, ServerOpList, ServerOpListEntry, UserBanList, UserWhiteList, UserWhiteListEntry};

const NAMES: [&str; 18] = [
    "Alice", "Bob", "Carol", "Dave", "Eve", "Frank", "Grace", "Heidi", "Ivan", "Judy", "Mallory", "Niaj", "Olivia", "Peggy", "Rupert", "Sybil", "Trent",
    "Victor",
];

fn mask_dates(text: &str) -> String {
    // `yyyy-MM-dd HH:mm:ss +ZZZZ`
    let pattern = b"dddd-dd-dd dd:dd:dd Zdddd";
    let bytes = text.as_bytes();
    let mut out = String::new();
    let mut i = 0;
    while i < bytes.len() {
        let matches = i + pattern.len() <= bytes.len()
            && pattern.iter().zip(&bytes[i..]).all(|(p, b)| match p {
                b'd' => b.is_ascii_digit(),
                b'Z' => *b == b'+' || *b == b'-',
                other => other == b,
            });
        if matches {
            out.push_str("<date>");
            i += pattern.len();
        } else {
            out.push(bytes[i] as char);
            i += 1;
        }
    }
    out
}

/// The profiles vanilla resolved (real Mojang accounts where they exist, offline ids otherwise), by lower-case name.
fn profiles(fixtures: &std::path::Path) -> impl Fn(&str) -> NameAndId {
    let text = std::fs::read_to_string(fixtures.join("usercache.json")).unwrap();
    let Some(gson::JsonElement::Array(entries)) = gson::parse(&text, true).unwrap() else { panic!("usercache is an array") };
    let users: Vec<NameAndId> = entries.iter().map(|e| NameAndId::from_json(e.as_object().unwrap()).unwrap().unwrap()).collect();
    move |name: &str| users.iter().find(|u| u.name.eq_ignore_ascii_case(name)).unwrap().clone()
}

fn read(path: &std::path::Path) -> String {
    mask_dates(&std::fs::read_to_string(path).unwrap())
}

#[test]
fn lists_match_vanilla_files() {
    let dir = tempfile::tempdir().unwrap();
    let fixtures = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../mc-game/tests/fixtures/dedicated/lists");
    let notifications = Arc::new(EmptyNotificationService);

    let mut ops = ServerOpList::new(dir.path().join("ops.json"), notifications.clone());
    let mut white = UserWhiteList::new(dir.path().join("whitelist.json"), notifications.clone());
    let mut bans = UserBanList::new(dir.path().join("banned-players.json"), notifications.clone());
    let mut ip_bans = IpBanList::new(dir.path().join("banned-ips.json"), notifications.clone());

    let user_of = profiles(&fixtures);
    for name in NAMES {
        let user = user_of(name);
        assert!(ops.add(ServerOpListEntry::new(user.clone(), LevelBasedPermissionSet::OWNER, false)));
        assert!(white.add(UserWhiteListEntry::new(user)));
    }
    for (name, reason) in [("Alice", Some("griefing <b>&x'=y")), ("Dave", None), ("Trent", Some("spam"))] {
        assert!(bans.add(UserBanListEntry::new(Some(user_of(name)), None, Some("Server"), None, reason)));
    }
    for (ip, reason) in [("1.2.3.4", Some("ip reason")), ("10.0.0.1", None), ("192.168.1.77", Some("x"))] {
        assert!(ip_bans.add(IpBanListEntry::new(Some(ip.to_owned()), None, Some("Server"), None, reason)));
    }
    assert!(ops.remove(&user_of("Bob")));
    assert!(white.remove(&user_of("Eve")));

    for file in ["ops.json", "whitelist.json", "banned-players.json", "banned-ips.json"] {
        assert_eq!(read(&dir.path().join(file)), read(&fixtures.join(file)), "{file}");
    }
}

#[test]
fn reload_and_resave_is_stable() {
    let dir = tempfile::tempdir().unwrap();
    let fixtures = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../mc-game/tests/fixtures/dedicated/lists");
    let notifications = Arc::new(EmptyNotificationService);
    for file in ["ops.json", "whitelist.json", "banned-players.json", "banned-ips.json", "usercache.json"] {
        std::fs::copy(fixtures.join(file), dir.path().join(file)).unwrap();
    }
    let mut ops = ServerOpList::new(dir.path().join("ops.json"), notifications.clone());
    ops.load().unwrap();
    ops.save().unwrap();
    let mut white = UserWhiteList::new(dir.path().join("whitelist.json"), notifications.clone());
    white.load().unwrap();
    white.save().unwrap();
    let mut bans = UserBanList::new(dir.path().join("banned-players.json"), notifications.clone());
    bans.load().unwrap();
    bans.save().unwrap();
    let mut ip_bans = IpBanList::new(dir.path().join("banned-ips.json"), notifications.clone());
    ip_bans.load().unwrap();
    ip_bans.save().unwrap();
    // the usercache keeps its file order (most recently used first)
    let cache = CachedUserNameToIdResolver::new(Arc::new(NoProfileLookup), dir.path().join("usercache.json"));
    cache_save(&cache);
    for file in ["ops.json", "whitelist.json", "banned-players.json", "banned-ips.json", "usercache.json"] {
        assert_eq!(read(&dir.path().join(file)), read(&fixtures.join(file)), "{file}");
    }
    let user_of = profiles(&fixtures);
    assert_eq!(ops.get_user_list().len(), 17);
    assert!(white.is_white_listed(&user_of("Alice")));
    assert!(bans.is_banned(&user_of("Trent")));
    assert!(ip_bans.is_banned("10.0.0.1"));
}

fn cache_save(cache: &CachedUserNameToIdResolver) {
    use mc_game::server::players::UserNameToIdResolver;
    cache.save();
}
