//! `mc-command-diff`: command-line front end of [`mc_testbot::corpus`], driven by `scripts/command-diff.sh`.
#![expect(
    clippy::print_stderr,
    reason = "command-line tool: its output is the product"
)]
use std::path::PathBuf;

use mc_testbot::corpus::{Corpus, Mode, Opts, run_all};

fn usage() -> ! {
    eprintln!(
        "usage: mc-command-diff [--seed N] [--level-type flat|normal] [--gamemode M] [--keep] [--tree] [--tree-strict]\n       \
         [--out DIR] [--record DIR | --golden DIR] --ours-bin PATH [CORPUS...]\n(use scripts/command-diff.sh)"
    );
    std::process::exit(2)
}

#[tokio::main]
async fn main() {
    let mut args = std::env::args().skip(1);
    let mut o = Opts::new(PathBuf::new());
    let mut mode = Mode::Both;
    let mut files = Vec::new();
    while let Some(a) = args.next() {
        let mut val = || args.next().unwrap_or_else(|| usage());
        match a.as_str() {
            "--seed" => o.seed = val().parse().unwrap_or_else(|_| usage()),
            "--level-type" => {
                let t = val();
                o.level_type = if t.contains(':') {
                    t
                } else {
                    format!("minecraft:{t}")
                };
            }
            "--gamemode" => o.gamemode = val(),
            "--keep" => o.keep = true,
            "--tree" => o.tree = true,
            "--tree-strict" => (o.tree, o.tree_strict) = (true, true),
            "--out" => o.out = PathBuf::from(val()),
            "--ours-bin" => o.ours_bin = PathBuf::from(val()),
            "--record" => mode = Mode::Record(PathBuf::from(val())),
            "--golden" => mode = Mode::Golden(PathBuf::from(val())),
            f if !f.starts_with("--") => files.push(PathBuf::from(f)),
            _ => usage(),
        }
    }
    if o.ours_bin.as_os_str().is_empty() || !o.ours_bin.exists() {
        eprintln!(
            "mc-command-diff: our server binary not found: {}",
            o.ours_bin.display()
        );
        std::process::exit(2);
    }
    if o.tree && !matches!(mode, Mode::Both) {
        usage();
    }
    if files.is_empty() && !o.tree {
        usage();
    }
    let mut corpora = Vec::new();
    for f in &files {
        match Corpus::load(f) {
            Ok(c) => corpora.push(c),
            Err(e) => {
                eprintln!("mc-command-diff: {e}");
                std::process::exit(2);
            }
        }
    }
    if corpora.is_empty() {
        corpora.push(Corpus::tree_only());
    }
    let code = tokio::select! {
        c = run_all(&o, &mode, &corpora) => c,
        _ = tokio::signal::ctrl_c() => {
            eprintln!("interrupted");
            130
        }
    };
    std::process::exit(code);
}
