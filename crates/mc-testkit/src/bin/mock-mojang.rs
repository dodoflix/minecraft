//! Runs the mock Mojang services until killed.
//!
//!     mock-mojang [--port N] [--log FILE]
//!
//! The first output line is the discovery URL (`-Dminecraft.api.discovery.host=` for the client and the vanilla server,
//! `MINECRAFT_API_DISCOVERY_HOST` for our server). `--log` appends one JSON object per request.
use mc_testkit::MockMojang;
use std::io::Write;
use std::path::PathBuf;

fn main() {
    let (mut port, mut log) = (0u16, None::<PathBuf>);
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        let value = args.next();
        match (arg.as_str(), value) {
            ("--port", Some(v)) if v.parse::<u16>().is_ok() => port = v.parse().unwrap_or(0),
            ("--log", Some(v)) => log = Some(v.into()),
            _ => fail("usage: mock-mojang [--port N] [--log FILE]"),
        }
    }
    let mock = MockMojang::bind(port, log.as_deref())
        .unwrap_or_else(|e| fail(&format!("mock-mojang: {e}")));
    let mut out = std::io::stdout();
    if writeln!(out, "{}", mock.discovery_url())
        .and_then(|()| out.flush())
        .is_err()
    {
        return;
    }
    loop {
        std::thread::park();
    }
}

fn fail(message: &str) -> ! {
    let _ = writeln!(std::io::stderr(), "{message}");
    std::process::exit(2);
}
