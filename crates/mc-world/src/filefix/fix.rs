//! `FileFix`: one registered file fix, the operations of a schema version run against the [`CowTree`].

use std::path::Path;

use super::cow_tree::CowTree;
use super::error::FileFixError;
use super::ops::{ContentContext, FileFixOperation, ModifyContent};
use super::progress::UpgradeProgress;

pub struct FileFix {
    version: i32,
    file_fix_operations: Vec<FileFixOperation>,
}

impl FileFix {
    /// A file fix of the schema `version` (`getVersion()`), still without operations (`makeFixer()` adds them).
    pub fn new(version: i32) -> FileFix {
        FileFix {
            version,
            file_fix_operations: Vec::new(),
        }
    }

    /// `getVersion`.
    pub fn version(&self) -> i32 {
        self.version
    }

    /// `addFileFixOperation`.
    pub fn add_file_fix_operation(&mut self, operation: FileFixOperation) {
        self.file_fix_operations.push(operation);
    }

    /// `addFileContentFix`: the files are read and written at this fix's version.
    pub fn add_file_content_fix(
        &mut self,
        fix_function: impl Fn(&mut ContentContext<'_>) -> Result<(), FileFixError>
        + Send
        + Sync
        + 'static,
    ) {
        self.file_fix_operations
            .push(FileFixOperation::ModifyContent(ModifyContent::new(
                self.version,
                fix_function,
            )));
    }

    /// `countFileOperations`.
    pub fn count_file_operations(&self) -> usize {
        self.file_fix_operations.len()
    }

    /// `runFixOperations`: the operations in order, the cancellation checked before each one.
    pub fn run_fix_operations(
        &self,
        tree: &mut CowTree,
        base_folder: &Path,
        upgrade_progress: &UpgradeProgress,
    ) -> Result<(), FileFixError> {
        for operation in &self.file_fix_operations {
            if upgrade_progress.is_canceled() {
                return Err(FileFixError::Canceled);
            }
            operation.fix(tree, base_folder, upgrade_progress)?;
            upgrade_progress.increment_finished_operations();
        }
        Ok(())
    }
}
