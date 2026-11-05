//! `mc-command-diff`: runs a command corpus on the real vanilla server and on OUR server and diffs the normalized
//! output. Driven by `scripts/command-diff.sh` (see its header for the corpus format and the normalization list).
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use mc_protocol::codec::{Decode, read_varint};
use mc_protocol::command_tree_diff::{canonical, diff_trees};
use mc_protocol::packets::play::ClientboundCommandsPacket;
use mc_testbot::harness::{OurServer, repo_root};
use mc_testbot::join::BotOptions;
use mc_testbot::log::Dir;
use mc_testbot::Bot;
use regex::Regex;

const BOTS: [&str; 2] = ["Bot", "Bot0"];
const MARKER: &str = "@@END";

type Res<T> = Result<T, String>;

// ---- corpus ---------------------------------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
enum Item {
    Console(String),
    /// `(bot index, command)`
    Player(usize, String),
    Wait(u64),
    Ticks(u64),
    Ignore(String),
    Packets(String),
    Join(usize),
    Leave(usize),
    /// Directory copied to `world/datapacks/<name>` of both servers before they start.
    Datapack(String),
}

#[derive(Debug, Clone)]
struct Line {
    file: String,
    no: usize,
    item: Item,
    text: String,
}

fn parse_corpus(file: &str, text: &str) -> Res<Vec<Line>> {
    let mut out = Vec::new();
    for (i, raw) in text.lines().enumerate() {
        let line = raw.trim_end();
        if line.trim().is_empty() || line.trim_start().starts_with('#') {
            continue;
        }
        let at = |m: String| format!("{file}:{}: {m}", i + 1);
        let item = if let Some(rest) = line.strip_prefix('!') {
            let (word, arg) = rest.split_once(' ').unwrap_or((rest, ""));
            let arg = arg.trim();
            match word {
                "wait" => Item::Wait(arg.parse().map_err(|_| at(format!("!wait needs milliseconds, got `{arg}`")))?),
                "ticks" => Item::Ticks(arg.parse().map_err(|_| at(format!("!ticks needs a tick count, got `{arg}`")))?),
                "ignore" => {
                    Regex::new(arg).map_err(|e| at(format!("bad !ignore regex: {e}")))?;
                    Item::Ignore(arg.to_string())
                }
                "packets" => {
                    Regex::new(arg).map_err(|e| at(format!("bad !packets regex: {e}")))?;
                    Item::Packets(arg.to_string())
                }
                "datapack" => {
                    let dir = Path::new(file).parent().unwrap_or(Path::new(".")).join(arg);
                    if arg.is_empty() || !dir.is_dir() {
                        return Err(at(format!("!datapack needs a directory relative to the corpus file ({} is none)", dir.display())));
                    }
                    Item::Datapack(dir.to_string_lossy().into_owned())
                }
                "join" | "leave" => {
                    let b = BOTS.iter().position(|n| *n == arg).ok_or_else(|| at(format!("!{word} needs Bot or Bot0")))?;
                    if word == "join" { Item::Join(b) } else { Item::Leave(b) }
                }
                _ => return Err(at(format!("unknown directive !{word}"))),
            }
        } else if let Some(rest) = line.strip_prefix("@bot0 ") {
            Item::Player(1, rest.trim_start_matches('/').to_string())
        } else if let Some(rest) = line.strip_prefix("@bot ") {
            Item::Player(0, rest.trim_start_matches('/').to_string())
        } else if line.starts_with('@') {
            return Err(at("unknown `@` prefix (use `@bot ` or `@bot0 `)".into()));
        } else {
            Item::Console(line.trim_start_matches('/').to_string())
        };
        out.push(Line { file: file.rsplit('/').next().unwrap_or(file).to_string(), no: i + 1, item, text: line.to_string() });
    }
    Ok(out)
}

// ---- normalization ----------------------------------------------------------------------------------------------

struct Norm {
    prefix: Regex,
    uuid: Regex,
    uuid_ints: Regex,
    addr: Regex,
    login: Regex,
    dirs: Vec<String>,
    keep_uuids: Vec<String>,
    keep_ints: Vec<String>,
}

impl Norm {
    fn new(dirs: Vec<String>, bot_uuids: &[uuid::Uuid]) -> Self {
        let ints = |u: &uuid::Uuid| {
            let b = u.as_u128();
            format!("[I; {},{},{},{}]", (b >> 96) as u32 as i32, (b >> 64) as u32 as i32, (b >> 32) as u32 as i32, b as u32 as i32)
        };
        Self {
            prefix: Regex::new(r"^\[\d\d:\d\d:\d\d\] \[.*?/([A-Z]+)\]: ").expect("regex"),
            uuid: Regex::new(r"(?i)\b[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}\b").expect("regex"),
            // `UUID: [I; a, b, c, d]` and the other UUID-valued NBT keys, with or without spaces
            uuid_ints: Regex::new(r"((?:UUID|Owner|Thrower|Target|LoveCause|AngryAt|LastHurtBy\w*|Leash|Trusted\w*)\W{1,3}\[I;\s*)(-?\d+),\s*(-?\d+),\s*(-?\d+),\s*(-?\d+)(\s*\])")
                .expect("regex"),
            addr: Regex::new(r"/127\.0\.0\.1:\d+").expect("regex"),
            // entity ids and the random spawn position (vanilla fudges it with the player's own unseeded RNG)
            login: Regex::new(r"(logged in with entity id )\d+( at \().*\)$").expect("regex"),
            dirs,
            keep_uuids: bot_uuids.iter().map(|u| u.to_string()).collect(),
            keep_ints: bot_uuids.iter().map(ints).collect(),
        }
    }

    fn line(&self, raw: &str) -> String {
        let mut s = match self.prefix.captures(raw) {
            Some(c) => {
                let rest = &raw[c.get(0).expect("match").end()..];
                if &c[1] == "INFO" { rest.to_string() } else { format!("{}: {rest}", &c[1]) }
            }
            None => raw.to_string(),
        };
        for d in &self.dirs {
            s = s.replace(d.as_str(), "<DIR>");
        }
        s = self.addr.replace_all(&s, "/127.0.0.1:<PORT>").into_owned();
        s = self.login.replace(&s, "${1}<ID>${2}<POS>)").into_owned();
        s = self.uuid.replace_all(&s, |c: &regex::Captures| if self.keep_uuids.contains(&c[0].to_lowercase()) { c[0].to_string() } else { "<UUID>".into() }).into_owned();
        s = self
            .uuid_ints
            .replace_all(&s, |c: &regex::Captures| {
                let whole = format!("[I; {},{},{},{}]", &c[2], &c[3], &c[4], &c[5]);
                if self.keep_ints.contains(&whole) { c[0].to_string() } else { format!("{}<UUID>{}", &c[1], &c[6]) }
            })
            .into_owned();
        s.trim_end().to_string()
    }
}

// ---- servers ----------------------------------------------------------------------------------------------------

enum Srv {
    Vanilla(mc_testkit::Server),
    Ours(OurServer),
}

impl Srv {
    fn port(&self) -> u16 {
        match self {
            Srv::Vanilla(v) => v.port,
            Srv::Ours(o) => o.port,
        }
    }
    fn cmd(&self, c: &str) {
        match self {
            Srv::Vanilla(v) => v.cmd(c),
            Srv::Ours(o) => o.cmd(c),
        }
    }
    fn log_path(&self) -> PathBuf {
        match self {
            Srv::Vanilla(v) => v.log_path(),
            Srv::Ours(o) => o.log_path(),
        }
    }
    fn dir(&self) -> PathBuf {
        match self {
            Srv::Vanilla(v) => v.dir().to_path_buf(),
            Srv::Ours(o) => o.dir().to_path_buf(),
        }
    }
}

#[derive(Clone)]
struct Opts {
    seed: i64,
    level_type: String,
    gamemode: String,
    keep: bool,
    tree: bool,
    tree_strict: bool,
    out: PathBuf,
    ours_bin: PathBuf,
    run_id: String,
}

const PROPS: &[&str] = &[
    "generate-structures=false",
    "view-distance=4",
    "simulation-distance=4",
    "command-spam-threshold-seconds=100000",
    "chat-spam-threshold-seconds=100000",
    "pause-when-empty-seconds=-1",
    "spawn-npcs=false",
    "spawn-animals=false",
    "spawn-monsters=false",
];

/// A fresh server directory with `ops.json` (Bot at level 4: `op Bot` from the console would ask Mojang's profile
/// service on a vanilla server with internet access and op somebody else's UUID) and the corpus' datapacks.
fn prepare_dir(dir: &Path, datapacks: &[String]) -> std::io::Result<()> {
    let _ = std::fs::remove_dir_all(dir);
    let packs = dir.join("world/datapacks");
    std::fs::create_dir_all(&packs)?;
    let uuid = mc_core::uuid_util::create_offline_player_uuid("Bot");
    std::fs::write(dir.join("ops.json"), format!("[{{\"uuid\":\"{uuid}\",\"name\":\"Bot\",\"level\":4,\"bypassesPlayerLimit\":false}}]\n"))?;
    for d in datapacks {
        let src = Path::new(d);
        copy_dir(src, &packs.join(src.file_name().unwrap_or_default()))?;
    }
    Ok(())
}

fn copy_dir(src: &Path, dst: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for e in std::fs::read_dir(src)? {
        let e = e?;
        let to = dst.join(e.file_name());
        if e.file_type()?.is_dir() {
            copy_dir(&e.path(), &to)?;
        } else {
            std::fs::copy(e.path(), to)?;
        }
    }
    Ok(())
}

fn start_server(ours: bool, o: &Opts, tag: &str, datapacks: &[String]) -> Res<Srv> {
    let level = format!("level-type={}", o.level_type);
    // `MC_CMDDIFF_STRUCTURES=1`: structures on (for `/locate structure` corpora).
    let structures = std::env::var_os("MC_CMDDIFF_STRUCTURES").is_some();
    let mut props: Vec<&str> = PROPS.iter().copied().filter(|p| !(structures && p.starts_with("generate-structures="))).collect();
    props.push(&level);
    if structures {
        props.push("generate-structures=true");
    }
    let prepare = |dir: &Path| prepare_dir(dir, datapacks).map_err(|e| format!("prepare {}: {e}", dir.display()));
    if ours {
        let dir = repo_root().join("run/cmd-diff").join(&o.run_id).join(tag).join("ours");
        prepare(&dir)?;
        OurServer::start_in(&o.ours_bin, o.seed, &o.gamemode, &props, Some(dir), o.keep).map(Srv::Ours).map_err(|e| e.to_string())
    } else {
        let dir = repo_root().join("run/servers").join(format!("cmddiff-{}-{tag}", o.run_id));
        prepare(&dir)?;
        mc_testbot::harness::vanilla(o.seed, &o.gamemode, &props).dir(dir).start().map(Srv::Vanilla).map_err(|e| e.to_string())
    }
}

// ---- one side ---------------------------------------------------------------------------------------------------

#[derive(Debug, Clone, Default)]
struct Seg {
    header: String,
    lines: Vec<String>,
}

struct Mark {
    chat: [usize; 2],
    pkt: [usize; 2],
}

struct Side {
    label: &'static str,
    srv: Srv,
    bots: [Option<Bot>; 2],
    log_off: usize,
    marker: u32,
    frozen: bool,
    ignore: Vec<Regex>,
    packets: Option<Regex>,
    norm: Norm,
    raw_packets: bool,
}

const MARKER_TIMEOUT: Duration = Duration::from_secs(30);

impl Side {
    async fn join_bot(&mut self, i: usize) -> Res<()> {
        let mut o = BotOptions::new("127.0.0.1", self.srv.port(), BOTS[i]);
        o.timeout = Duration::from_secs(120);
        o.record_raw = self.raw_packets;
        let bot = Bot::connect(o).await.map_err(|e| format!("{} {}: {e}", self.label, BOTS[i]))?;
        bot.wait_joined(Duration::from_secs(60)).await.map_err(|e| format!("{} {} login: {e}", self.label, BOTS[i]))?;
        bot.wait_ready(Duration::from_secs(90)).await.map_err(|e| format!("{} {} ready: {e}", self.label, BOTS[i]))?;
        self.bots[i] = Some(bot);
        Ok(())
    }

    fn mark(&self) -> Mark {
        let mut m = Mark { chat: [0; 2], pkt: [0; 2] };
        for (i, b) in self.bots.iter().enumerate() {
            if let Some(b) = b {
                m.chat[i] = b.chat_len();
                m.pkt[i] = b.shared.log.lock().map(|l| l.entries().len()).unwrap_or(0);
            }
        }
        m
    }

    fn log_text(&self) -> String {
        std::fs::read(self.srv.log_path()).map(|b| String::from_utf8_lossy(&b).into_owned()).unwrap_or_default()
    }

    /// Server log (raw lines) since the last marker up to the marker line `n`; advances the read offset past it.
    async fn wait_log_marker(&mut self, n: u32, issuer: Option<usize>) -> Res<String> {
        let tag = format!("{MARKER} {n}");
        let start = Instant::now();
        let mut console_fallback = issuer.is_none();
        loop {
            let text = self.log_text();
            if text.len() >= self.log_off
                && let Some(rel) = text.get(self.log_off..).and_then(|t| t.find(&tag))
            {
                let at = self.log_off + rel;
                let line_start = text[..at].rfind('\n').map_or(0, |p| p + 1);
                let line_end = text[at..].find('\n').map_or(text.len(), |p| at + p + 1);
                let seg = text[self.log_off..line_start].to_string();
                self.log_off = line_end;
                return Ok(seg);
            }
            if !console_fallback
                && start.elapsed() > Duration::from_millis(1500)
                && let Some(i) = issuer
                && self.bots[i].as_ref().is_none_or(|b| b.disconnect_reason().is_some())
            {
                // the issuing bot was kicked by its own command: no chat marker will ever come
                self.srv.cmd(&format!("say {tag}"));
                console_fallback = true;
            }
            if start.elapsed() > MARKER_TIMEOUT {
                return Err(format!("{}: timeout waiting for the log marker `{tag}` (server dead or hung?)", self.label));
            }
            tokio::time::sleep(Duration::from_millis(15)).await;
        }
    }

    fn keep_line(&self, line: &str) -> bool {
        !line.contains(MARKER) && !self.ignore.iter().any(|r| r.is_match(line))
    }

    async fn finish(&mut self, m: Mark, header: String, n: u32, issuer: Option<usize>) -> Res<Seg> {
        let log = self.wait_log_marker(n, issuer).await?;
        let tag = format!("{MARKER} {n}");
        let mut lines: Vec<String> = log.lines().map(|l| self.norm.line(l)).filter(|l| self.keep_line(l)).map(|l| format!("log| {l}")).collect();
        for i in 0..2 {
            let Some(bot) = &self.bots[i] else { continue };
            // the marker is a broadcast chat line: once the bot has it, everything before it arrived
            let _ = bot.wait_for(Duration::from_secs(10), |s| s.chat.iter().skip(m.chat[i]).any(|l| l.text.contains(&tag)).then_some(())).await;
            let chat: Vec<(String, String)> = bot.with_state(|s| {
                let mut chat = Vec::new();
                for l in s.chat.iter().skip(m.chat[i]) {
                    if l.text.contains(MARKER) {
                        if l.text.contains(&tag) {
                            break;
                        }
                        continue;
                    }
                    let who = l.sender.as_deref().map(|n| format!("{n}: ")).unwrap_or_default();
                    let overlay = if l.overlay { " overlay" } else { "" };
                    chat.push((format!("{}| [{}{overlay}] {who}{}", BOTS[i], l.kind, l.text), l.json.clone()));
                }
                chat
            });
            for (text, json) in chat {
                let t = self.norm.line(&text);
                if self.keep_line(&t) {
                    lines.push(t);
                    if !json.is_empty() {
                        lines.push(format!("{}|   json {}", BOTS[i], self.norm.line(&json)));
                    }
                }
            }
            if let Some(re) = &self.packets {
                let entries: Vec<String> = bot
                    .shared
                    .log
                    .lock()
                    .map(|l| {
                        l.entries().iter().skip(m.pkt[i]).filter(|e| e.dir == Dir::In && re.is_match(&e.name)).map(|e| format!("{}| packet {} {}", BOTS[i], e.name, e.debug)).collect()
                    })
                    .unwrap_or_default();
                lines.extend(entries.into_iter().map(|e| self.norm.line(&e)).filter(|l| self.keep_line(l)));
            }
            if let Some(r) = bot.disconnect_reason() {
                lines.push(format!("{}| (disconnected: {})", BOTS[i], self.norm.line(&r)));
                self.bots[i] = None;
            }
        }
        Ok(Seg { header, lines })
    }

    /// Runs one command and returns its output; `None` = only a marker (a sync point).
    async fn issue(&mut self, who: Option<usize>, cmd: Option<&str>, header: String) -> Res<Seg> {
        self.marker += 1;
        let n = self.marker;
        let m = self.mark();
        let tag = format!("{MARKER} {n}");
        match who {
            None => {
                if let Some(c) = cmd {
                    self.srv.cmd(c);
                }
                self.srv.cmd(&format!("say {tag}"));
            }
            Some(i) => {
                let Some(bot) = &self.bots[i] else {
                    return Ok(Seg { header, lines: vec![format!("{}| (not connected: command not sent)", BOTS[i])] });
                };
                if let Some(c) = cmd {
                    bot.command(c);
                }
                // a command, not a chat message: ours (and vanilla) order chat and commands independently
                bot.command(&format!("me {tag}"));
            }
        }
        self.finish(m, header, n, who).await
    }

    async fn gametime(&mut self) -> Res<i64> {
        let seg = self.issue(None, Some("time query gametime"), String::new()).await?;
        let re = Regex::new(r"The time is (-?\d+)").expect("regex");
        seg.lines.iter().find_map(|l| re.captures(l).and_then(|c| c[1].parse().ok())).ok_or_else(|| format!("{}: `time query gametime` gave no time: {:?}", self.label, seg.lines))
    }

    async fn run_item(&mut self, l: &Line) -> Res<Option<Seg>> {
        let header = format!("[{}:{}] {}", l.file, l.no, l.text);
        match &l.item {
            Item::Console(c) => {
                let t = c.trim();
                if t == "tick freeze" || t == "tick freeze true" {
                    self.frozen = true;
                } else if t == "tick unfreeze" || t == "tick freeze false" {
                    self.frozen = false;
                }
                self.issue(None, Some(c), header).await.map(Some)
            }
            Item::Player(i, c) => self.issue(Some(*i), Some(c), header).await.map(Some),
            Item::Wait(ms) => {
                // output arriving meanwhile (a kicked client's disconnect) belongs to the next command's segment
                tokio::time::sleep(Duration::from_millis(*ms)).await;
                Ok(None)
            }
            Item::Ticks(n) => {
                let g0 = self.gametime().await?;
                if self.frozen {
                    self.issue(None, Some(&format!("tick step {n}")), String::new()).await?;
                }
                let deadline = Instant::now() + Duration::from_secs(30) + Duration::from_millis(*n * 50);
                while self.gametime().await? < g0 + *n as i64 {
                    if Instant::now() > deadline {
                        return Err(format!("{}: !ticks {n}: the game time does not advance (frozen without `tick freeze` seen?)", self.label));
                    }
                    tokio::time::sleep(Duration::from_millis(40)).await;
                }
                Ok(None)
            }
            Item::Datapack(_) => Ok(None),
            Item::Ignore(r) => {
                self.ignore.push(Regex::new(r).expect("checked at parse"));
                Ok(None)
            }
            Item::Packets(r) => {
                self.packets = Some(Regex::new(r).expect("checked at parse"));
                Ok(None)
            }
            Item::Join(i) => {
                if let Some(b) = self.bots[*i].take() {
                    b.disconnect().await;
                }
                self.marker += 1;
                let n = self.marker;
                let mut m = self.mark();
                self.join_bot(*i).await?;
                (m.chat[*i], m.pkt[*i]) = (0, 0);
                self.srv.cmd(&format!("say {MARKER} {n}"));
                self.finish(m, header, n, None).await.map(Some)
            }
            Item::Leave(i) => {
                self.marker += 1;
                let n = self.marker;
                let m = self.mark();
                if let Some(b) = self.bots[*i].take() {
                    b.disconnect().await;
                }
                self.srv.cmd(&format!("say {MARKER} {n}"));
                self.finish(m, header, n, None).await.map(Some)
            }
        }
    }

    /// Marker round trip whose output is dropped (also advances the log offset).
    async fn sync(&mut self) -> Res<()> {
        self.issue(None, None, String::new()).await.map(|_| ())
    }
}

struct SideOut {
    segs: Vec<Seg>,
    trees: Vec<Option<ClientboundCommandsPacket>>,
}

async fn run_side(ours: bool, o: &Opts, tag: &str, lines: &[Line]) -> Res<SideOut> {
    let label = if ours { "ours" } else { "vanilla" };
    let (oc, tc) = (o.clone(), tag.to_string());
    let packs: Vec<String> = lines.iter().filter_map(|l| if let Item::Datapack(d) = &l.item { Some(d.clone()) } else { None }).collect();
    let srv = tokio::task::spawn_blocking(move || start_server(ours, &oc, &tc, &packs)).await.map_err(|e| e.to_string())??;
    let uuids: Vec<uuid::Uuid> = BOTS.iter().map(|n| mc_core::uuid_util::create_offline_player_uuid(n)).collect();
    let mut dirs = vec![srv.dir().to_string_lossy().into_owned(), repo_root().to_string_lossy().into_owned()];
    dirs.sort_by_key(|d| std::cmp::Reverse(d.len()));
    let mut side = Side {
        label,
        srv,
        bots: [None, None],
        log_off: 0,
        marker: 0,
        frozen: false,
        ignore: Vec::new(),
        packets: None,
        norm: Norm::new(dirs, &uuids),
        raw_packets: o.tree,
    };
    // Bot is opped through ops.json (prepare_dir) before the server starts: its first `commands` packet is the op view
    side.join_bot(0).await?;
    side.join_bot(1).await?;
    // the spawn position is random (unseeded player RNG) even in vanilla: put both bots at fixed places
    side.srv.cmd("tp Bot 0.5 -60 0.5 0 0");
    side.srv.cmd("tp Bot0 2.5 -60 0.5 0 0");
    side.sync().await?;
    let mut trees = vec![];
    for b in &side.bots {
        trees.push(b.as_ref().and_then(|b| first_commands_packet(b)));
    }
    let mut segs = Vec::new();
    let mut result = Ok(());
    if !lines.is_empty() {
        for l in lines {
            match side.run_item(l).await {
                Ok(Some(seg)) => segs.push(seg),
                Ok(None) => {}
                Err(e) => {
                    result = Err(e);
                    break;
                }
            }
        }
    }
    for b in side.bots.iter().flatten() {
        b.disconnect().await;
    }
    let dir = side.srv.dir();
    drop(side);
    if !o.keep && !ours {
        let _ = std::fs::remove_dir_all(dir);
    }
    result.map(|()| SideOut { segs, trees })
}

/// The first `commands` packet the bot received (op/permission view at join).
fn first_commands_packet(bot: &Bot) -> Option<ClientboundCommandsPacket> {
    let log = bot.shared.log.lock().ok()?;
    let entry = log.entries().iter().find(|e| e.dir == Dir::In && e.name == "commands")?;
    let bytes: Vec<u8> = (0..entry.raw.as_ref()?.len()).step_by(2).map(|i| u8::from_str_radix(&entry.raw.as_ref()?[i..i + 2], 16).ok()).collect::<Option<_>>()?;
    let mut s = bytes.as_slice();
    read_varint(&mut s).ok()?; // packet id
    ClientboundCommandsPacket::decode(&mut s).ok()
}

// ---- diff -------------------------------------------------------------------------------------------------------

/// Line diff (`None` = common): longest common subsequence.
fn lcs_diff<'a>(a: &'a [String], b: &'a [String]) -> Vec<(char, &'a str)> {
    let (n, m) = (a.len(), b.len());
    let mut t = vec![vec![0u32; m + 1]; n + 1];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            t[i][j] = if a[i] == b[j] { t[i + 1][j + 1] + 1 } else { t[i + 1][j].max(t[i][j + 1]) };
        }
    }
    let (mut i, mut j, mut out) = (0, 0, Vec::new());
    while i < n || j < m {
        if i < n && j < m && a[i] == b[j] {
            out.push((' ', a[i].as_str()));
            i += 1;
            j += 1;
        } else if j == m || (i < n && t[i + 1][j] >= t[i][j + 1]) {
            out.push(('-', a[i].as_str()));
            i += 1;
        } else {
            out.push(('+', b[j].as_str()));
            j += 1;
        }
    }
    out
}

fn transcript(segs: &[Seg]) -> String {
    segs.iter().map(|s| format!("### {}\n{}\n", s.header, s.lines.join("\n"))).collect()
}

/// Prints the per-command diff; number of differing commands.
fn report(file: &str, v: &[Seg], o: &[Seg]) -> usize {
    let mut differing = 0;
    for (sv, so) in v.iter().zip(o) {
        if sv.lines == so.lines {
            continue;
        }
        differing += 1;
        println!("=== {}", sv.header);
        println!("--- vanilla / +++ ours");
        for (c, l) in lcs_diff(&sv.lines, &so.lines) {
            println!("{c} {l}");
        }
        println!();
    }
    if v.len() != o.len() {
        println!("{file}: vanilla produced {} outputs, ours {}", v.len(), o.len());
        differing += 1;
    }
    differing
}

fn compare_trees(v: &SideOut, o: &SideOut, strict: bool) -> usize {
    let mut bad = 0;
    for (i, name) in BOTS.iter().enumerate() {
        let what = if i == 0 { "Bot (op, level 4)" } else { "Bot0 (non-op, level 0)" };
        let (Some(a), Some(b)) = (&v.trees[i], &o.trees[i]) else {
            println!("tree {name}: no commands packet captured (vanilla {}, ours {})", v.trees[i].is_some(), o.trees[i].is_some());
            bad += 1;
            continue;
        };
        let r = diff_trees(a, b);
        println!("--- command tree for {what}");
        print!("{}", r.render());
        if !r.missing_in_b.is_empty() {
            println!("not implemented yet: {}", r.missing_in_b.join(" "));
        }
        let mut ok = r.common_equal();
        if strict {
            ok &= r.missing_in_b.is_empty() && canonical(a) == canonical(b);
        }
        if !ok {
            bad += 1;
        }
    }
    bad
}

// ---- main -------------------------------------------------------------------------------------------------------

fn usage() -> ! {
    eprintln!(
        "usage: mc-command-diff [--seed N] [--level-type flat|normal] [--gamemode M] [--keep] [--tree] [--tree-strict]\n       \
         [--out DIR] --ours-bin PATH [CORPUS...]\n(use scripts/command-diff.sh)"
    );
    std::process::exit(2)
}

#[tokio::main]
async fn main() {
    let mut args = std::env::args().skip(1);
    let mut o = Opts {
        seed: 12345,
        level_type: "minecraft:flat".into(),
        gamemode: "creative".into(),
        keep: false,
        tree: false,
        tree_strict: false,
        out: repo_root().join("run/cmd-diff"),
        ours_bin: PathBuf::new(),
        run_id: std::env::var("MC_CMDDIFF_RUN_ID").unwrap_or_else(|_| std::process::id().to_string()),
    };
    let mut files = Vec::new();
    while let Some(a) = args.next() {
        let mut val = || args.next().unwrap_or_else(|| usage());
        match a.as_str() {
            "--seed" => o.seed = val().parse().unwrap_or_else(|_| usage()),
            "--level-type" => {
                let t = val();
                o.level_type = if t.contains(':') { t } else { format!("minecraft:{t}") };
            }
            "--gamemode" => o.gamemode = val(),
            "--keep" => o.keep = true,
            "--tree" => o.tree = true,
            "--tree-strict" => (o.tree, o.tree_strict) = (true, true),
            "--out" => o.out = PathBuf::from(val()),
            "--ours-bin" => o.ours_bin = PathBuf::from(val()),
            f if !f.starts_with("--") => files.push(f.to_string()),
            _ => usage(),
        }
    }
    if o.ours_bin.as_os_str().is_empty() || !o.ours_bin.exists() {
        eprintln!("mc-command-diff: our server binary not found: {}", o.ours_bin.display());
        std::process::exit(2);
    }
    if files.is_empty() && !o.tree {
        usage();
    }
    let mut corpora = Vec::new();
    for f in &files {
        let text = std::fs::read_to_string(f).unwrap_or_else(|e| {
            eprintln!("mc-command-diff: cannot read {f}: {e}");
            std::process::exit(2)
        });
        match parse_corpus(f, &text) {
            Ok(l) => corpora.push((f.clone(), l)),
            Err(e) => {
                eprintln!("mc-command-diff: {e}");
                std::process::exit(2);
            }
        }
    }
    if corpora.is_empty() {
        corpora.push((String::from("(tree only)"), Vec::new()));
    }
    let code = tokio::select! {
        c = run_all(&o, &corpora) => c,
        _ = tokio::signal::ctrl_c() => {
            eprintln!("interrupted");
            130
        }
    };
    std::process::exit(code);
}

async fn run_all(o: &Opts, corpora: &[(String, Vec<Line>)]) -> i32 {
    let mut total_diff = 0;
    let mut total_cmds = 0;
    for (idx, (file, lines)) in corpora.iter().enumerate() {
        let tag = format!("{idx}");
        eprintln!("[command-diff] {file}: {} corpus lines, seed {}, {}", lines.len(), o.seed, o.level_type);
        let (v, ours) = tokio::join!(run_side(false, o, &tag, lines), run_side(true, o, &tag, lines));
        let (v, ours) = match (v, ours) {
            (Ok(v), Ok(ours)) => (v, ours),
            (a, b) => {
                for e in [a.err(), b.err()].into_iter().flatten() {
                    eprintln!("[command-diff] ERROR: {e}");
                }
                return 2;
            }
        };
        let stem = Path::new(file).file_stem().and_then(|s| s.to_str()).unwrap_or("corpus");
        let dir = o.out.join(&o.run_id);
        let _ = std::fs::create_dir_all(&dir);
        let _ = std::fs::write(dir.join(format!("{stem}.vanilla.txt")), transcript(&v.segs));
        let _ = std::fs::write(dir.join(format!("{stem}.ours.txt")), transcript(&ours.segs));
        total_cmds += v.segs.len();
        total_diff += report(file, &v.segs, &ours.segs);
        if o.tree {
            total_diff += compare_trees(&v, &ours, o.tree_strict);
        }
    }
    if total_diff == 0 {
        println!("IDENTICAL ({total_cmds} commands)");
        0
    } else {
        println!("DIFFERENT: {total_diff} difference(s) in {total_cmds} commands (transcripts in {})", o.out.join(&o.run_id).display());
        1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn corpus_parses_every_form() {
        let t = "# c\nsay hi\n/time set day\n@bot give @s stone\n@bot0 /kill\n!wait 5\n!ticks 3\n!ignore ^foo\n!join Bot0\n\n";
        let l = parse_corpus("x.txt", t).unwrap();
        let items: Vec<Item> = l.into_iter().map(|l| l.item).collect();
        assert_eq!(
            items,
            vec![
                Item::Console("say hi".into()),
                Item::Console("time set day".into()),
                Item::Player(0, "give @s stone".into()),
                Item::Player(1, "kill".into()),
                Item::Wait(5),
                Item::Ticks(3),
                Item::Ignore("^foo".into()),
                Item::Join(1),
            ]
        );
        assert!(parse_corpus("x", "!bogus").is_err());
        assert!(parse_corpus("x", "@alice hi").is_err());
    }

    #[test]
    fn normalization() {
        let bot = mc_core::uuid_util::create_offline_player_uuid("Bot");
        let n = Norm::new(vec!["/srv/run".into()], &[bot]);
        assert_eq!(n.line("[12:00:01] [Server thread/INFO]: [Bot: Done]"), "[Bot: Done]");
        assert_eq!(n.line("[12:00:01] [Server thread/WARN]: careful"), "WARN: careful");
        assert_eq!(n.line("x /srv/run/world y"), "x <DIR>/world y");
        assert_eq!(n.line("a/127.0.0.1:5555 b"), "a/127.0.0.1:<PORT> b");
        assert_eq!(n.line("u 123e4567-e89b-12d3-a456-426614174000"), "u <UUID>");
        assert_eq!(n.line(&format!("u {bot}")), format!("u {bot}"));
        assert_eq!(n.line("UUID: [I; 1, 2, 3, 4]"), "UUID: [I; <UUID>]");
        assert_eq!(n.line("Bot[/1.2.3.4:5] logged in with entity id 12 at (0.5, 4.0, 1.5)"), "Bot[/1.2.3.4:5] logged in with entity id <ID> at (<POS>)");
    }

    #[test]
    fn lcs() {
        let a: Vec<String> = ["a", "b", "c"].map(String::from).to_vec();
        let b: Vec<String> = ["a", "x", "c"].map(String::from).to_vec();
        assert_eq!(lcs_diff(&a, &b), vec![(' ', "a"), ('-', "b"), ('+', "x"), (' ', "c")]);
    }
}
