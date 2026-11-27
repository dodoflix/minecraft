//! `GameTestMainUtil.runGameTestServer`: the command line of `net.minecraft.gametest.Main` (a jopt-simple parser), the
//! universe directory and the packs folder, then the headless server.

use super::game_test_server::{self, GameTestConfig, LEVEL_NAME};
use super::global_test_reporter;
use super::junit_like_test_reporter::JUnitLikeTestReporter;
use std::path::{Path, PathBuf};

const DEFAULT_UNIVERSE_DIR: &str = "gametestserver";

/// `parser.printHelpOn(System.err)`, as the real jar prints it.
const HELP: &str = include_str!("game_test_help.txt");

/// An exception of the option parser or of the file operations: what vanilla's uncaught exception prints through its
/// `System.err` log redirect (first line, then the `Caused by:` lines; the stack frames are not reproduced).
#[derive(Debug)]
struct Exception {
    class: &'static str,
    message: String,
    caused_by: Vec<String>,
}

impl Exception {
    fn new(class: &'static str, message: String) -> Self {
        Self {
            class,
            message,
            caused_by: Vec::new(),
        }
    }

    /// The exception ends `Main.main` on the `ServerMain` thread, which the bundler never joins: the process exits
    /// with 0.
    #[expect(
        clippy::print_stderr,
        reason = "the default uncaught exception handler writes its prefix to the process's stderr"
    )]
    fn report(&self) -> i32 {
        // `System.err` (the log redirect) prints the trace; only the handler's own prefix reaches the real stderr
        eprint!("Exception in thread \"ServerMain\" ");
        log::info!("[STDERR]: {}: {}", self.class, self.message);
        for cause in &self.caused_by {
            log::info!("[STDERR]: Caused by: {cause}");
        }
        0
    }
}

/// The values every option got on the command line, in order.
#[derive(Default)]
struct Options {
    help: bool,
    values: Vec<(&'static str, String)>,
}

impl Options {
    fn has(&self, name: &str) -> bool {
        self.values.iter().any(|(n, _)| *n == name)
    }

    /// `OptionSet.valueOf(spec)`: the one value, `None` without any; a second occurrence is an exception.
    fn value_of(&self, name: &str) -> Result<Option<&str>, Exception> {
        let mut values = self
            .values
            .iter()
            .filter(|(n, _)| *n == name)
            .map(|(_, v)| v.as_str());
        let first = values.next();
        if values.next().is_some() {
            return Err(Exception::new(
                "joptsimple.MultipleArgumentsForOptionException",
                format!("Found multiple arguments for option {name}, but you asked for only one"),
            ));
        }
        Ok(first)
    }
}

/// The names of the options, for jopt-simple's unique-prefix abbreviations.
const NAMES: [&str; 7] = [
    "help",
    "packs",
    "repeatCount",
    "report",
    "tests",
    "universe",
    "verify",
];

/// jopt-simple's `isRecognized`: the full name or a unique abbreviation.
fn resolve(name: &str) -> Option<&'static str> {
    let mut candidates = NAMES.iter().copied().filter(|n| n.starts_with(name));
    match (
        NAMES.iter().copied().find(|n| *n == name),
        candidates.next(),
        candidates.next(),
    ) {
        (Some(exact), ..) => Some(exact),
        (None, Some(only), None) => Some(only),
        _ => None,
    }
}

/// `handleShortOptionCluster`: `-tVALUE`, `-hu DIR`: single-letter options, the first one that takes an argument takes the
/// rest of the token (or the next argument). A character that names no option makes the whole token a plain argument.
fn short_cluster(body: &str) -> Option<Vec<(&'static str, Option<String>)>> {
    let chars: Vec<char> = body.chars().collect();
    let options: Vec<&str> = chars
        .iter()
        .scan(false, |done, c| {
            (!*done).then(|| {
                let full = resolve(&c.to_string());
                *done = full != Some("help");
                full
            })
        })
        .collect::<Option<_>>()?;
    let mut actions = Vec::new();
    for (i, full) in options.into_iter().enumerate() {
        if full == "help" {
            actions.push((full, None));
        } else {
            let rest = chars[i + 1..].iter().collect::<String>();
            actions.push((full, (!rest.is_empty()).then_some(rest)));
        }
    }
    Some(actions)
}

/// `parser.parse(args)` with `allowsUnrecognizedOptions()`: `--name value`, `--name=value`, `-name value` and unique
/// abbreviations, short-option clusters; a token that names no option (an ambiguous abbreviation too) is a plain argument;
/// `--` ends the options.
fn parse(args: &[String]) -> Result<Options, Exception> {
    let mut options = Options::default();
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        if arg == "--" {
            break;
        }
        let (long, body) = match arg.strip_prefix("--") {
            Some(body) => (true, body),
            None => (false, arg.strip_prefix('-').unwrap_or_default()),
        };
        if body.is_empty() {
            continue;
        }
        let (name, inline) = match body.split_once('=') {
            Some((name, value)) => (name, Some(value.to_owned())),
            None => (body, None),
        };
        let actions = match resolve(name) {
            Some(full) => Some(vec![(full, inline)]),
            None if long => None,
            None => short_cluster(body),
        };
        for (full, inline) in actions.into_iter().flatten() {
            if full == "help" {
                options.help = true;
                continue;
            }
            let Some(value) = inline.or_else(|| args.next().cloned()) else {
                return Err(Exception::new(
                    "joptsimple.OptionMissingRequiredArgumentException",
                    format!("Option {full} requires an argument"),
                ));
            };
            options.values.push((full, value));
        }
    }
    Ok(options)
}

/// `Integer.valueOf(text)` through jopt-simple's value converter.
fn repeat_count_of(text: &str) -> Result<i32, Exception> {
    text.parse::<i32>().map_err(|_| Exception {
        class: "joptsimple.OptionArgumentConversionException",
        message: format!("Cannot parse argument '{text}' of option repeatCount"),
        caused_by: vec![
            format!(
                "joptsimple.internal.ReflectionException: java.lang.NumberFormatException: For input string: \"{text}\""
            ),
            format!("java.lang.NumberFormatException: For input string: \"{text}\""),
        ],
    })
}

/// `runGameTestServer(args, onUniverseCreated)`: the process exit code when this returns (the server itself ends the
/// process through `System.exit`).
pub fn run_game_test_server(args: &[String]) -> i32 {
    run(args).unwrap_or_else(|exception| exception.report())
}

#[expect(
    clippy::print_stderr,
    reason = "the help text and the parser's exception go to stderr, as in vanilla"
)]
fn run(args: &[String]) -> Result<i32, Exception> {
    // the parser runs before the console log exists: its exception goes to the process's own stderr
    let options = match parse(args) {
        Ok(options) => options,
        Err(exception) => {
            eprintln!(
                "Exception in thread \"ServerMain\" {}: {}",
                exception.class, exception.message
            );
            return Ok(0);
        }
    };
    if options.help {
        eprint!("{HELP}");
        return Ok(0);
    }
    // `Boolean.valueOf`: anything but "true" is false
    let verify = options
        .value_of("verify")?
        .is_some_and(|v| v.eq_ignore_ascii_case("true"));
    if verify && !options.has("tests") {
        log::error!(
            "Please specify a test selection to run the verify option. For example: --verify --tests example:test_something_*"
        );
        std::process::exit(-1);
    }
    if verify && options.has("repeatCount") {
        log::info!("Flag --verify is true, the --repeatCount value will be ignored");
    }
    let universe = options
        .value_of("universe")?
        .unwrap_or(DEFAULT_UNIVERSE_DIR);
    log::info!(
        "Running GameTestMain with cwd '{}', universe path '{}'",
        std::env::current_dir()
            .map(|d| d.display().to_string())
            .unwrap_or_default(),
        universe
    );
    if let Some(report) = options.value_of("report")? {
        global_test_reporter::replace_with(Box::new(JUnitLikeTestReporter::new(PathBuf::from(
            report,
        ))));
    }
    let io = |e: std::io::Error| Exception::new("java.io.IOException", e.to_string());
    create_or_reset_dir(Path::new(universe)).map_err(io)?;
    if let Some(packs) = options.value_of("packs")? {
        copy_packs(Path::new(universe), Path::new(packs)).map_err(io)?;
    }
    // the arguments of `GameTestServer.create(..)`, read when the server thread starts
    let config = || -> Result<GameTestConfig, Exception> {
        let selection = options.value_of("tests")?.map(str::to_owned);
        let repeat_count = match options.value_of("repeatCount")? {
            None => 1,
            Some(text) => repeat_count_of(text)?,
        };
        Ok(GameTestConfig {
            selection,
            verify,
            repeat_count,
        })
    };
    Ok(game_test_server::spin(
        Path::new(universe),
        || match config() {
            Ok(config) => Some(config),
            Err(exception) => {
                exception.report();
                None
            }
        },
    ))
}

/// `createOrResetDir`.
fn create_or_reset_dir(universe: &Path) -> std::io::Result<()> {
    if universe.exists() {
        std::fs::remove_dir_all(universe)?;
    }
    std::fs::create_dir_all(universe)
}

/// `copyPacks`: the folder packs (with a `pack.mcmeta`) and the zip packs of the source folder.
fn copy_packs(universe: &Path, pack_source: &Path) -> std::io::Result<()> {
    let world_pack_folder = universe.join(LEVEL_NAME).join("datapacks");
    if !world_pack_folder.exists() {
        std::fs::create_dir_all(&world_pack_folder)?;
    }
    if !pack_source.exists() {
        return Ok(());
    }
    for entry in std::fs::read_dir(pack_source)? {
        let path = entry?.path();
        let name = path.file_name().unwrap_or_default().to_owned();
        let destination = world_pack_folder.join(&name);
        if path.is_dir() {
            if path.join("pack.mcmeta").is_file() {
                copy_dir(&path, &destination)?;
                log::info!("Included folder pack {}", name.to_string_lossy());
            }
        } else if path.to_string_lossy().ends_with(".zip") {
            std::fs::copy(&path, &destination)?;
            log::info!("Included zip pack {}", name.to_string_lossy());
        }
    }
    Ok(())
}

fn copy_dir(from: &Path, to: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let target = to.join(entry.file_name());
        if entry.path().is_dir() {
            copy_dir(&entry.path(), &target)?;
        } else {
            std::fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::parse;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| (*s).to_owned()).collect()
    }

    #[test]
    fn options_take_a_value_inline_or_next_and_may_be_abbreviated() {
        let options = parse(&args(&[
            "--tests=a:b",
            "--univ",
            "u",
            "--verify",
            "true",
            "x",
        ]))
        .unwrap();
        assert_eq!(options.value_of("tests").unwrap(), Some("a:b"));
        assert_eq!(options.value_of("universe").unwrap(), Some("u"));
        assert_eq!(options.value_of("verify").unwrap(), Some("true"));
    }

    #[test]
    fn an_ambiguous_or_unknown_name_is_a_plain_argument_and_a_missing_value_an_error() {
        let options = parse(&args(&["--re", "x", "--nothing", "y", "-", "--tests", "t"])).unwrap();
        assert!(!options.has("repeatCount") && !options.has("report"));
        assert_eq!(options.value_of("tests").unwrap(), Some("t"));
        assert!(parse(&args(&["--tests"])).is_err());
    }

    #[test]
    fn a_double_dash_ends_the_options_and_a_repeated_option_fails_when_read() {
        let options = parse(&args(&["--tests", "a", "--", "--tests", "b"])).unwrap();
        assert_eq!(options.value_of("tests").unwrap(), Some("a"));
        let options = parse(&args(&["--tests", "a", "--tests", "b"])).unwrap();
        assert!(options.has("tests"));
        assert!(options.value_of("tests").is_err());
    }

    /// The results of jopt-simple 5.0.4 for the same option set and `allowsUnrecognizedOptions()`.
    #[test]
    fn short_options_form_clusters_like_jopt_simple() {
        let value = |list: &[&str], name: &str| {
            parse(&args(list))
                .unwrap()
                .value_of(name)
                .unwrap()
                .map(str::to_owned)
        };
        assert_eq!(value(&["-tfoo"], "tests").as_deref(), Some("foo"));
        assert_eq!(value(&["-t", "foo"], "tests").as_deref(), Some("foo"));
        assert_eq!(value(&["-ufoo"], "universe").as_deref(), Some("foo"));
        assert_eq!(value(&["-tab=c"], "tests").as_deref(), Some("ab=c"));
        assert_eq!(value(&["-t=a"], "tests").as_deref(), Some("a"));
        assert_eq!(value(&["-vu"], "verify").as_deref(), Some("u"));
        // `-h` takes no argument, so `u` after it is the next option, which takes the next argument
        let options = parse(&args(&["-hu", "dir"])).unwrap();
        assert!(options.help);
        assert_eq!(options.value_of("universe").unwrap(), Some("dir"));
        // a character that names no option (`x`, the ambiguous `r`) makes the whole token a plain argument
        for plain in ["-hx", "-rx", "-5", "-rep=x"] {
            let options = parse(&args(&[plain])).unwrap();
            assert!(!options.help && options.values.is_empty(), "{plain}");
        }
        assert!(parse(&args(&["-hv"])).is_err());
    }
}
