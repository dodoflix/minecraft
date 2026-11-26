//! The headless game test server (`net.minecraft.gametest.Main`): `mc-gametest --tests <selector> [--universe <dir>]
//! [--report <file>] [--repeatCount <n>] [--verify <bool>] [--packs <dir>]`. The exit code is the number of failed
//! required tests.

use std::process::ExitCode;

use log::LevelFilter;
use mc_game::gametest::game_test_main_util::run_game_test_server;
use mc_game::server::dedicated::logging;

fn main() -> ExitCode {
    // vanilla runs `Main.main` on a thread called `ServerMain`; the log lines carry that name
    let main_thread = std::thread::Builder::new()
        .name("ServerMain".to_owned())
        .spawn(|| {
            if let Err(e) = logging::init("logs", LevelFilter::Info) {
                #[expect(
                    clippy::print_stderr,
                    reason = "a logging-init failure goes to stderr before the logger exists"
                )]
                {
                    eprintln!("Failed to initialize logging: {e}");
                }
            }
            let args: Vec<String> = std::env::args().skip(1).collect();
            run_game_test_server(&args)
        });
    match main_thread.map(|t| t.join()) {
        Ok(Ok(0)) => ExitCode::SUCCESS,
        Ok(Ok(code)) => ExitCode::from(code as u8),
        _ => ExitCode::FAILURE,
    }
}
