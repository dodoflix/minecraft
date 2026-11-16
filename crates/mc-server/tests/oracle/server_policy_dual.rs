//! The login policies of `PlayerList.canPlayerLogin` and the connection policies around them, observed through the same
//! bot script on the vanilla server and on OUR server: the refusal text the client receives and the log lines for an
//! expiring ban (player and address), a lapsed ban, the whitelist with an unlisted operator, a full server with and
//! without `bypassesPlayerLimit` (also for a duplicate login), an unanswered keep-alive, a silent client's read timeout
//! and `rate-limit`. Permanent
//! bans, address bans, the whitelist commands, the duplicate login and the idle timeout are in `kick_dual`.
//!
//! `server_policy_dual_vanilla` runs the scenarios against the real vanilla server and checks them against the
//! committed observations in `tests/fixtures/server_policy_dual/*.txt` (`MC_UPDATE_FIXTURES=1` rewrites them;
//! `SERVER_POLICY_DUAL_ONLY=name` runs one scenario). `server_policy_dual_ours` runs them against `mc-server`.
//!
//! The ban lists and operator list are written before the first launch: no command sets an expiry date. The date a
//! ban expires on is masked (its zone name and time of day depend on the machine; `format_ban_date` has its own
//! golden test); a ban refusal still shows that the expiration text follows the reason.
use std::path::Path;
use std::time::Duration;

use crate::common::dual::PROPS;
use crate::kick_dual::{Case, Rec, SETTLE, Silent, T, case, compare_ours, record_vanilla};
use mc_core::uuid_util::create_offline_player_uuid;
use mc_testkit::Server;

const DIR: &str = "server_policy_dual";
const ONLY: &str = "SERVER_POLICY_DUAL_ONLY";

const CASES: &[Case] = &[
    case!(ban_expiring, PROPS, 1, Some(prepare_bans)),
    case!(ban_ip_expiring, PROPS, 1, Some(prepare_ip_ban)),
    case!(
        whitelist_listed_and_ops,
        &["white-list=true"],
        1,
        Some(prepare_whitelist)
    ),
    case!(server_full, &["max-players=1"], 1, Some(prepare_operators)),
    case!(keep_alive_timeout, &["difficulty=peaceful"]),
    // the read timeout and the keep-alive timeout start from almost the same instant: a tie is retried
    case!(read_timeout, &["difficulty=peaceful"], 3),
    case!(rate_limit, &["rate-limit=5"]),
];

pub(crate) fn write(dir: &Path, file: &str, entries: &[String]) {
    std::fs::write(dir.join(file), format!("[{}]", entries.join(","))).expect("write the list");
}

fn ban(name: &str, expires: &str, reason: &str) -> String {
    format!(
        r#"{{"uuid":"{}","name":"{name}","created":"2020-01-01 00:00:00 +0000","source":"Console","expires":"{expires}","reason":"{reason}"}}"#,
        create_offline_player_uuid(name)
    )
}

fn user(name: &str) -> String {
    format!(
        r#"{{"uuid":"{}","name":"{name}"}}"#,
        create_offline_player_uuid(name)
    )
}

pub(crate) fn operator(name: &str, bypasses: bool) -> String {
    format!(
        r#"{{"uuid":"{}","name":"{name}","level":4,"bypassesPlayerLimit":{bypasses}}}"#,
        create_offline_player_uuid(name)
    )
}

fn prepare_bans(dir: &Path) {
    write(
        dir,
        "banned-players.json",
        &[
            ban("Victim", "2099-12-31 23:59:59 +0000", "griefing"),
            ban("Forever", "forever", "Banned by an operator."),
            ban("Lapsed", "2021-01-01 00:00:00 +0000", "an old offence"),
        ],
    );
}

fn prepare_ip_ban(dir: &Path) {
    write(
        dir,
        "banned-ips.json",
        &[
            r#"{"ip":"127.0.0.1","created":"2020-01-01 00:00:00 +0000","source":"Console","expires":"2099-12-31 23:59:59 +0000","reason":"no more"}"#
                .to_string(),
        ],
    );
}

fn prepare_whitelist(dir: &Path) {
    write(dir, "whitelist.json", &[user("Listed")]);
    write(dir, "ops.json", &[operator("Operator", false)]);
}

fn prepare_operators(dir: &Path) {
    write(
        dir,
        "ops.json",
        &[operator("Operator", false), operator("Bypasser", true)],
    );
}

/// The expiry date of a ban refusal is replaced: see the module documentation.
fn mask_date(mut r: Rec<'_>) -> Vec<String> {
    for line in &mut r.out {
        if let Some(at) = line.find("removed on ") {
            line.truncate(at + "removed on ".len());
            line.push_str("<date>");
        }
    }
    r.out
}

async fn ban_expiring(s: &Server) -> Vec<String> {
    let mut r = Rec::new(s);
    for name in ["Victim", "Forever", "Lapsed"] {
        if let Some(client) = r.try_join(name).await {
            client.bot.disconnect().await;
            tokio::time::sleep(SETTLE).await;
            r.log();
        }
    }
    mask_date(r)
}

async fn ban_ip_expiring(s: &Server) -> Vec<String> {
    let mut r = Rec::new(s);
    r.try_join("Victim").await;
    mask_date(r)
}

async fn whitelist_listed_and_ops(s: &Server) -> Vec<String> {
    let mut r = Rec::new(s);
    r.try_join("Stranger").await;
    let listed = r.try_join("Listed").await.expect("a listed player joins");
    let operator = r
        .try_join("Operator")
        .await
        .expect("an operator joins without being listed");
    r.settle(&[], &[&listed, &operator], Duration::ZERO).await;
    r.out
}

async fn server_full(s: &Server) -> Vec<String> {
    let mut r = Rec::new(s);
    let first = r.try_join("First").await.expect("the first player joins");
    r.try_join("Second").await;
    r.step("an operator without bypassesPlayerLimit");
    r.try_join("Operator").await;
    // `canPlayerLogin` runs before the duplicate check: a full server refuses a second login of a listed player
    r.step("the first player again");
    r.try_join("First").await;
    let bypasser = r.try_join("Bypasser").await.expect("a bypasser joins");
    r.settle(&[], &[&first, &bypasser], Duration::ZERO).await;
    r.step("the bypasser logs in a second time");
    let mut again = r.try_join("Bypasser").await.expect("a bypasser joins");
    again.name = "Bypasser2";
    r.settle(&[&bypasser], &[&first, &bypasser, &again], T)
        .await;
    r.out
}

/// A client that keeps sending but never answers the keep-alive sent after 15 s: `disconnect.timeout` 15 s later.
async fn keep_alive_timeout(s: &Server) -> Vec<String> {
    let mut r = Rec::new(s);
    r.ordered = true;
    r.drop_leave_lines = true;
    let client = Silent::join_pinging(s, "Bot").await;
    r.log();
    r.step("never answer the keep-alive");
    client.report(&mut r).await;
    r.out
}

/// A client that sends nothing at all: the 30 second read timeout closes the connection without a disconnect packet.
async fn read_timeout(s: &Server) -> Vec<String> {
    let mut r = Rec::new(s);
    r.ordered = true;
    r.drop_leave_lines = true;
    let client = Silent::join(s, "Bot").await;
    r.log();
    r.step("send nothing");
    client.report(&mut r).await;
    r.out
}

async fn rate_limit(s: &Server) -> Vec<String> {
    let mut r = Rec::new(s);
    let bot = r.try_join("Bot").await;
    if let Some(bot) = bot {
        r.step("wait for the first packet-rate check");
        r.settle(&[&bot], &[&bot], T).await;
    }
    r.out
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "oracle: starts the vanilla server"]
async fn server_policy_dual_vanilla() {
    record_vanilla(DIR, ONLY, CASES).await;
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "oracle: starts the mc-server binary"]
async fn server_policy_dual_ours() {
    compare_ours(DIR, ONLY, CASES).await;
}
