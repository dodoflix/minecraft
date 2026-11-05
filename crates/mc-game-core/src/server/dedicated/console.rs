//! The console reader (`DedicatedServer.initServer`'s `Server console handler` thread) and the queue the game thread
//! drains in `handleConsoleInputs`.

use std::io::{BufRead, BufReader, Read};
use std::sync::Arc;

use parking_lot::Mutex;

use super::stop::ServerStopSignal;

/// Lines typed on the console, in order. Cheap to clone; the reader thread pushes, the game thread drains.
#[derive(Clone, Default)]
pub struct ConsoleInputQueue(Arc<Mutex<Vec<String>>>);

impl ConsoleInputQueue {
    pub fn new() -> ConsoleInputQueue {
        ConsoleInputQueue::default()
    }

    pub fn push(&self, line: String) {
        self.0.lock().push(line);
    }

    /// Everything queued so far, oldest first.
    pub fn drain(&self) -> Vec<String> {
        std::mem::take(&mut *self.0.lock())
    }
}

/// `BufferedReader.readLine` over UTF-8: `\n`, `\r\n` and a lone `\r` end a line; the terminator is dropped.
fn read_java_line(reader: &mut impl BufRead) -> std::io::Result<Option<String>> {
    let mut bytes = Vec::new();
    loop {
        let buf = reader.fill_buf()?;
        if buf.is_empty() {
            return Ok(if bytes.is_empty() { None } else { Some(String::from_utf8_lossy(&bytes).into_owned()) });
        }
        match buf.iter().position(|&b| b == b'\n' || b == b'\r') {
            Some(i) => {
                bytes.extend_from_slice(&buf[..i]);
                let cr = buf[i] == b'\r';
                reader.consume(i + 1);
                if cr && reader.fill_buf()?.first() == Some(&b'\n') {
                    reader.consume(1);
                }
                return Ok(Some(String::from_utf8_lossy(&bytes).into_owned()));
            }
            None => {
                let n = buf.len();
                bytes.extend_from_slice(buf);
                reader.consume(n);
            }
        }
    }
}

/// Read lines from `input` into `queue` until EOF, an I/O error, or `stop` is requested (checked between lines, like
/// vanilla's `isStopped()/isRunning()` loop condition).
pub fn run_console_reader(input: impl Read, queue: &ConsoleInputQueue, stop: &ServerStopSignal) {
    let mut reader = BufReader::new(input);
    while !stop.is_stop_requested() {
        match read_java_line(&mut reader) {
            Ok(Some(line)) => queue.push(line),
            Ok(None) => break,
            Err(e) => {
                log::error!("Exception handling console input\n{e}");
                break;
            }
        }
    }
}

/// Start the `Server console handler` thread reading stdin.
pub fn spawn_console_thread(queue: ConsoleInputQueue, stop: ServerStopSignal) -> std::io::Result<std::thread::JoinHandle<()>> {
    std::thread::Builder::new().name("Server console handler".to_owned()).spawn(move || run_console_reader(std::io::stdin(), &queue, &stop))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_lines_like_java() {
        let queue = ConsoleInputQueue::new();
        let stop = ServerStopSignal::new();
        run_console_reader("say hi\r\nstop\nlast\rno newline".as_bytes(), &queue, &stop);
        assert_eq!(queue.drain(), ["say hi", "stop", "last", "no newline"]);
        assert!(queue.drain().is_empty());
    }

    #[test]
    fn stops_reading_when_signalled() {
        let queue = ConsoleInputQueue::new();
        let stop = ServerStopSignal::new();
        stop.request_stop();
        run_console_reader("a\nb\n".as_bytes(), &queue, &stop);
        assert!(queue.drain().is_empty());
    }
}
