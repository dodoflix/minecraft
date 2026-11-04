//! `worldcmp A_DIR B_DIR [--dim overworld,nether,end] [--chunks x1,z1,x2,z2] [--ignore-status-below full]
//! [--max-listed N] [--report out.txt|out.json]`
//!
//! Compares the region files of two worlds chunk by chunk (see `compare.rs` for what is compared / ignored). The text
//! report always goes to stdout; `--report` additionally writes it to a file (JSON if the name ends in `.json`).
//! Exit status: 0 identical, 1 differences found, 2 usage/IO error.

use std::path::PathBuf;
use std::process::ExitCode;

use worldcmp::compare::status_rank;
use worldcmp::world::{compare_dimension, render_json, render_text, Filter};

fn usage() -> ExitCode {
    eprintln!(
        "usage: worldcmp A_DIR B_DIR [--dim overworld,nether,end] [--chunks x1,z1,x2,z2] [--ignore-status-below full] [--max-listed N=10] [--report out.txt|out.json]"
    );
    ExitCode::from(2)
}

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    let (mut pos, mut dims, mut report) = (Vec::<String>::new(), vec!["overworld".to_owned(), "the_nether".into(), "the_end".into()], None);
    let mut f = Filter { chunks: None, min_status: status_rank("full").unwrap_or(0), max_listed: 10 };
    while let Some(a) = args.next() {
        let mut val = || args.next();
        match a.as_str() {
            "--dim" | "--dims" => match val() {
                Some(v) => dims = v.split(',').map(str::to_owned).collect(),
                None => return usage(),
            },
            "--chunks" => {
                let n: Option<Vec<i32>> = val().and_then(|v| v.split(',').map(|p| p.trim().parse().ok()).collect());
                match n.as_deref() {
                    Some(&[a, b, c, d]) => f.chunks = Some((a, b, c, d)),
                    _ => return usage(),
                }
            }
            "--ignore-status-below" => match val().as_deref().and_then(status_rank) {
                Some(r) => f.min_status = r,
                None => return usage(),
            },
            "--max-listed" => match val().and_then(|v| v.parse().ok()) {
                Some(n) => f.max_listed = n,
                None => return usage(),
            },
            "--report" => report = val(),
            s if s.starts_with("--") => return usage(),
            _ => pos.push(a),
        }
    }
    let [a, b] = pos.as_slice() else { return usage() };
    let (pa, pb) = (PathBuf::from(a), PathBuf::from(b));
    for (p, n) in [(&pa, a), (&pb, b)] {
        if !p.is_dir() {
            eprintln!("worldcmp: not a directory: {n}");
            return ExitCode::from(2);
        }
    }
    let reports: Vec<_> = dims.iter().map(|d| compare_dimension(&pa, &pb, d, &f)).collect();
    let text = render_text(a, b, &reports, 50);
    print!("{text}");
    if let Some(path) = report {
        let body = if path.ends_with(".json") { render_json(a, b, &reports) } else { text };
        if let Err(e) = std::fs::write(&path, body) {
            eprintln!("worldcmp: cannot write {path}: {e}");
            return ExitCode::from(2);
        }
    }
    ExitCode::from(if reports.iter().all(|r| r.is_clean()) { 0 } else { 1 })
}
