//! `FileFixException` and its subclasses, `FileSystemCapabilities` and `FileMove`.

use std::fmt;
use std::io;
use std::path::PathBuf;

use mc_core::java_exception::JavaException;

/// `FileMove`: one file moved from `from` to `to`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileMove {
    pub from: PathBuf,
    pub to: PathBuf,
}

/// `FileSystemCapabilities`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FileSystemCapabilities {
    pub atomic_move: bool,
    pub hard_links: bool,
}

impl fmt::Display for FileSystemCapabilities {
    /// The `record` `toString`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "FileSystemCapabilities[atomicMove={}, hardLinks={}]",
            self.atomic_move, self.hard_links
        )
    }
}

/// A `CrashReportCategory`: a name and its details in order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CrashReportCategory {
    pub name: String,
    pub details: Vec<(String, String)>,
}

/// What `FileFixException.makeReportedException` puts into its crash report.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileFixCrashReport {
    pub title: &'static str,
    pub categories: Vec<CrashReportCategory>,
}

/// `Throwable.toString()` of the Java exception behind `e`: its class name, then the message. An error with a
/// payload that no Java exception stands for prints as its message alone.
pub(super) fn java_to_string(e: &io::Error) -> String {
    match e.get_ref() {
        Some(inner) if !inner.is::<JavaException>() => e.to_string(),
        _ => JavaException::of_io(e).to_string(),
    }
}

/// What stops a file fix. The variants are the `FileFixException` subclasses; [`FileFixError::Io`] is the plain
/// `IOException` / `UncheckedIOException` of the steps around them.
#[derive(Debug)]
pub enum FileFixError {
    /// `AbortedFileFixException`: the world was left as it was, except for the moves in `not_reverted_moves`.
    Aborted {
        cause: io::Error,
        not_reverted_moves: Vec<FileMove>,
        capabilities: Option<FileSystemCapabilities>,
    },
    /// `AtomicMoveNotSupportedFileFixException`.
    AtomicMoveNotSupported {
        capabilities: FileSystemCapabilities,
    },
    /// `CanceledFileFixException`.
    Canceled,
    /// `FailedCleanupFileFixException`: the upgraded world is complete but could not be moved into place; it
    /// waits in the folder `new_world_folder_name`.
    FailedCleanup {
        cause: io::Error,
        new_world_folder_name: String,
        capabilities: FileSystemCapabilities,
    },
    /// An `IOException` (or `UncheckedIOException`) of a step: before the swap the world is as it was.
    Io(io::Error),
    /// A `RuntimeException` that is not a `FileFixException` (a journal that cannot be parsed, a wrong number of
    /// files): not an I/O failure, so it is neither reported as one nor wrapped.
    Unchecked(String),
}

impl FileFixError {
    /// `new AbortedFileFixException(cause)`.
    pub fn aborted(cause: io::Error) -> FileFixError {
        FileFixError::Aborted {
            cause,
            not_reverted_moves: Vec::new(),
            capabilities: None,
        }
    }

    fn capabilities(&self) -> Option<FileSystemCapabilities> {
        match self {
            FileFixError::Aborted { capabilities, .. } => *capabilities,
            FileFixError::AtomicMoveNotSupported { capabilities }
            | FileFixError::FailedCleanup { capabilities, .. } => Some(*capabilities),
            FileFixError::Canceled | FileFixError::Io(_) | FileFixError::Unchecked(_) => None,
        }
    }

    /// `createCrashReport`: `None` for [`FileFixError::Io`], which is not a `FileFixException`.
    pub fn crash_report(&self) -> Option<FileFixCrashReport> {
        if matches!(self, FileFixError::Io(_) | FileFixError::Unchecked(_)) {
            return None;
        }
        let capability = |value: Option<bool>| value.map_or("null".to_owned(), |v| v.to_string());
        let capabilities = self.capabilities();
        let mut categories = vec![CrashReportCategory {
            name: "File system capabilities".to_owned(),
            details: vec![
                (
                    "Hard Links".to_owned(),
                    capability(capabilities.map(|c| c.hard_links)),
                ),
                (
                    "Atomic Move".to_owned(),
                    capability(capabilities.map(|c| c.atomic_move)),
                ),
            ],
        }];
        match self {
            FileFixError::Aborted {
                not_reverted_moves, ..
            } => categories.push(CrashReportCategory {
                name: "Moves that failed to revert".to_owned(),
                details: not_reverted_moves
                    .iter()
                    .enumerate()
                    .map(|(i, m)| {
                        (
                            i.to_string(),
                            format!("{} -> {}", m.from.display(), m.to.display()),
                        )
                    })
                    .collect(),
            }),
            FileFixError::FailedCleanup {
                new_world_folder_name,
                ..
            } => categories.push(CrashReportCategory {
                name: "World upgrade".to_owned(),
                details: vec![("New Name".to_owned(), new_world_folder_name.clone())],
            }),
            _ => {}
        }
        Some(FileFixCrashReport {
            title: "Upgrading world failed with errors",
            categories,
        })
    }
}

impl fmt::Display for FileFixError {
    /// `Throwable.toString()` of the Java exception: its class name, then the cause.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FileFixError::Aborted { cause, .. } => {
                write!(
                    f,
                    "net.minecraft.util.filefix.AbortedFileFixException: {}",
                    java_to_string(cause)
                )
            }
            FileFixError::FailedCleanup { cause, .. } => {
                write!(
                    f,
                    "net.minecraft.util.filefix.FailedCleanupFileFixException: {}",
                    java_to_string(cause)
                )
            }
            FileFixError::AtomicMoveNotSupported { .. } => {
                f.write_str("net.minecraft.util.filefix.AtomicMoveNotSupportedFileFixException")
            }
            FileFixError::Canceled => {
                f.write_str("net.minecraft.util.filefix.CanceledFileFixException")
            }
            FileFixError::Io(e) => write!(f, "{e}"),
            FileFixError::Unchecked(message) => f.write_str(message),
        }
    }
}

impl std::error::Error for FileFixError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            FileFixError::Aborted { cause, .. }
            | FileFixError::FailedCleanup { cause, .. }
            | FileFixError::Io(cause) => Some(cause),
            FileFixError::AtomicMoveNotSupported { .. }
            | FileFixError::Canceled
            | FileFixError::Unchecked(_) => None,
        }
    }
}

impl From<io::Error> for FileFixError {
    fn from(e: io::Error) -> Self {
        FileFixError::Io(e)
    }
}
