//! Legacy user list conversion: the real server and `mc-server` start in offline mode (`online-mode=false`, so the
//! conversion makes offline UUIDs and asks no profile service) on identical directories with the old files
//! `ops.txt`, `white-list.txt`, `banned-players.txt`, `banned-ips.txt` and `world/players/<name>.dat`, then stop. What
//! they leave is compared exactly: the new lists, the renamed old files (`*.converted`), the moved player files, the
//! user cache (expiry times left out: wall clock) and the warnings and errors of the log.
//!
//! The online-mode variant needs a profile service both servers can be pointed at; ours has no such hook yet.
use std::fs;
use std::path::{Path, PathBuf};

use mc_nbt::CompoundTag;
use mc_nbt::compress::write_compressed;
use worldcmp::fields::{FieldSpec, Ignores};
use worldcmp::tree::{self, TreeOptions, compare_trees};

use crate::world_start::{Run, Side, copy_dir, first_log_difference, run};

/// The world folder of a corpus world of the data fixer tests (extracted into the user cache).
fn corpus_world(id: &str) -> PathBuf {
    let world = Path::new(&std::env::var_os("HOME").expect("HOME"))
        .join(".cache/mc-datafix-corpus/worlds")
        .join(id);
    assert!(
        world.is_dir(),
        "corpus world {id} is not extracted: {}",
        world.display()
    );
    world
}

const OPS: &str = "\
Steve
alex_1
# not a comment: every line of the old op list is a name

Notch
";

const WHITE_LIST: &str = "Steve\nHerobrine\nalex_1\n";

/// `name|created|source|expires|reason`; the old format lower-cases the key, so `Alex_B` converts as `alex_b`.
const BANNED_PLAYERS: &str = "\
# banned players
Alex_B|2013-01-01 00:00:00 +0000|Server|forever|Griefing|more|columns
Steve|2014-02-03 04:05:06 +0000|Console
Notch|2015-06-07 08:09:10 +0000|Console|2099-01-01 00:00:00 +0000|Time out
";

const BANNED_IPS: &str = "\
10.0.0.1|2013-01-01 00:00:00 +0000|Server|forever|Spam
192.168.1.20|2016-01-02 03:04:05 +0000|Console|2099-01-01 00:00:00 +0000|Flood
";

fn player_file(dir: &Path, name: &str) {
    let mut tag = CompoundTag::new();
    tag.put_string("name", name);
    tag.put_int("DataVersion", 5023);
    fs::write(dir.join(name), write_compressed(&tag)).expect("player file");
}

/// The legacy lists: file name and bytes.
type Lists<'a> = &'a [(&'a str, &'a [u8])];

/// The default lists, with the lists of `replaced` (by file name) in place of the default ones.
fn lists_with(replaced: Lists) -> Vec<(String, Vec<u8>)> {
    [
        ("ops.txt", OPS),
        ("white-list.txt", WHITE_LIST),
        ("banned-players.txt", BANNED_PLAYERS),
        ("banned-ips.txt", BANNED_IPS),
    ]
    .into_iter()
    .map(|(name, text)| {
        let text = replaced
            .iter()
            .find(|(n, _)| *n == name)
            .map_or(text.as_bytes(), |(_, text)| text);
        (name.to_owned(), text.to_vec())
    })
    .collect()
}

/// The legacy files, with player files named `players`.
fn prepare(dir: &Path, lists: &[(String, Vec<u8>)], players: &[&str]) {
    for (name, text) in lists {
        fs::write(dir.join(name), text).expect("legacy list");
    }
    let old = dir.join("world/players");
    fs::create_dir_all(&old).expect("players folder");
    for name in players {
        player_file(&old, name);
    }
}

/// Copies what a conversion changes into a directory of its own: the lists, their renamed originals, the user cache and the player folders.
fn view(run: &Run) -> std::path::PathBuf {
    let view = run.dir.join("view");
    fs::create_dir_all(&view).expect("view");
    for entry in fs::read_dir(&run.dir).expect("server dir").flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        let list =
            name.ends_with(".json") || name.ends_with(".converted") || name.ends_with(".txt");
        if list && name != "eula.txt" {
            fs::copy(entry.path(), view.join(&name)).expect("copy list");
        }
    }
    for folder in ["world/players", "unknownplayers", "world/unknownplayers"] {
        let from = run.dir.join(folder);
        if from.is_dir() {
            let to = view.join(folder);
            fs::create_dir_all(to.parent().expect("parent")).expect("view folder");
            let status = std::process::Command::new("cp")
                .arg("-r")
                .arg(&from)
                .arg(&to)
                .status()
                .expect("cp");
            assert!(status.success());
        }
    }
    view
}

/// The log records that are compared: a crash report is saved under a path of the server directory and a time.
fn records(run: &Run) -> Vec<&String> {
    run.log
        .iter()
        .filter(|l| !l.starts_with("ERROR This crash report has been saved to:"))
        .collect()
}

fn compare(vanilla: &Run, ours: &Run, ignored: &[&str]) -> Vec<String> {
    let mut findings = Vec::new();
    if (vanilla.started, vanilla.exit) != (ours.started, ours.exit) {
        findings.push(format!(
            "outcome: vanilla started={} exit={:?}, ours started={} exit={:?}",
            vanilla.started, vanilla.exit, ours.started, ours.exit
        ));
    }
    let (v, o): (Vec<String>, Vec<String>) = (
        records(vanilla).into_iter().cloned().collect(),
        records(ours).into_iter().cloned().collect(),
    );
    if let Some(d) = first_log_difference(&v, &o) {
        findings.push(d);
    }
    let ignores = Ignores(
        // the cache entries expire a month after they were made
        ["usercache.json#*.expiresOn"]
            .iter()
            .chain(ignored)
            .map(|s| s.parse::<FieldSpec>().expect("spec"))
            .collect(),
    );
    let files = compare_trees(
        &view(vanilla),
        &view(ours),
        &TreeOptions {
            ignores,
            chunks: None,
            max_listed: 10,
        },
    );
    if !files.is_clean() {
        findings.push(tree::render_text("vanilla", "ours", &files, 20));
    }
    findings
}

/// What the real server does with the files.
#[derive(Clone, Copy)]
enum Expect {
    /// It starts; `fails`: the conversion of the player files is reported as failed and retried (the lists are
    /// converted and the server starts anyway).
    Converts { fails: bool },
    /// An exception ends the start; `unconverted`: the old lists it did not get to rename.
    Aborts {
        unconverted: &'static [&'static str],
    },
}

/// Both servers on the same files: `ignored` are the fields (`file#path` specs) that hold the time of the run.
fn dual_case(lists: Lists, players: &'static [&'static str], expect: Expect, ignored: &[&str]) {
    dual_on(None, lists, players, expect, ignored);
}

/// [`dual_case`] on a copy of the corpus world `world` (the world folder of the server directory) when there is one.
fn dual_on(
    world: Option<&str>,
    lists: Lists,
    players: &'static [&'static str],
    expect: Expect,
    ignored: &[&str],
) {
    let world = world.map(corpus_world);
    let lists = lists_with(lists);
    let dirs = [
        tempfile::tempdir().expect("temp dir"),
        tempfile::tempdir().expect("temp dir"),
    ];
    let args: [&str; 0] = [];
    let start = |side, dir: &Path| {
        let lists = lists.clone();
        let world = world.clone();
        run(
            side,
            dir,
            move |dir| {
                if let Some(world) = &world {
                    copy_dir(world, &dir.join("world"));
                    let _ = fs::remove_file(dir.join("world/session.lock"));
                }
                prepare(dir, &lists, players);
            },
            &args,
        )
    };
    let vanilla = start(Side::Vanilla, dirs[0].path());
    let ours = start(Side::Ours, dirs[1].path());
    // the comparison must have seen what is expected, not two servers that stopped at the same point
    match expect {
        Expect::Converts { fails } => {
            assert!(vanilla.started, "{:?}", vanilla.log);
            assert!(
                dirs[0].path().join("ops.txt.converted").exists(),
                "{:?}",
                vanilla.log
            );
            let failed = vanilla.log.iter().any(|l| l.contains("Conversion failed"));
            assert_eq!(failed, fails, "{:?}", vanilla.log);
        }
        Expect::Aborts { unconverted } => {
            assert!(!vanilla.started, "{:?}", vanilla.log);
            assert!(
                vanilla
                    .log
                    .iter()
                    .any(|l| l == "ERROR Encountered an unexpected exception"),
                "{:?}",
                vanilla.log
            );
            for name in unconverted {
                assert!(dirs[0].path().join(name).exists(), "{name}");
            }
        }
    }
    let findings = compare(&vanilla, &ours, ignored);
    assert!(findings.is_empty(), "{}", findings.join("\n"));
}

fn dual(players: &'static [&'static str], fails: bool) {
    dual_case(&[], players, Expect::Converts { fails }, &[]);
}

#[test]
#[ignore = "oracle: spawns vanilla server"]
fn offline_lists_and_player_files() {
    dual(
        &["Steve.dat", "alex_1.dat", "Notch.dat", "Mixed_Case.dat"],
        false,
    );
}

/// The player file is looked up as `<name>.dat`: with an upper-case extension the rename fails, the conversion of the player files is reported as failed and retried, and the server starts anyway.
#[test]
#[ignore = "oracle: spawns vanilla server"]
fn player_file_with_an_upper_case_extension() {
    dual(&["Steve.dat", "Herobrine.DAT"], true);
}

const BAN_LIST_WITH_PIPES_ONLY: &str = "\
Alex_B|2013-01-01 00:00:00 +0000|Server|forever|Griefing
||||
Notch|2015-06-07 08:09:10 +0000|Console
";

/// `parts[0]` of a line made of `|` only (`split` leaves no part): the exception is not caught by the conversion, so the start ends and not one old list is converted.
#[test]
#[ignore = "oracle: spawns vanilla server"]
fn pipes_only_line_in_the_old_player_ban_list() {
    dual_case(
        &[("banned-players.txt", BAN_LIST_WITH_PIPES_ONLY.as_bytes())],
        &["Steve.dat"],
        Expect::Aborts {
            unconverted: &[
                "banned-players.txt",
                "banned-ips.txt",
                "ops.txt",
                "white-list.txt",
            ],
        },
        &[],
    );
}

/// The ip list is the second conversion: the player ban list is converted before the exception ends the start.
#[test]
#[ignore = "oracle: spawns vanilla server"]
fn single_pipe_line_in_the_old_ip_ban_list() {
    dual_case(
        &[(
            "banned-ips.txt",
            b"10.0.0.1|2013-01-01 00:00:00 +0000|Server\n  |  \n|\n",
        )],
        &["Steve.dat"],
        Expect::Aborts {
            unconverted: &["banned-ips.txt", "ops.txt", "white-list.txt"],
        },
        &[],
    );
}

/// The op and white lists are not trimmed or split: a line of `|` is a name there.
#[test]
#[ignore = "oracle: spawns vanilla server"]
fn pipe_lines_are_names_in_the_op_and_white_lists() {
    dual_case(
        &[
            ("ops.txt", b"||||\n|\nSteve\n"),
            ("white-list.txt", b"||||\nSteve|x\n"),
        ],
        &["Steve.dat"],
        Expect::Converts { fails: false },
        &[],
    );
}

const ODD_OPS: &str =
    "  Steve  \nalex 1\nthis_name_is_much_longer_than_16\n\n#c\n\u{c4}rger\nSteve\nsteve\n";
const ODD_WHITE_LIST: &str = "Steve\nSTEVE\n\t\nx\n";
const ODD_BANNED_PLAYERS: &str = "\
Steve|2013-01-01 00:00:00 +0000|Server|forever|r
STEVE|2014-01-01 00:00:00 +0000|Other
 \u{c4}rger |2013-01-01 00:00:00 +0000
this_name_is_much_longer_than_16|2013-01-01 00:00:00 +0000|S|forever
x y|2013-01-01 00:00:00 +0000|S|2099-01-01 00:00:00 +0000
";

/// Names with spaces, longer than 16 characters, non-ASCII, repeated in other case, and the white space only name.
#[test]
#[ignore = "oracle: spawns vanilla server"]
fn odd_names_in_all_lists() {
    dual_case(
        &[
            ("ops.txt", ODD_OPS.as_bytes()),
            ("white-list.txt", ODD_WHITE_LIST.as_bytes()),
            ("banned-players.txt", ODD_BANNED_PLAYERS.as_bytes()),
        ],
        &["Steve.dat"],
        Expect::Converts { fails: false },
        &[],
    );
}

/// Numbers without padding, a zone name, and expiry columns that are not dates (no expiry) or empty.
const SHORT_DATES: &str = "\
a|2013-1-1 0:0:0 +0000|S|2013-1-1 0:0:0 +0000|r
e|2013-01-01 00:00:00 +0000|S|2013-01-01 00:00:00 UTC|r
f|2013-01-01 00:00:00 -0830|S||r
g|2013-01-01 00:00:00 +0000|||
h|2013-01-01 00:00:00 +0000|S|not a date|r
";

#[test]
#[ignore = "oracle: spawns vanilla server"]
fn short_and_unparsable_expiry_dates() {
    dual_case(
        &[("banned-players.txt", SHORT_DATES.as_bytes())],
        &["Steve.dat"],
        Expect::Converts { fails: false },
        &[],
    );
}

/// An unparsable creation date is the time of the conversion.
#[test]
#[ignore = "oracle: spawns vanilla server"]
fn unparsable_creation_dates() {
    dual_case(
        &[
            (
                "banned-players.txt",
                b"a|bad date|S|forever|r\nb|2013-01-01\nc|\n",
            ),
            (
                "banned-ips.txt",
                b"10.0.0.1|bad date|S\nnot-an-ip|2013-01-01 00:00:00 +0000|S|forever|r\n::1\n",
            ),
        ],
        &["Steve.dat"],
        Expect::Converts { fails: false },
        &["banned-players.json#*.created", "banned-ips.json#*.created"],
    );
}

/// `SimpleDateFormat` is lenient: a field past its range rolls over into the next one (month 13, hour 25, day 0), a
/// number may carry a sign or any Unicode digit, and a field of the date may be missing its leading zeros.
#[test]
#[ignore = "oracle: spawns vanilla server"]
fn dates_with_fields_out_of_range() {
    dual_case(
        &[
            (
                "banned-players.txt",
                "Alex_B|2013-02-30 25:61:61 +0100|Server|2013-13-32 00:00:00 +0000|Griefing\n\
                 Steve|2013-00-00 24:60:60 +0000|Console|2014-1-3 4:5:6 +0000|x\n\
                 Notch|0000-01-01 00:00:00 +0000|C|-1-01-01 00:00:00 +0000|y\n\
                 Dan|\u{ff12}\u{ff10}13-02-03 04:05:06 +0100|C|2013-02-03 -1:-1:-1 +0100|z\n\
                 Eve|2013E0-02-03 04:05:06 +0100|C|2013-02-03.9 04:05:06 +0100|w\n"
                    .as_bytes(),
            ),
            (
                "banned-ips.txt",
                b"10.0.0.1|2013-02-30 25:61:61 +0100|S|2013-13-32 00:00:00 +0000|r\n",
            ),
        ],
        &["Steve.dat"],
        Expect::Converts { fails: false },
        &[],
    );
}

/// The date is parsed from the start of the text: what follows the zone is ignored, and a zone is `+hhmm`, `GMT` or
/// `GMT+hh:mm` (not `+hh`, not `GMT+hhmm`; those expiry dates do not parse).
#[test]
#[ignore = "oracle: spawns vanilla server"]
fn dates_with_text_after_the_zone() {
    dual_case(
        &[(
            "banned-players.txt",
            b"Alex_B|2013-02-03 04:05:06 +0100 trailing|Server|2014-05-06 07:08:09 +0100x|r\n\
              Steve|2013-02-03 04:05:06 +01001|C|2015-01-01 00:00:00 GMT+02:00 more|r\n\
              Notch|  2013-02-03 04:05:06  +0100|C|2013-02-03 04:05:06 +01|r\n\
              Dan|2013-02-03 04:05:06 GMT|C|2013-02-03 04:05:06 GMT+0200|r\n\
              Eve|2013-02-03 04:05:06 +0100|C|2013-02-03 04:05:06 +2400|r\n\
              Fay|2013-02-03 04:05:06 gmt-05:30|C|forever|r\n",
        )],
        &["Steve.dat"],
        Expect::Converts { fails: false },
        &[],
    );
}

/// A parsed `UTC` zone name leaves its zone in the shared format: every date it writes afterwards is in UTC.
#[test]
#[ignore = "oracle: spawns vanilla server"]
fn a_utc_zone_name_changes_the_zone_of_the_written_dates() {
    dual_case(
        &[
            (
                "banned-players.txt",
                b"Alex_B|2013-02-03 04:05:06 +0100|Server|2014-01-01 00:00:00 +0300|r\n\
                  Steve|2013-02-03 04:05:06 UTC|C|2014-01-01 00:00:00 utc|r\n",
            ),
            (
                "banned-ips.txt",
                b"10.0.0.1|2013-02-03 04:05:06 +0100|S|2014-01-01 00:00:00 +0300|r\n",
            ),
        ],
        &["Steve.dat"],
        Expect::Converts { fails: false },
        &[],
    );
}

/// A zone NAME places the local time by the history of the zone (New York was five hours behind in 1900, 1945 and 2040
/// alike, the daylight name of a zone adds the saving of the zone, which is none before it had daylight time), and the
/// zone stays in the shared format: every date it writes afterwards is in that zone, in its offset at that date. The
/// names are the ones of the `ROOT` locale (`New York (+0)`, `Kyiv (+1)`, `EET`).
#[test]
#[ignore = "oracle: spawns vanilla server"]
fn a_named_zone_changes_the_zone_of_the_written_dates() {
    dual_case(
        &[
            (
                "banned-players.txt",
                b"Alex_B|2013-07-03 04:05:06 New York (+0)|Server|2013-07-03 04:05:06 New York (+1)|r\n\
                  Steve|1900-01-01 00:00:00 +0100|C|1945-08-15 03:30:00 +0100|r\n\
                  Notch|2040-07-01 12:00:00 +0000|C|2040-11-04 01:30:00 +0000|r\n\
                  Dan|2013-02-03 04:05:06 Los Angeles (+0)|C|2007-03-11 02:30:00 Los Angeles (+1)|r\n\
                  Eve|1987-07-01 12:00:00 Belize (+0)|C|2013-07-03 04:05:06 Berlin (+0)|r\n\
                  Fay|2013-07-03 04:05:06 Berlin (+1)|C|2013-02-03 04:05:06 Tokyo (+0)|r\n\
                  Gus|2024-10-27 01:30:00 London (+1)|C|2024-03-31 02:30:00 Kyiv (+1)|r\n\
                  Hal|2024-10-27 01:30:00 EET|C|2024-03-31 02:30:00 EEST|r\n\
                  Ian|1850-06-15 12:00:00 Winnipeg (+1)|C|2100-03-14 02:30:00 Winnipeg (+0)|r\n",
            ),
            (
                "banned-ips.txt",
                b"10.0.0.1|2013-02-03 04:05:06 +0100|S|2013-07-03 04:05:06 +0100|r\n",
            ),
        ],
        &["Steve.dat"],
        Expect::Converts { fails: false },
        &[],
    );
}

/// The zone left by the last parse is the one the later lists write in; the lists are converted in the order user bans,
/// ip bans, so the ip bans are written in the zone of the last user ban.
#[test]
#[ignore = "oracle: spawns vanilla server"]
fn the_last_zone_name_decides_the_zone_of_the_next_list() {
    dual_case(
        &[
            (
                "banned-players.txt",
                b"Alex_B|2013-07-03 04:05:06 Tokyo (+0)|Server|2013-07-03 04:05:06 Tokyo (+1)|r\n\
                  Steve|2013-07-03 04:05:06 Chicago (+1)|C|2013-02-03 04:05:06 Chicago (+0)|r\n",
            ),
            (
                "banned-ips.txt",
                b"10.0.0.1|2013-02-03 04:05:06 +0100|S|2013-07-03 04:05:06 +0100|r\n\
                  10.0.0.2|2013-02-03 04:05:06 +0100|S|forever|r\n",
            ),
        ],
        &["Steve.dat"],
        Expect::Converts { fails: false },
        &[],
    );
}

/// A date past the range a `Date` has in `java.time` stays a `long` of milliseconds: a year of nine digits, a year that
/// wraps the millisecond count, and a negative one are all written back.
#[test]
#[ignore = "oracle: spawns vanilla server"]
fn dates_beyond_the_range_of_a_calendar_library() {
    dual_case(
        &[
            (
                "banned-players.txt",
                b"Alex_B|300000-01-01 00:00:00 +0000|Server|-300000-01-01 00:00:00 +0000|r\n\
                  Steve|292278994-08-17 07:12:55 +0000|C|292278995-01-01 00:00:00 +0000|r\n\
                  Notch|999999999-12-31 23:59:59 +0100|C|2147483647-01-01 00:00:00 +0000|r\n\
                  Dan|9223372036854775807-01-01 00:00:00 +0000|C|1E12-01-01 00:00:00 +0000|r\n\
                  Eve|-292275055-05-16 16:47:04 +0000|C|-1E9-01-01 00:00:00 +0000|r\n",
            ),
            (
                "banned-ips.txt",
                b"10.0.0.1|262143-01-01 00:00:00 +0100|S|262142-12-31 23:59:59 -0500|r\n",
            ),
        ],
        &["Steve.dat"],
        Expect::Converts { fails: false },
        &[],
    );
}

/// The same dates through the rules of a zone with daylight time: the transitions of the last year of the `long` range
/// wrap, and the dates the zone leaves in the shared format are written back in it.
#[test]
#[ignore = "oracle: spawns vanilla server"]
fn dates_beyond_the_range_in_a_named_zone() {
    dual_case(
        &[
            (
                "banned-players.txt",
                b"Alex_B|292278994-01-01 00:00:00 New York (+0)|Server|292278994-08-17 07:00:00 New York (+1)|r\n\
                  Steve|-292275055-05-16 16:47:04 Sydney (+0)|C|292278994-12-31 23:59:59 Sydney (+1)|r\n\
                  Notch|262143-07-01 12:00:00 Kyiv (+0)|C|300000-03-15 02:30:00 Kyiv (+1)|r\n\
                  Dan|292278994-08-17 07:12:55 +0000|C|-292275055-01-01 00:00:00 +0000|r\n",
            ),
            (
                "banned-ips.txt",
                b"10.0.0.1|292278994-08-17 07:12:55 +0000|S|-292275055-05-16 16:47:04 +0000|r\n",
            ),
        ],
        &["Steve.dat"],
        Expect::Converts { fails: false },
        &[],
    );
}

/// A zone keeps the transitions it computed last (`SimpleTimeZone`'s cache): a date in the last year of the `long`
/// range makes the cached range of a southern zone cover almost every date after it, so a later date of that zone is
/// read and written in daylight time although it is in winter.
#[test]
#[ignore = "oracle: spawns vanilla server"]
fn a_date_at_the_end_of_the_range_changes_later_dates_of_the_zone() {
    dual_case(
        &[
            (
                "banned-players.txt",
                b"Steve|292278994-01-01 00:00:00 Sydney (+0)|C|2150-07-01 12:00:00 Sydney (+0)|r\n\
                  Alex_B|292278994-08-17 07:12:55 Auckland (+1)|Server|2150-07-01 12:00:00 Auckland (+0)|r\n",
            ),
            (
                "banned-ips.txt",
                b"10.0.0.1|292278994-01-01 00:00:00 +0000|S|2150-07-01 12:00:00 +0000|r\n",
            ),
        ],
        &["Steve.dat"],
        Expect::Converts { fails: false },
        &[],
    );
}

/// Before 1582-10-15 the calendar is the Julian one: 1500-01-01 is ten days later than the Gregorian date of the same
/// instant, the ten days after 1582-10-04 do not exist (the later date is kept), and 1000-02-29 is a leap day.
#[test]
#[ignore = "oracle: spawns vanilla server"]
fn dates_before_the_gregorian_cutover() {
    dual_case(
        &[(
            "banned-players.txt",
            b"Alex_B|1500-01-01 00:00:00 +0000|S|1582-10-10 12:00:00 +0000|r\n\
              Steve|1582-10-04 23:59:59 +0000|C|1582-10-15 00:00:00 +0000|r\n\
              Notch|1000-02-29 00:00:00 +0000|C|0001-01-01 00:00:00 +0000|r\n\
              Dan|1582-13-01 00:00:00 +0000|C|1581-13-01 00:00:00 +0000|r\n",
        )],
        &["Steve.dat"],
        Expect::Converts { fails: false },
        &[],
    );
}

/// `trim` removes every character up to U+0020 and nothing above (not U+00A0); `readLines` ends a line at a lone CR.
#[test]
#[ignore = "oracle: spawns vanilla server"]
fn java_trim_and_line_ends() {
    dual_case(
        &[
            ("ops.txt", "\u{a0}Alex\u{a0}\n\u{1}Bob\u{1}\nCar\rDan\n".as_bytes()),
            (
                "banned-players.txt",
                "\u{a0}Alex\u{a0}|2013-01-01 00:00:00 +0000|S\n\u{1}Bob\u{1}|2013-01-01 00:00:00 +0000|S\n\u{1}#hidden\nCar|2013-01-01 00:00:00 +0000|S\rDan|2013-01-01 00:00:00 +0000|S\n".as_bytes(),
            ),
        ],
        &["Steve.dat"],
        Expect::Converts { fails: false },
        &[],
    );
}

/// `equalsIgnoreCase` finds the file of a profile: two files whose names differ only in the case of a non-ASCII letter are one profile for it, the first is moved twice and the second move fails.
#[test]
#[ignore = "oracle: spawns vanilla server"]
fn player_files_differing_in_non_ascii_case() {
    dual(&["\u{c4}rger.dat", "\u{e4}rger.dat", "Steve.dat"], true);
}

/// Characters whose `Character.toUpperCase` is a one-character mapping that differs from the full case mapping
/// (`U+1FB3` becomes `U+1FBC`; `U+0130` lower-cases to `i`): their files are found for the profile names that differ in
/// that case only.
#[test]
#[ignore = "oracle: spawns vanilla server"]
fn player_files_differing_in_one_character_case_mappings() {
    dual(
        &[
            "\u{1fb3}.dat",
            "\u{1fbc}.dat",
            "\u{130}.dat",
            "I.dat",
            "Steve.dat",
        ],
        true,
    );
}

/// A world from before the file fixer's version is moved into a new folder before the server starts: the files of
/// `players/` are listed in the order the new folder was filled in (the order of a `HashMap` of the names), which
/// is the order the player files are converted in, so the order of the user cache.
#[test]
#[ignore = "oracle: spawns vanilla server"]
fn player_files_of_a_world_that_needs_file_fixing() {
    dual_on(
        Some("1.7.10"),
        &[],
        &["Steve.dat", "alex_1.dat", "Notch.dat"],
        Expect::Converts { fails: false },
        &[],
    );
}
