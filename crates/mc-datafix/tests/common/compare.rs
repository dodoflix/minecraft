//! One update compared between vanilla and our fixer, by four rules: the result tree (`==`), its `NbtIo` bytes (compound
//! key order is `HashMap` order, so this is the only check of a rebuilt compound's capacity history), the exception that
//! was thrown (class and message hash) and the ERROR log events (count and message hash).
//!
//! Both sides are an [`Outcome`]: vanilla's comes from a record of the oracle (`oracle_client`), ours from [`run_ours`].

use mc_datafix::{DataFixTypes, DataFixer, References};
use mc_dfu::{Dyn, Dynamic, NBT};
use mc_nbt::{CompoundTag, NbtAccounter, Tag};
use sha2::{Digest, Sha256};
use std::cell::RefCell;
use std::fmt;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Once;

/// One update to run: `ty` is a `DataFixTypes` constant name or `ref:NAME` (a `References` type name).
#[derive(Clone)]
pub struct Request {
    pub id: String,
    pub ty: String,
    pub from: i32,
    pub to: i32,
    pub input: CompoundTag,
}

#[derive(Clone, Debug)]
pub enum Status {
    /// The updated compound and its `NbtIo.write` bytes.
    Ok { tag: CompoundTag, bytes: Vec<u8> },
    /// The call threw: the exception class and the sha256 of its message.
    Threw { class: String, message_hash: String },
    /// The input could not be read.
    Bad(String),
}

/// What one update produced: the result and the ERROR log events of the call (their count and the sha256 of the
/// messages joined with LF, `None` when there was none).
#[derive(Clone, Debug)]
pub struct Outcome {
    pub status: Status,
    pub error_count: usize,
    pub errors_hash: Option<String>,
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

pub fn errors_hash(errors: &[String]) -> Option<String> {
    (!errors.is_empty()).then(|| sha256_hex(errors.join("\n").as_bytes()))
}

/// The ways two outcomes differ, the first failing rule wins.
#[derive(Debug, PartialEq, Eq)]
pub enum Mismatch {
    /// One side returned a result and the other threw or could not read the input.
    Status {
        vanilla: String,
        ours: String,
    },
    ExceptionClass {
        vanilla: String,
        ours: String,
    },
    ExceptionMessage {
        vanilla: String,
        ours: String,
    },
    /// The result trees differ.
    TreeDiffers,
    /// The trees are equal but the `NbtIo` bytes differ: a compound iterates in another key order.
    OrderOnly,
    ErrorCount {
        vanilla: usize,
        ours: usize,
    },
    ErrorMessages {
        vanilla: String,
        ours: String,
    },
}

impl Mismatch {
    pub fn is_order_only(&self) -> bool {
        matches!(self, Mismatch::OrderOnly)
    }
}

impl fmt::Display for Mismatch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Mismatch::Status { vanilla, ours } => {
                write!(f, "vanilla: {vanilla}; ours: {ours}")
            }
            Mismatch::ExceptionClass { vanilla, ours } => {
                write!(f, "exception class: vanilla {vanilla}, ours {ours}")
            }
            Mismatch::ExceptionMessage { vanilla, ours } => {
                write!(f, "exception message hash: vanilla {vanilla}, ours {ours}")
            }
            Mismatch::TreeDiffers => f.write_str("tree-differs"),
            Mismatch::OrderOnly => f.write_str("order-only: equal tree, different key order"),
            Mismatch::ErrorCount { vanilla, ours } => {
                write!(f, "ERROR log events: vanilla {vanilla}, ours {ours}")
            }
            Mismatch::ErrorMessages { vanilla, ours } => {
                write!(f, "ERROR log message hash: vanilla {vanilla}, ours {ours}")
            }
        }
    }
}

fn describe(s: &Status) -> String {
    match s {
        Status::Ok { .. } => "returned a result".into(),
        Status::Threw { class, .. } => format!("threw {class}"),
        Status::Bad(why) => format!("unreadable input ({why})"),
    }
}

/// The key order of the `Properties` of every block state (a compound holding a string `Name` and a compound
/// `Properties`) is sorted. Vanilla builds the legacy block states of `BlockStateData` and `ChunkPalettedStorageFix`
/// from immutable `Map.of` maps, whose iteration order changes with every JVM run: the key order of those `Properties`
/// differs between two runs of vanilla itself, so only the rest of the key order is compared.
fn sort_block_state_properties(tag: &mut Tag) {
    match tag {
        Tag::Compound(c) => {
            let is_block_state = matches!(c.get("Name"), Some(Tag::String(_)));
            if is_block_state && let Some(Tag::Compound(properties)) = c.get_mut("Properties") {
                let mut entries: Vec<(String, Tag)> = properties
                    .iter()
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect();
                entries.sort_by(|a, b| a.0.cmp(&b.0));
                *properties = entries.into_iter().collect();
            }
            c.values_mut().for_each(sort_block_state_properties);
        }
        Tag::List(l) => l.iter_mut().for_each(sort_block_state_properties),
        _ => {}
    }
}

/// Whether the two results (their `NbtIo` bytes) come in the same key order apart from the `Properties` of block states.
fn same_order_but_block_state_properties(vanilla: &[u8], ours: &[u8]) -> bool {
    let read = |bytes: &[u8]| {
        let mut tag = Tag::Compound(read_tag(bytes).expect("a result is NBT"));
        sort_block_state_properties(&mut tag);
        tag
    };
    first_order_difference(&read(vanilla), &read(ours)).is_none()
}

pub fn compare(vanilla: &Outcome, ours: &Outcome) -> Result<(), Mismatch> {
    match (&vanilla.status, &ours.status) {
        (Status::Ok { tag: vt, bytes: vb }, Status::Ok { tag: ot, bytes: ob }) => {
            if vt != ot {
                return Err(Mismatch::TreeDiffers);
            }
            if vb != ob && !(vb.len() == ob.len() && same_order_but_block_state_properties(vb, ob))
            {
                return Err(Mismatch::OrderOnly);
            }
        }
        (
            Status::Threw {
                class: vc,
                message_hash: vm,
            },
            Status::Threw {
                class: oc,
                message_hash: om,
            },
        ) => {
            if vc != oc {
                return Err(Mismatch::ExceptionClass {
                    vanilla: vc.clone(),
                    ours: oc.clone(),
                });
            }
            if vm != om {
                return Err(Mismatch::ExceptionMessage {
                    vanilla: vm.clone(),
                    ours: om.clone(),
                });
            }
        }
        (Status::Bad(_), Status::Bad(_)) => {}
        (v, o) => {
            return Err(Mismatch::Status {
                vanilla: describe(v),
                ours: describe(o),
            });
        }
    }
    if vanilla.error_count != ours.error_count {
        return Err(Mismatch::ErrorCount {
            vanilla: vanilla.error_count,
            ours: ours.error_count,
        });
    }
    if vanilla.errors_hash != ours.errors_hash {
        return Err(Mismatch::ErrorMessages {
            vanilla: vanilla.errors_hash.clone().unwrap_or_default(),
            ours: ours.errors_hash.clone().unwrap_or_default(),
        });
    }
    Ok(())
}

/// A readable summary of failed comparisons: the number of `tree-differs` and `order-only` results and the first
/// `shown` of them with their ids.
pub fn report(failures: &[(String, Mismatch)], total: usize, shown: usize) -> String {
    let order_only = failures.iter().filter(|(_, m)| m.is_order_only()).count();
    let tree = failures
        .iter()
        .filter(|(_, m)| matches!(m, Mismatch::TreeDiffers))
        .count();
    let mut out = format!(
        "{} of {total} differ ({tree} tree-differs, {order_only} order-only)\n",
        failures.len()
    );
    for (id, m) in failures.iter().take(shown) {
        out.push_str(&format!("  {id}: {m}\n"));
    }
    out
}

// ---- our side ------------------------------------------------------------------------------------------------------

thread_local! {
    static ERRORS: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
}

struct ErrorCapture;

impl log::Log for ErrorCapture {
    fn enabled(&self, metadata: &log::Metadata) -> bool {
        metadata.level() == log::Level::Error
    }
    fn log(&self, record: &log::Record) {
        if self.enabled(record.metadata()) {
            ERRORS.with(|e| e.borrow_mut().push(record.args().to_string()));
        }
    }
    fn flush(&self) {}
}

/// Runs `f` and returns what it logged at ERROR level on this thread. Installs the process-wide capture logger on the
/// first call; a different logger already installed is a failure, not a silent empty capture.
pub fn capture_errors<T>(f: impl FnOnce() -> T) -> (T, Vec<String>) {
    static INSTALL: Once = Once::new();
    INSTALL.call_once(|| {
        log::set_logger(&ErrorCapture).expect("another logger is already installed");
        log::set_max_level(log::LevelFilter::Error);
    });
    ERRORS.with(|e| e.borrow_mut().clear());
    let out = f();
    (out, ERRORS.with(|e| std::mem::take(&mut *e.borrow_mut())))
}

/// The class and message of a panic raised by a port of a Java exception: a payload `java.lang.X: message` (a
/// qualified class name, a colon, a space) keeps its class; any other panic is class `panic`.
fn thrown(payload: &(dyn std::any::Any + Send)) -> (String, String) {
    let message = payload
        .downcast_ref::<String>()
        .map(String::as_str)
        .or_else(|| payload.downcast_ref::<&str>().copied())
        .unwrap_or("");
    if let Some((class, rest)) = message.split_once(": ")
        && class.contains('.')
        && class
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '$' | '_'))
    {
        return (class.to_string(), rest.to_string());
    }
    ("panic".to_string(), message.to_string())
}

/// `DataFixTypes.update` / `fixer.update(reference, ..)` of our fixer for `request`, with panics as a thrown exception.
pub fn run_ours(fixer: &DataFixer, request: &Request) -> Outcome {
    let (result, errors) = capture_errors(|| {
        catch_unwind(AssertUnwindSafe(|| match request.ty.strip_prefix("ref:") {
            Some(name) => {
                let reference = References::by_name(name)
                    .unwrap_or_else(|| panic!("unknown References type {name}"));
                let input = Dynamic::new(NBT, Dyn::from(&Tag::Compound(request.input.clone())));
                match fixer
                    .update(&reference, input, request.from, request.to)
                    .value
                    .to_tag()
                {
                    Ok(Tag::Compound(c)) => c,
                    Ok(other) => panic!("java.lang.ClassCastException: {other:?}"),
                    Err(e) => panic!("invalid NBT tree: {e}"),
                }
            }
            None => DataFixTypes::by_name(&request.ty)
                .unwrap_or_else(|| panic!("unknown DataFixTypes constant {}", request.ty))
                .update_tag(fixer, &request.input, request.from, request.to),
        }))
    });
    let status = match result {
        // The tree is the one read back from the written bytes, like the oracle's: a compound that holds an end tag
        // (a `JsonNull` converted to NBT) is cut short where the end tag is written.
        Ok(tag) => {
            let bytes = mc_nbt::nbt_io::to_bytes(&tag);
            Status::Ok {
                tag: read_tag(&bytes).unwrap_or(tag),
                bytes,
            }
        }
        Err(payload) => {
            let (class, message) = thrown(&*payload);
            Status::Threw {
                class,
                message_hash: sha256_hex(message.as_bytes()),
            }
        }
    };
    Outcome {
        status,
        error_count: errors.len(),
        errors_hash: errors_hash(&errors),
    }
}

/// Parses the `NbtIo.write` bytes of the oracle (a named root compound).
pub fn read_tag(bytes: &[u8]) -> Result<CompoundTag, String> {
    let mut input = bytes;
    mc_nbt::read_named(&mut input, &mut NbtAccounter::unlimited_heap()).map_err(|e| e.to_string())
}

/// Parses a record of the oracle (format: the header of `DataFixerOracle.java`).
pub fn parse_record(line: &str) -> (String, Outcome) {
    let f: Vec<&str> = line.splitn(9, '\t').collect();
    assert_eq!(f.len(), 9, "malformed oracle record: {line}");
    let dash = |s: &str| (s != "-").then(|| s.to_string());
    let status = match f[1] {
        "ok" => {
            let bytes = base64::Engine::decode(&base64::engine::general_purpose::STANDARD, f[3])
                .expect("record bytes are base64");
            let tag =
                read_tag(&bytes).unwrap_or_else(|e| panic!("record {} is not NBT: {e}", f[0]));
            Status::Ok { tag, bytes }
        }
        "threw" => Status::Threw {
            class: f[4].to_string(),
            message_hash: f[5].to_string(),
        },
        "bad" => Status::Bad(f[4].to_string()),
        other => panic!("unknown record status {other}"),
    };
    let outcome = Outcome {
        status,
        error_count: f[6].parse().expect("error count"),
        errors_hash: dash(f[7]),
    };
    (f[0].to_string(), outcome)
}

/// The path and the two key sequences of the first compound whose keys come in another order in `a` than in `b`
/// (both read back from `NbtIo` bytes, so the order is the written order).
pub fn first_order_difference(a: &Tag, b: &Tag) -> Option<String> {
    match (a, b) {
        (Tag::Compound(x), Tag::Compound(y)) => {
            let (kx, ky): (Vec<&String>, Vec<&String>) = (x.keys().collect(), y.keys().collect());
            if kx != ky {
                return Some(format!("keys {kx:?} vs {ky:?}"));
            }
            kx.iter().find_map(|k| {
                first_order_difference(x.get(k)?, y.get(k)?).map(|d| format!("{k}/{d}"))
            })
        }
        (Tag::List(x), Tag::List(y)) => x
            .iter()
            .zip(y.iter())
            .enumerate()
            .find_map(|(i, (p, q))| first_order_difference(p, q).map(|d| format!("[{i}]/{d}"))),
        _ => None,
    }
}
