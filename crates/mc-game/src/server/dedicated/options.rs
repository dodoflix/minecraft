//! The command line of `net.minecraft.server.Main` with joptsimple semantics (`--opt`, `-opt`, `--opt value`,
//! `--opt=value`, unambiguous abbreviations, `--` terminator, short-option clusters).

use std::path::PathBuf;

/// `parser.printHelpOn(System.err)` for the vanilla option set, as printed by the real server (golden:
/// `tests/fixtures/dedicated/help.txt`).
pub const USAGE: &str = include_str!("../../../tests/fixtures/dedicated/help.txt");

/// A joptsimple exception; `Display` is `Throwable.toString()` like vanilla logs it.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{class}: {message}")]
pub struct OptionException {
    pub class: &'static str,
    pub message: String,
}

impl OptionException {
    fn unrecognized(name: &str) -> Self {
        OptionException { class: "joptsimple.UnrecognizedOptionException", message: format!("{name} is not a recognized option") }
    }

    fn missing_argument(option: &str) -> Self {
        OptionException { class: "joptsimple.OptionMissingRequiredArgumentException", message: format!("Option {option} requires an argument") }
    }
}

/// The parsed command line (`Main`'s `OptionSet`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerOptions {
    pub nogui: bool,
    pub init_settings: bool,
    pub demo: bool,
    pub bonus_chest: bool,
    pub force_upgrade: bool,
    pub erase_cache: bool,
    pub recreate_region_files: bool,
    pub safe_mode: bool,
    pub help: bool,
    pub jfr_profile: bool,
    /// `--universe` (default `.`).
    pub universe: String,
    /// `--world`.
    pub world_name: Option<String>,
    /// `--port` (default `-1`: use `server-port`).
    pub port: i32,
    /// `--serverId`.
    pub server_id: Option<String>,
    /// `--pidFile`.
    pub pid_file: Option<PathBuf>,
    /// Non-option arguments (a literal `nogui` among them also disables the GUI).
    pub non_options: Vec<String>,
    /// joptsimple's `valueOf` failure for `--pidFile` (more than one value); vanilla hits it first, before anything
    /// else happens.
    pub early_error: Option<OptionException>,
    /// The first conversion / multiple-values failure of the other options. Vanilla only notices it when it reads the
    /// value (`universe` after the EULA check, `port` and `serverId` while spinning up the server).
    pub late_error: Option<OptionException>,
}

impl Default for ServerOptions {
    fn default() -> Self {
        ServerOptions {
            nogui: false,
            init_settings: false,
            demo: false,
            bonus_chest: false,
            force_upgrade: false,
            erase_cache: false,
            recreate_region_files: false,
            safe_mode: false,
            help: false,
            jfr_profile: false,
            universe: ".".to_owned(),
            world_name: None,
            port: -1,
            server_id: None,
            pid_file: None,
            non_options: Vec::new(),
            early_error: None,
            late_error: None,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Opt {
    Nogui,
    InitSettings,
    Demo,
    BonusChest,
    ForceUpgrade,
    EraseCache,
    RecreateRegionFiles,
    SafeMode,
    Help,
    Universe,
    World,
    Port,
    ServerId,
    JfrProfile,
    PidFile,
}

/// `(name, option, takes an argument)`.
const OPTIONS: [(&str, Opt, bool); 15] = [
    ("nogui", Opt::Nogui, false),
    ("initSettings", Opt::InitSettings, false),
    ("demo", Opt::Demo, false),
    ("bonusChest", Opt::BonusChest, false),
    ("forceUpgrade", Opt::ForceUpgrade, false),
    ("eraseCache", Opt::EraseCache, false),
    ("recreateRegionFiles", Opt::RecreateRegionFiles, false),
    ("safeMode", Opt::SafeMode, false),
    ("help", Opt::Help, false),
    ("universe", Opt::Universe, true),
    ("world", Opt::World, true),
    ("port", Opt::Port, true),
    ("serverId", Opt::ServerId, true),
    ("jfrProfile", Opt::JfrProfile, false),
    ("pidFile", Opt::PidFile, true),
];

/// joptsimple's `AbbreviationMap`: an exact name, or a prefix that selects exactly one option.
fn lookup(name: &str) -> Option<(&'static str, Opt, bool)> {
    if name.is_empty() {
        return None;
    }
    if let Some(&(n, o, arg)) = OPTIONS.iter().find(|(n, _, _)| *n == name) {
        return Some((n, o, arg));
    }
    let mut candidates = OPTIONS.iter().filter(|(n, _, _)| n.starts_with(name));
    match (candidates.next(), candidates.next()) {
        (Some(&(n, o, arg)), None) => Some((n, o, arg)),
        _ => None,
    }
}

#[derive(Default)]
struct Collected {
    flags: Vec<Opt>,
    values: Vec<(Opt, &'static str, String)>,
}

impl ServerOptions {
    /// `parser.parse(args)` plus the `valueOf` conversions (lazily reported, see `early_error` / `late_error`).
    pub fn parse<I, S>(args: I) -> Result<ServerOptions, OptionException>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let args: Vec<String> = args.into_iter().map(|a| a.as_ref().to_owned()).collect();
        let mut collected = Collected::default();
        let mut non_options = Vec::new();
        let mut i = 0;
        while i < args.len() {
            let token = &args[i];
            i += 1;
            if token == "--" {
                non_options.extend(args[i..].iter().cloned());
                break;
            }
            let handled = if let Some(body) = token.strip_prefix("--") {
                Some(handle_long(body, &args, &mut i, &mut collected)?)
            } else if token.len() > 1 && token.starts_with('-') {
                Some(handle_short(&token[1..], &args, &mut i, &mut collected)?)
            } else {
                None
            };
            if handled.is_none() {
                non_options.push(token.clone());
            }
        }
        Ok(collected.into_options(non_options))
    }

    /// `!options.has(nogui) && !options.valuesOf(nonOptions).contains("nogui")` is the GUI condition.
    pub fn wants_gui(&self) -> bool {
        !self.nogui && !self.non_options.iter().any(|o| o == "nogui")
    }
}

fn split_eq(body: &str) -> (&str, Option<&str>) {
    match body.split_once('=') {
        Some((name, value)) => (name, Some(value)),
        None => (body, None),
    }
}

fn take_value(
    spec: (&'static str, Opt, bool),
    inline: Option<String>,
    args: &[String],
    i: &mut usize,
    out: &mut Collected,
) -> Result<(), OptionException> {
    let value = match inline {
        Some(v) => v,
        None => {
            let v = args.get(*i).ok_or_else(|| OptionException::missing_argument(spec.0))?;
            *i += 1;
            v.clone()
        }
    };
    out.values.push((spec.1, spec.0, value));
    Ok(())
}

fn handle_long(body: &str, args: &[String], i: &mut usize, out: &mut Collected) -> Result<(), OptionException> {
    let (name, value) = split_eq(body);
    let spec = lookup(name).ok_or_else(|| OptionException::unrecognized(name))?;
    if spec.2 {
        take_value(spec, value.map(str::to_owned), args, i, out)
    } else {
        out.flags.push(spec.1);
        Ok(())
    }
}

fn handle_short(body: &str, args: &[String], i: &mut usize, out: &mut Collected) -> Result<(), OptionException> {
    let (name, value) = split_eq(body);
    if lookup(name).is_some() {
        return handle_long(body, args, i, out);
    }
    // a cluster of single-character options (each may itself be an abbreviation)
    let chars: Vec<char> = name.chars().collect();
    for (idx, c) in chars.iter().enumerate() {
        let spec = lookup(&c.to_string()).ok_or_else(|| OptionException::unrecognized(&c.to_string()))?;
        if spec.2 {
            let rest: String = chars[idx + 1..].iter().collect();
            let inline = if !rest.is_empty() { Some(rest) } else { value.map(str::to_owned) };
            return take_value(spec, inline, args, i, out);
        }
        out.flags.push(spec.1);
    }
    Ok(())
}

impl Collected {
    fn into_options(self, non_options: Vec<String>) -> ServerOptions {
        let has = |o: Opt| self.flags.contains(&o);
        let mut options = ServerOptions {
            nogui: has(Opt::Nogui),
            init_settings: has(Opt::InitSettings),
            demo: has(Opt::Demo),
            bonus_chest: has(Opt::BonusChest),
            force_upgrade: has(Opt::ForceUpgrade),
            erase_cache: has(Opt::EraseCache),
            recreate_region_files: has(Opt::RecreateRegionFiles),
            safe_mode: has(Opt::SafeMode),
            help: has(Opt::Help),
            jfr_profile: has(Opt::JfrProfile),
            non_options,
            ..ServerOptions::default()
        };
        // `OptionSet.valueOf`: one value, or MultipleArgumentsForOptionException
        let single = |opt: Opt| -> Result<Option<(&'static str, &String)>, OptionException> {
            let mut it = self.values.iter().filter(|(o, _, _)| *o == opt);
            match (it.next(), it.next()) {
                (Some((_, name, v)), None) => Ok(Some((name, v))),
                (None, _) => Ok(None),
                (Some((_, name, _)), Some(_)) => Err(OptionException {
                    class: "joptsimple.MultipleArgumentsForOptionException",
                    message: format!("Found multiple arguments for option {name}, but you asked for only one"),
                }),
            }
        };
        match single(Opt::PidFile) {
            Ok(Some((_, v))) => options.pid_file = Some(PathBuf::from(v)),
            Ok(None) => {}
            Err(e) => options.early_error = Some(e),
        }
        let mut late = |r: Result<(), OptionException>| {
            if let Err(e) = r {
                options.late_error.get_or_insert(e);
            }
        };
        let mut universe = None;
        let mut world_name = None;
        let mut port = None;
        let mut server_id = None;
        late(single(Opt::Universe).map(|v| universe = v.map(|(_, v)| v.clone())));
        late(single(Opt::World).map(|v| world_name = v.map(|(_, v)| v.clone())));
        late(single(Opt::Port).and_then(|v| {
            port = match v {
                Some((name, text)) => Some(text.parse::<i32>().map_err(|_| OptionException {
                    class: "joptsimple.OptionArgumentConversionException",
                    message: format!("Cannot parse argument '{text}' of option {name}"),
                })?),
                None => None,
            };
            Ok(())
        }));
        late(single(Opt::ServerId).map(|v| server_id = v.map(|(_, v)| v.clone())));
        if let Some(u) = universe {
            options.universe = u;
        }
        options.world_name = world_name;
        if let Some(p) = port {
            options.port = p;
        }
        options.server_id = server_id;
        options
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> ServerOptions {
        ServerOptions::parse(args).unwrap()
    }

    #[test]
    fn defaults() {
        let o = parse(&[]);
        assert_eq!(o, ServerOptions::default());
        assert_eq!(o.universe, ".");
        assert_eq!(o.port, -1);
        assert!(o.wants_gui());
    }

    #[test]
    fn flags_values_and_forms() {
        let o = parse(&["--nogui", "-demo", "--port", "25570", "--world=w", "--universe", "u", "-serverId=x", "--pidFile", "p.pid", "extra"]);
        assert!(o.nogui && o.demo);
        assert_eq!((o.port, o.world_name.as_deref(), o.universe.as_str(), o.server_id.as_deref()), (25570, Some("w"), "u", Some("x")));
        assert_eq!(o.pid_file, Some(PathBuf::from("p.pid")));
        assert_eq!(o.non_options, ["extra"]);
        assert!(!o.wants_gui());
        assert!(parse(&["nogui"]).non_options.contains(&"nogui".to_owned()) && !parse(&["nogui"]).wants_gui());
    }

    #[test]
    fn abbreviations_clusters_and_terminator() {
        assert!(parse(&["--nog", "--init"]).init_settings);
        assert_eq!(parse(&["--po", "1"]).port, 1);
        assert_eq!(parse(&["-w", "name"]).world_name.as_deref(), Some("name"));
        assert!(parse(&["-d"]).demo);
        assert_eq!(parse(&["--", "--bogus", "x"]).non_options, ["--bogus", "x"]);
        assert_eq!(parse(&["-"]).non_options, ["-"]);
        // `--p` is ambiguous (pidFile, port)
        assert_eq!(ServerOptions::parse(["--p"]).unwrap_err().to_string(), "joptsimple.UnrecognizedOptionException: p is not a recognized option");
    }

    #[test]
    fn errors_match_vanilla() {
        let e = |args: &[&str]| ServerOptions::parse(args).unwrap_err().to_string();
        assert_eq!(e(&["--bogus"]), "joptsimple.UnrecognizedOptionException: bogus is not a recognized option");
        assert_eq!(e(&["-x"]), "joptsimple.UnrecognizedOptionException: x is not a recognized option");
        assert_eq!(e(&["--port"]), "joptsimple.OptionMissingRequiredArgumentException: Option port requires an argument");
        assert_eq!(e(&["-w"]), "joptsimple.OptionMissingRequiredArgumentException: Option world requires an argument");
        // conversion problems surface lazily, after the EULA check
        let o = parse(&["--port", "abc"]);
        assert_eq!(o.late_error.unwrap().to_string(), "joptsimple.OptionArgumentConversionException: Cannot parse argument 'abc' of option port");
        let o = parse(&["--world", "a", "--world", "b"]);
        assert!(o.late_error.unwrap().to_string().contains("Found multiple arguments for option world"));
    }

    #[test]
    fn usage_lists_every_option() {
        for (name, _, takes_arg) in OPTIONS {
            let line = USAGE.lines().find(|l| l.starts_with(&format!("--{name}"))).unwrap_or_else(|| panic!("{name} missing from usage"));
            assert_eq!(line[2 + name.len()..].trim_start().starts_with('<'), takes_arg, "{name}");
        }
        assert_eq!(USAGE.lines().filter(|l| l.starts_with("--") && !l.starts_with("---")).count(), OPTIONS.len());
    }
}
