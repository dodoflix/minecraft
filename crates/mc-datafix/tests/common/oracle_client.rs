//! Client of the data fixer oracle (`scripts/datafix-oracle.sh`, `tests/oracle/DataFixerOracle.java`): a batch JVM that
//! runs vanilla's `DataFixers` on NBT inputs, and one-shot verbs. One JVM per test process.

pub use crate::compare::parse_record;
use crate::compare::{Outcome, Request};
use base64::Engine;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::{Mutex, MutexGuard, OnceLock, PoisonError};

pub fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn command(args: &[&str]) -> Command {
    let root = repo_root();
    assert!(
        root.join("ref/bundle/META-INF").is_dir(),
        "ref/ is missing: run scripts/setup-ref.sh"
    );
    let mut cmd = Command::new(root.join("scripts/datafix-oracle.sh"));
    cmd.args(args).current_dir(&root);
    cmd
}

/// Runs a one-shot verb (`fixes-list`, `schema-dump ..`) and returns its stdout; a failure shows the script's stderr.
pub fn run_verb(args: &[&str]) -> String {
    let mut cmd = command(args);
    cmd.stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let out = mc_testkit::spawn_in_group(cmd)
        .unwrap_or_else(|e| panic!("cannot start scripts/datafix-oracle.sh: {e}"))
        .wait_with_output()
        .expect("wait for the oracle");
    assert!(
        out.status.success(),
        "scripts/datafix-oracle.sh {} failed ({}): {}",
        args.join(" "),
        out.status,
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).expect("the oracle prints UTF-8")
}

/// The batch JVM: one request line in, one record line out.
pub struct Oracle {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
}

impl Oracle {
    fn start() -> Oracle {
        let mut cmd = command(&["batch", "-", "-"]);
        cmd.stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit());
        let mut child = mc_testkit::spawn_in_group(cmd)
            .unwrap_or_else(|e| panic!("cannot start scripts/datafix-oracle.sh: {e}"));
        let stdin = child.stdin.take().expect("piped stdin");
        let stdout = BufReader::new(child.stdout.take().expect("piped stdout"));
        Oracle {
            child,
            stdin,
            stdout,
        }
    }

    /// Vanilla's result for one request.
    pub fn update(&mut self, request: &Request) -> Outcome {
        let bytes = mc_nbt::nbt_io::to_bytes(&request.input);
        let line = format!(
            "{}\t{}\t{}\t{}\t{}\n",
            request.id,
            request.ty,
            request.from,
            request.to,
            base64::engine::general_purpose::STANDARD.encode(bytes)
        );
        self.exchange(&request.id, &line)
    }

    /// Sends one request line (with its trailing LF) and reads the record of `id` back.
    pub fn exchange(&mut self, id: &str, line: &str) -> Outcome {
        self.stdin
            .write_all(line.as_bytes())
            .and_then(|()| self.stdin.flush())
            .unwrap_or_else(|e| self.died(&format!("writing a request failed: {e}")));
        let mut record = String::new();
        match self.stdout.read_line(&mut record) {
            Ok(n) if n > 0 => {}
            _ => self.died("no record came back"),
        }
        let (answered, outcome) = parse_record(record.trim_end_matches('\n'));
        assert_eq!(answered, id, "the oracle answered another request");
        outcome
    }

    pub fn batch(&mut self, requests: &[Request]) -> Vec<Outcome> {
        requests.iter().map(|r| self.update(r)).collect()
    }

    fn died(&mut self, why: &str) -> ! {
        let status = self.child.try_wait().ok().flatten();
        panic!(
            "the oracle process is gone ({why}; exit status {status:?}): run `scripts/datafix-oracle.sh fixes-list` to see the error"
        )
    }
}

impl Drop for Oracle {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

static ORACLE: OnceLock<Mutex<Oracle>> = OnceLock::new();

/// The batch JVM of this process, started on first use; tests take turns.
pub fn oracle() -> MutexGuard<'static, Oracle> {
    ORACLE
        .get_or_init(|| Mutex::new(Oracle::start()))
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
}
