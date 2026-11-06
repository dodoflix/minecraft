//! `mc-testbot` command line.
//!
//! ```text
//! mc-testbot status HOST[:PORT]
//! mc-testbot join HOST[:PORT] [--name Bot] [--seconds 30] [--walk] [--chat TEXT] [--command CMD]... [--log FILE.jsonl] [--text]
//! mc-testbot run HOST[:PORT] SCENARIO.scn [--name Bot] [--log FILE.jsonl] [--obs FILE.txt]   (see src/scenario.rs for the scenario commands)
//! mc-testbot diff A.jsonl B.jsonl [--ignore NAME]... [--state play] [--dir in|out] [--all-fields] [--field NAME=FIELD,FIELD]...
//! mc-testbot text FILE.jsonl
//! ```
use std::sync::{Arc, Mutex};
use std::time::Duration;

use mc_testbot::Bot;
use mc_testbot::join::{BotOptions, status_ping};
use mc_testbot::log::{DiffOptions, Dir, PacketLog, diff_logs, read_jsonl};

fn usage() -> ! {
    eprintln!(
        "usage:\n  mc-testbot run HOST[:PORT] SCENARIO.scn [--name N] [--log F] [--obs F]\n  mc-testbot status HOST[:PORT]\n  mc-testbot join HOST[:PORT] [--name N] [--seconds S] [--walk] [--chat T] [--command C]... [--log F] [--text]\n  \
         mc-testbot diff A.jsonl B.jsonl [--ignore NAME]... [--state S] [--dir in|out] [--all-fields] [--field NAME=F1,F2]...\n  mc-testbot text F.jsonl"
    );
    std::process::exit(2)
}

fn host_port(s: &str) -> (String, u16) {
    match s.rsplit_once(':') {
        Some((h, p)) if p.parse::<u16>().is_ok() => (h.to_string(), p.parse().expect("checked")),
        _ => (s.to_string(), 25565),
    }
}

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(cmd) = args.first() else { usage() };
    let code = match cmd.as_str() {
        "status" => status(&args[1..]).await,
        "join" => join(&args[1..]).await,
        "run" => run(&args[1..]).await,
        "diff" => diff(&args[1..]),
        "text" => {
            let Some(f) = args.get(1) else { usage() };
            for e in read_jsonl(f).unwrap_or_else(|e| fatal(&e.to_string())) {
                println!("{}", e.line());
            }
            0
        }
        _ => usage(),
    };
    std::process::exit(code);
}

fn fatal(msg: &str) -> ! {
    eprintln!("error: {msg}");
    std::process::exit(1)
}

async fn status(args: &[String]) -> i32 {
    let Some(target) = args.first() else { usage() };
    let (host, port) = host_port(target);
    let opts = BotOptions::new(host, port, "Bot");
    match status_ping(&opts, Arc::new(Mutex::new(PacketLog::new()))).await {
        Ok(r) => {
            println!("{}", r.status.to_json_string().unwrap());
            println!("latency: {} ms", r.latency.as_millis());
            0
        }
        Err(e) => fatal(&e.to_string()),
    }
}

async fn join(args: &[String]) -> i32 {
    let Some(target) = args.first() else { usage() };
    let (host, port) = host_port(target);
    let (mut name, mut seconds, mut walk, mut chat, mut log, mut text) =
        ("Bot".to_string(), 30u64, false, None, None, false);
    let mut commands = Vec::new();
    let mut it = args[1..].iter();
    while let Some(a) = it.next() {
        let mut val = || it.next().cloned().unwrap_or_else(|| usage());
        match a.as_str() {
            "--name" => name = val(),
            "--seconds" => seconds = val().parse().unwrap_or_else(|_| usage()),
            "--walk" => walk = true,
            "--chat" => chat = Some(val()),
            "--command" => commands.push(val()),
            "--log" => log = Some(val()),
            "--text" => text = true,
            _ => usage(),
        }
    }
    let opts = BotOptions::new(host, port, name);
    let bot = Bot::connect(opts)
        .await
        .unwrap_or_else(|e| fatal(&e.to_string()));
    let result = async {
        bot.wait_joined(Duration::from_secs(60)).await?;
        bot.wait_ready(Duration::from_secs(90)).await?;
        println!("joined at {:?}", bot.position());
        if let Some(c) = &chat {
            bot.chat(c);
        }
        for c in &commands {
            bot.command(c);
        }
        let start = bot.position();
        let end = std::time::Instant::now() + Duration::from_secs(seconds);
        while std::time::Instant::now() < end {
            if walk {
                for (dx, dz) in [(6.0, 0.0), (6.0, 6.0), (0.0, 6.0), (0.0, 0.0)] {
                    let _ = bot
                        .walk_to(
                            start.x + dx,
                            start.z + dz,
                            0.6,
                            false,
                            Duration::from_secs(10),
                        )
                        .await;
                    bot.assert_connected()?;
                }
            } else {
                tokio::time::sleep(Duration::from_millis(200)).await;
                bot.assert_connected()?;
            }
        }
        Ok::<(), mc_testbot::BotError>(())
    }
    .await;
    bot.with_state(|s| {
        for l in &s.chat {
            println!("[{}] {}", l.kind, l.text);
        }
        println!(
            "health {} food {} chunks {} entities {}",
            s.health,
            s.food,
            s.world.chunk_count(),
            s.entities.len()
        );
    });
    if let Some(f) = &log
        && let Err(e) = bot.write_log(f)
    {
        eprintln!("cannot write log: {e}");
    }
    if text {
        print!("{}", bot.log_text());
    }
    bot.disconnect().await;
    match result {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("failed: {e}");
            1
        }
    }
}

/// `run HOST SCENARIO [--name N] [--log F] [--obs F]`: join, run the scenario file, print/write the observation lines.
async fn run(args: &[String]) -> i32 {
    let (Some(target), Some(file)) = (args.first(), args.get(1)) else {
        usage()
    };
    let (host, port) = host_port(target);
    let (mut name, mut log, mut obs) = ("Bot".to_string(), None, None);
    let mut it = args[2..].iter();
    while let Some(a) = it.next() {
        let mut val = || it.next().cloned().unwrap_or_else(|| usage());
        match a.as_str() {
            "--name" => name = val(),
            "--log" => log = Some(val()),
            "--obs" => obs = Some(val()),
            _ => usage(),
        }
    }
    let text = std::fs::read_to_string(file).unwrap_or_else(|e| fatal(&format!("{file}: {e}")));
    let bot = Bot::connect(BotOptions::new(host, port, name))
        .await
        .unwrap_or_else(|e| fatal(&e.to_string()));
    let result = async {
        bot.wait_joined(Duration::from_secs(60)).await?;
        bot.wait_ready(Duration::from_secs(90)).await?;
        mc_testbot::scenario::run(&bot, &text).await
    }
    .await;
    if let Some(f) = &log
        && let Err(e) = bot.write_log(f)
    {
        eprintln!("cannot write log: {e}");
    }
    bot.disconnect().await;
    match result {
        Ok(lines) => {
            let body = lines.join("\n") + "\n";
            match obs {
                Some(f) => std::fs::write(&f, body).unwrap_or_else(|e| fatal(&format!("{f}: {e}"))),
                None => print!("{body}"),
            }
            0
        }
        Err(e) => {
            eprintln!("failed: {e}");
            1
        }
    }
}

fn diff(args: &[String]) -> i32 {
    let (Some(a), Some(b)) = (args.first(), args.get(1)) else {
        usage()
    };
    let mut opts = DiffOptions::default();
    let mut it = args[2..].iter();
    while let Some(arg) = it.next() {
        let mut val = || it.next().cloned().unwrap_or_else(|| usage());
        match arg.as_str() {
            "--ignore" => opts.ignore.push(val()),
            "--state" => opts.state = Some(val()),
            "--dir" => {
                opts.dir = Some(match val().as_str() {
                    "in" => Dir::In,
                    "out" => Dir::Out,
                    _ => usage(),
                })
            }
            "--all-fields" => opts.compare_all_fields = true,
            "--field" => {
                let v = val();
                let Some((n, f)) = v.split_once('=') else {
                    usage()
                };
                opts.fields
                    .entry(n.to_string())
                    .or_insert_with(Vec::new)
                    .extend(f.split(',').map(str::to_string));
            }
            _ => usage(),
        }
    }
    let (la, lb) = (
        read_jsonl(a).unwrap_or_else(|e| fatal(&e.to_string())),
        read_jsonl(b).unwrap_or_else(|e| fatal(&e.to_string())),
    );
    let d = diff_logs(&la, &lb, &opts);
    print!("{}", d.render());
    if d.is_empty() {
        println!("logs are structurally equal ({} packets)", d.a.len());
        0
    } else {
        1
    }
}
