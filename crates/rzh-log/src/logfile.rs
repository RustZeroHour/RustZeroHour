// SPDX-License-Identifier: GPL-3.0-or-later
// This file is part of RustZeroHour, a derivative of the Zero Hour source
// release. Copyright 2025 Electronic Arts Inc.; Rust port Copyright 2026 the
// RustZeroHour contributors.
//
// Re-implements the log-file management of Core/Libraries/Source/Lib/
// Debug.cpp (`DebugOpenLogFile`) at baseline 571a7c5.
//
//! The rotating debug log file: `DebugOpenLogFile` semantics.
//!
//! The original named two files next to the executable:
//!
//! - current: `<dir>/<prefix>DebugLogFile<suffix>.txt`
//! - previous: `<dir>/<prefix>DebugLogFilePrev<suffix>.txt`
//!
//! with a `D` appended to the `DebugLogFile`/`DebugLogFilePrev` stems in
//! the Debug library variant, and `<suffix>` the per-instance marker
//! (`_InstanceNN` when more than one client runs). On open it deleted
//! the previous file, renamed the current file onto the previous name
//! (falling back to deleting the current file when the rename failed —
//! both with warnings) and truncated the current file for writing.
//!
//! The re-implementation keeps the names, the rotation order and the
//! warnings; the warnings go through `tracing` and are therefore only
//! visible once a subscriber is installed ([`crate::init`] rotates
//! before installing one, mirroring the original's ordering).
//!
//! ```
//! use std::path::Path;
//! use rzh_log::logfile;
//!
//! # fn main() -> std::io::Result<()> {
//! let dir = std::env::temp_dir().join(format!("rzh-log-doc-{}", std::process::id()));
//! std::fs::create_dir_all(&dir)?;
//! let (file, paths) = logfile::open_log_file(&dir, "RzhProbe", "", false)?;
//! # let _ = file;
//! assert!(paths.current.ends_with("RzhProbeDebugLogFile.txt"));
//! assert!(paths.previous.ends_with("RzhProbeDebugLogFilePrev.txt"));
//! # std::fs::remove_dir_all(&dir).ok();
//! # Ok(())
//! # }
//! ```

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// The current and previous log file paths, the `DebugGetLogFileName`
/// and `DebugGetLogFileNamePrev` queries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogFilePaths {
    /// The file this run writes to.
    pub current: PathBuf,
    /// The renamed log of the previous run.
    pub previous: PathBuf,
}

/// The `DebugLogFile` / `DebugLogFilePrev` stems, with the debug-library
/// variant's `D` suffix (`DEBUG_FILE_NAME` in the original).
fn stems(debug_build: bool) -> (&'static str, &'static str) {
    if debug_build {
        ("DebugLogFileD", "DebugLogFilePrevD")
    } else {
        ("DebugLogFile", "DebugLogFilePrev")
    }
}

/// Builds the log file names for a configuration without touching the
/// file system (useful for callers that only want to report paths).
#[must_use]
pub fn log_file_paths(dir: &Path, prefix: &str, suffix: &str, debug_build: bool) -> LogFilePaths {
    let (current_stem, previous_stem) = stems(debug_build);
    LogFilePaths {
        current: dir.join(format!("{prefix}{current_stem}{suffix}.txt")),
        previous: dir.join(format!("{prefix}{previous_stem}{suffix}.txt")),
    }
}

/// `DebugOpenLogFile(prefix, suffix)`: rotates and (re)creates the log
/// file in `dir`.
///
/// - deletes the *previous* file if it exists;
/// - renames the *current* file onto the previous name; when that fails
///   (and the current file exists) it deletes the current file instead —
///   both paths log a warning through `tracing`, exactly like the
///   original's `DebugLog("Warning: ...")` calls;
/// - opens the current file for writing, truncated.
///
/// Returns the fresh file plus both paths. The "Log opened" event that
/// the original wrote into the new file is emitted by [`crate::init`],
/// which owns the subscriber.
///
/// # Errors
///
/// Propagates file-system errors from the final open (and from deleting
/// the previous file — the original ignored that one, but a permission
/// problem here predicts the same problem for the open, so failing
/// early is more honest).
pub fn open_log_file(
    dir: &Path,
    prefix: &str,
    suffix: &str,
    debug_build: bool,
) -> io::Result<(fs::File, LogFilePaths)> {
    let paths = log_file_paths(dir, prefix, suffix, debug_build);

    // remove(theLogFileNamePrev)
    match fs::remove_file(&paths.previous) {
        Ok(()) => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }

    // rename(theLogFileName, theLogFileNamePrev)
    match fs::rename(&paths.current, &paths.previous) {
        Ok(()) => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => {
            tracing::warn!(
                "Warning: Could not rename buffer file '{}' to '{}'. Will remove instead",
                paths.current.display(),
                paths.previous.display()
            );
            // remove(theLogFileName)
            match fs::remove_file(&paths.current) {
                Ok(()) => {}
                Err(remove_error) if remove_error.kind() == io::ErrorKind::NotFound => {}
                Err(remove_error) => {
                    tracing::warn!(
                        "Warning: Failed to remove file '{}': {remove_error}",
                        paths.current.display()
                    );
                    let _ = error; // both attempts failed; the open below decides
                }
            }
        }
    }

    // fopen(theLogFileName, "w")
    let file = fs::File::options()
        .write(true)
        .create(true)
        .truncate(true)
        .open(&paths.current)?;

    Ok((file, paths))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    /// A fresh directory per test, cleaned up best-effort on drop.
    struct TempDir(PathBuf);

    impl TempDir {
        fn new(tag: &str) -> Self {
            let path = std::env::temp_dir().join(format!(
                "rzh-log-test-{tag}-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            fs::create_dir_all(&path).expect("create temp dir");
            Self(path)
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn names_match_the_original_layout() {
        let paths = log_file_paths(Path::new("/game"), "Generals", "", false);
        assert_eq!(
            paths.current,
            Path::new("/game").join("GeneralsDebugLogFile.txt")
        );
        assert_eq!(
            paths.previous,
            Path::new("/game").join("GeneralsDebugLogFilePrev.txt")
        );

        // The debug library variant appends D to both stems.
        let paths = log_file_paths(Path::new("/game"), "Generals", "", true);
        assert_eq!(
            paths.current,
            Path::new("/game").join("GeneralsDebugLogFileD.txt")
        );
        assert_eq!(
            paths.previous,
            Path::new("/game").join("GeneralsDebugLogFilePrevD.txt")
        );

        // The multi-instance suffix the game passes sits between stem and
        // extension.
        let paths = log_file_paths(Path::new("/game"), "Generals", "_Instance02", false);
        assert_eq!(
            paths.current,
            Path::new("/game").join("GeneralsDebugLogFile_Instance02.txt")
        );
    }

    #[test]
    fn first_open_creates_an_empty_log() {
        let dir = TempDir::new("first");

        let (mut file, paths) = open_log_file(&dir.0, "RzhTest", "", false).expect("first open");
        writeln!(file, "hello").expect("write");

        assert!(paths.current.exists());
        assert!(!paths.previous.exists());
    }

    #[test]
    fn second_open_rotates_the_log_to_prev() {
        let dir = TempDir::new("rotate");

        let (mut file, _) = open_log_file(&dir.0, "RzhTest", "", false).expect("open 1");
        writeln!(file, "run one").expect("write 1");
        drop(file);

        let (mut file, paths) = open_log_file(&dir.0, "RzhTest", "", false).expect("open 2");
        writeln!(file, "run two").expect("write 2");
        drop(file);

        let prev = fs::read_to_string(&paths.previous).expect("read prev");
        assert_eq!(prev, "run one\n");
        let cur = fs::read_to_string(&paths.current).expect("read current");
        assert_eq!(cur, "run two\n");
    }

    #[test]
    fn third_open_discards_the_oldest_run() {
        let dir = TempDir::new("rotate3");

        for run in 1..=3 {
            let (mut file, _) = open_log_file(&dir.0, "RzhTest", "", false).expect("open");
            writeln!(file, "run {run}").expect("write");
        }

        let paths = log_file_paths(&dir.0, "RzhTest", "", false);
        assert_eq!(
            fs::read_to_string(&paths.previous).expect("prev"),
            "run 2\n"
        );
        assert_eq!(
            fs::read_to_string(&paths.current).expect("current"),
            "run 3\n"
        );
    }

    #[test]
    fn a_stray_prev_file_is_deleted_on_open() {
        let dir = TempDir::new("stray");
        let paths = log_file_paths(&dir.0, "RzhTest", "", false);
        fs::write(&paths.previous, "ancient").expect("write stray prev");

        let (_file, _paths) = open_log_file(&dir.0, "RzhTest", "", false).expect("open");

        // The stray previous log is gone; the fresh current file exists.
        assert!(!paths.previous.exists());
        assert!(paths.current.exists());
    }
}
