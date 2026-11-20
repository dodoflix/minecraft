//! `datafix-corpus extract WORLD_DIR --id ID --out DIR [--flat]`
//! `datafix-corpus select [--count N=300] [--budget BYTES=786432] [--max-gz BYTES=24576] [--max-gz-oversize BYTES=163840]
//!                        [--seed N=1] [--targets V,V,..] --out REQUESTS CANDIDATES..`
//! `datafix-corpus import GENERATED_BATCH OUT.cand`
//! `datafix-corpus pack REQUESTS RECORDS OUT.tsv.gz`
//! `datafix-corpus dump FIXTURE.tsv.gz SAMPLE_ID`
//!
//! `extract` (`--flat`: the overworld is a flat world, which the context of a chunk update tells) writes `DIR/<TYPE>.cand`
//! for every `DataFixTypes` constant the world has files for (the format is
//! `Candidate::to_line`); `import` does the same for the batch lines of the schema-driven generator. `select` takes the
//! candidate files of one type (any number of worlds and generators) and
//! writes the batch input of `scripts/datafix-oracle.sh batch` (`ID TYPE FROM TO BASE64`): the kept samples, one line
//! per target version above the sample's version. `pack` pairs those requests with the oracle's records into the
//! gzip fixture (format: `pack.rs`). `dump` writes the stored input of one sample to stdout (`NbtIo` bytes, the input
//! of `scripts/datafix-oracle.sh fix TYPE FROM TO FILE`). Driven by `scripts/datafix-corpus.sh`.

#![expect(
    clippy::print_stderr,
    reason = "command-line tool: its messages are the product"
)]
use base64::{Engine, engine::general_purpose::STANDARD};
use datafix_corpus::candidate::Candidate;
use datafix_corpus::select::{Params, dedup, requests, select};
use datafix_corpus::{extract, pack};
use flate2::{Compression, read::MultiGzDecoder, write::GzEncoder};
use mc_datafix::CURRENT_VERSION;
use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::path::Path;
use std::process::ExitCode;

/// The band ends of the fix chain, then the current version: a sample is replayed to every one above its version.
const TARGETS: [i32; 8] = [1343, 1459, 2199, 2699, 3299, 3999, 4898, CURRENT_VERSION];

type Fail = String;

fn read_gz(path: &str) -> Result<String, Fail> {
    let mut text = String::new();
    MultiGzDecoder::new(std::fs::File::open(path).map_err(|e| format!("{path}: {e}"))?)
        .read_to_string(&mut text)
        .map_err(|e| format!("{path}: {e}"))?;
    Ok(text)
}

fn run(args: &[String]) -> Result<(), Fail> {
    let usage = || {
        "usage: datafix-corpus extract WORLD --id ID --out DIR [--flat] | import BATCH OUT | select [options] --out FILE CAND.. | pack REQ REC OUT.tsv.gz | dump FIXTURE ID".to_string()
    };
    match args.first().map(String::as_str) {
        Some("extract") => {
            let (mut world, mut id, mut out, mut flat) = (None, None, None, false);
            let mut it = args[1..].iter();
            while let Some(a) = it.next() {
                match a.as_str() {
                    "--id" => id = it.next(),
                    "--out" => out = it.next(),
                    "--flat" => flat = true,
                    _ if world.is_none() => world = Some(a),
                    _ => return Err(usage()),
                }
            }
            let (Some(world), Some(id), Some(out)) = (world, id, out) else {
                return Err(usage());
            };
            let got = extract::extract_world(Path::new(world), id, flat);
            for s in &got.skipped {
                eprintln!("datafix-corpus: skipped {s}");
            }
            let read = got.candidates.len();
            let kept = dedup(got.candidates);
            let mut by_type: BTreeMap<&str, String> = BTreeMap::new();
            for c in &kept {
                let text = by_type.entry(&c.ty).or_default();
                text.push_str(&c.to_line());
                text.push('\n');
            }
            std::fs::create_dir_all(out).map_err(|e| format!("{out}: {e}"))?;
            for (ty, text) in &by_type {
                std::fs::write(Path::new(out).join(format!("{ty}.cand")), text)
                    .map_err(|e| format!("{out}: {e}"))?;
            }
            eprintln!(
                "datafix-corpus: {id}: {read} inputs, {} distinct, {} types",
                kept.len(),
                by_type.len()
            );
            Ok(())
        }
        Some("select") => {
            let mut p = Params {
                count: 300,
                budget: 786_432,
                max_gz: 24_576,
                max_gz_oversize: 163_840,
                seed: 1,
            };
            let (mut out, mut targets, mut files) = (None, TARGETS.to_vec(), Vec::new());
            let mut it = args[1..].iter();
            while let Some(a) = it.next() {
                let mut num = |name: &str| -> Result<u64, Fail> {
                    it.next()
                        .and_then(|v| v.parse().ok())
                        .ok_or_else(|| format!("{name} needs a number"))
                };
                match a.as_str() {
                    "--count" => p.count = num("--count")? as usize,
                    "--budget" => p.budget = num("--budget")?,
                    "--max-gz" => p.max_gz = num("--max-gz")?,
                    "--max-gz-oversize" => p.max_gz_oversize = num("--max-gz-oversize")?,
                    "--seed" => p.seed = num("--seed")?,
                    "--targets" => {
                        targets = it
                            .next()
                            .and_then(|v| v.split(',').map(|t| t.parse().ok()).collect())
                            .ok_or("--targets needs a comma separated list of versions")?;
                    }
                    "--out" => out = it.next(),
                    f => files.push(f.to_string()),
                }
            }
            let Some(out) = out else { return Err(usage()) };
            let mut cands = Vec::new();
            for f in &files {
                let text = std::fs::read_to_string(f).map_err(|e| format!("{f}: {e}"))?;
                for line in text.lines() {
                    cands.push(Candidate::parse_line(line)?);
                }
            }
            let total = cands.len();
            let kept = select(cands, &p);
            let lines = requests(&kept, &targets, CURRENT_VERSION);
            eprintln!(
                "datafix-corpus: {} of {total} candidates kept, {} requests",
                kept.len(),
                lines.len()
            );
            let mut text = lines.join("\n");
            if !text.is_empty() {
                text.push('\n');
            }
            std::fs::write(out, text).map_err(|e| format!("{out}: {e}"))
        }
        Some("import") if args.len() == 3 => {
            // the batch lines of the sample generator: the target version of a line is not used
            let text =
                std::fs::read_to_string(&args[1]).map_err(|e| format!("{}: {e}", args[1]))?;
            let (mut out, mut skipped) = (String::new(), 0);
            for line in text.lines() {
                let [id, ty, from, _to, b64] = line.split('\t').collect::<Vec<_>>()[..] else {
                    return Err(format!("not a batch line: {line:.80}"));
                };
                let bytes = STANDARD.decode(b64).map_err(|e| format!("{id}: {e}"))?;
                let from = from.parse().map_err(|e| format!("{id}: {e}"))?;
                match Candidate::from_bytes(id.to_string(), ty, from, bytes) {
                    Ok(c) => {
                        out.push_str(&c.to_line());
                        out.push('\n');
                    }
                    Err(_) => skipped += 1,
                }
            }
            eprintln!("datafix-corpus: {skipped} generated samples not reproducible, skipped");
            std::fs::write(&args[2], out).map_err(|e| format!("{}: {e}", args[2]))
        }
        Some("pack") if args.len() == 4 => {
            let read = |p: &str| std::fs::read_to_string(p).map_err(|e| format!("{p}: {e}"));
            let text = pack::pack(&read(&args[1])?, &read(&args[2])?)?;
            let mut enc = GzEncoder::new(Vec::new(), Compression::best());
            enc.write_all(text.as_bytes()).map_err(|e| e.to_string())?;
            let bytes = enc.finish().map_err(|e| e.to_string())?;
            std::fs::write(&args[3], bytes).map_err(|e| format!("{}: {e}", args[3]))
        }
        Some("dump") if args.len() == 3 => {
            let bytes = pack::dump(&read_gz(&args[1])?, &args[2])
                .ok_or_else(|| format!("no sample {} in {}", args[2], args[1]))?;
            std::io::stdout()
                .write_all(&bytes)
                .map_err(|e| e.to_string())
        }
        _ => Err(usage()),
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("datafix-corpus: {e}");
            ExitCode::FAILURE
        }
    }
}
