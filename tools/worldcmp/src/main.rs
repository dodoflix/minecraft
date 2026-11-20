//! `worldcmp A_DIR B_DIR [--dim overworld,nether,end] [--chunks x1,z1,x2,z2] [--ignore-status-below full]
//! [--max-listed N] [--ignore cat[,cat..]] [--ignore-field SPEC].. [--report out.txt|out.json]`
//! `worldcmp A_DIR B_DIR --exact [--dim ..] [--chunks ..] [--max-listed N] [--ignore-field SPEC].. [--report ..]`
//! `worldcmp A_DIR B_DIR --tree [--chunks ..] [--max-listed N] [--ignore-field SPEC].. [--report ..]`
//!
//! Default mode: compares the region files of two worlds chunk by chunk (see `compare.rs` for what is compared /
//! ignored). `--exact` compares every chunk of `region/`, `entities/` and `poi/` as stored: equal tags, equal key
//! order, equal serialised bytes, no chunk skipped by status. `--tree` compares the two directories file by file (see
//! `tree.rs`). Both exact modes normalise nothing except what `--ignore-field` names (`fields.rs` has the spec
//! syntax); a typical oracle comparison passes `session.lock`, `level.dat_old`, `logs/`, `filefix/`,
//! `level.dat#Data.LastPlayed`, `level.dat#Data.Time`, `level.dat#Data.DayTime` and the tick counters of the saved
//! data it ran against.
//!
//! The text report always goes to stdout; `--report` additionally writes it to a file (JSON if the name ends in
//! `.json`). Exit status: 0 identical, 1 differences found, 2 usage/IO error.

#![expect(
    clippy::print_stdout,
    clippy::print_stderr,
    reason = "command-line tool: its output is the product"
)]
use std::path::PathBuf;
use std::process::ExitCode;

use worldcmp::compare::{Category, status_rank};
use worldcmp::fields::{FieldSpec, Ignores};
use worldcmp::tree::{self, TreeOptions, compare_trees};
use worldcmp::world::{Filter, compare_dimension, render_json, render_text};

fn usage() -> ExitCode {
    eprintln!(
        "usage: worldcmp A_DIR B_DIR [--dim overworld,nether,end] [--chunks x1,z1,x2,z2] [--ignore-status-below full] [--max-listed N=10] [--ignore cat[,cat..]] [--ignore-field GLOB#FIELD.PATH].. [--report out.txt|out.json]\n       worldcmp A_DIR B_DIR --exact|--tree [--dim overworld,nether,end (--exact only)] [--chunks x1,z1,x2,z2] [--max-listed N=10] [--ignore-field GLOB[#FIELD.PATH]].. [--report out.txt|out.json]"
    );
    ExitCode::from(2)
}

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    let (mut pos, mut dims, mut report) = (
        Vec::<String>::new(),
        vec![
            "overworld".to_owned(),
            "the_nether".into(),
            "the_end".into(),
        ],
        None,
    );
    let mut f = Filter {
        chunks: None,
        min_status: status_rank("full").unwrap_or(0),
        max_listed: 10,
        ignore: Vec::new(),
        exact: false,
        ignores: Ignores::default(),
    };
    let (mut tree_mode, mut status_set, mut dims_set) = (false, false, false);
    while let Some(a) = args.next() {
        let mut val = || args.next();
        match a.as_str() {
            "--dim" | "--dims" => match val() {
                Some(v) => {
                    dims = v.split(',').map(str::to_owned).collect();
                    dims_set = true;
                }
                None => return usage(),
            },
            "--chunks" => {
                let n: Option<Vec<i32>> =
                    val().and_then(|v| v.split(',').map(|p| p.trim().parse().ok()).collect());
                match n.as_deref() {
                    Some(&[a, b, c, d]) => f.chunks = Some((a, b, c, d)),
                    _ => return usage(),
                }
            }
            "--ignore-status-below" => match val().as_deref().and_then(status_rank) {
                Some(r) => {
                    f.min_status = r;
                    status_set = true;
                }
                None => return usage(),
            },
            "--max-listed" => match val().and_then(|v| v.parse().ok()) {
                Some(n) => f.max_listed = n,
                None => return usage(),
            },
            "--ignore" => match val().and_then(|v| {
                v.split(',')
                    .map(Category::from_name)
                    .collect::<Option<Vec<_>>>()
            }) {
                Some(c) => f.ignore.extend(c),
                None => return usage(),
            },
            "--exact" => f.exact = true,
            "--tree" => tree_mode = true,
            "--ignore-field" => match val().map(|v| v.parse::<FieldSpec>()) {
                Some(Ok(s)) => f.ignores.0.push(s),
                Some(Err(e)) => {
                    eprintln!("worldcmp: {e}");
                    return ExitCode::from(2);
                }
                None => return usage(),
            },
            "--report" => report = val(),
            s if s.starts_with("--") => return usage(),
            _ => pos.push(a),
        }
    }
    let [a, b] = pos.as_slice() else {
        return usage();
    };
    let exact = f.exact || tree_mode;
    if (f.exact && tree_mode)
        || (exact && (status_set || !f.ignore.is_empty()))
        || (tree_mode && dims_set)
    {
        return usage();
    }
    if f.exact {
        f.min_status = 0;
    }
    let (pa, pb) = (PathBuf::from(a), PathBuf::from(b));
    for (p, n) in [(&pa, a), (&pb, b)] {
        if !p.is_dir() {
            eprintln!("worldcmp: not a directory: {n}");
            return ExitCode::from(2);
        }
    }
    if tree_mode {
        let o = TreeOptions {
            ignores: f.ignores,
            chunks: f.chunks,
            max_listed: f.max_listed,
        };
        let r = compare_trees(&pa, &pb, &o);
        let text = tree::render_text(a, b, &r, 50);
        print!("{text}");
        if let Some(path) = report {
            let body = if path.ends_with(".json") {
                tree::render_json(a, b, &r)
            } else {
                text
            };
            if let Err(e) = std::fs::write(&path, body) {
                eprintln!("worldcmp: cannot write {path}: {e}");
                return ExitCode::from(2);
            }
        }
        return ExitCode::from(if r.is_clean() { 0 } else { 1 });
    }
    let reports: Vec<_> = dims
        .iter()
        .map(|d| compare_dimension(&pa, &pb, d, &f))
        .collect();
    let text = render_text(a, b, &reports, 50);
    print!("{text}");
    if let Some(path) = report {
        let body = if path.ends_with(".json") {
            render_json(a, b, &reports)
        } else {
            text
        };
        if let Err(e) = std::fs::write(&path, body) {
            eprintln!("worldcmp: cannot write {path}: {e}");
            return ExitCode::from(2);
        }
    }
    ExitCode::from(if reports.iter().all(|r| r.is_clean()) {
        0
    } else {
        1
    })
}
