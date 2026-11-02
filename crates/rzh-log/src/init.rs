// SPDX-License-Identifier: GPL-3.0-or-later
// This file is part of RustZeroHour, a derivative of the Zero Hour source
// release. Copyright 2025 Electronic Arts Inc.; Rust port Copyright 2026 the
// RustZeroHour contributors.
//
// Re-implements the GameDebugInit wiring of Core/GameEngine/Source/Common/
// System/Debug.cpp at baseline 571a7c5.
//
//! Subscriber setup: the `GameDebugInit` equivalent.
//!
//! [`init`] composes the pieces the original wired together: it opens
//! (and rotates) the log file, installs a `tracing` fmt subscriber that
//! writes to the file and/or stderr, installs the [`crate::panic`]
//! hook and writes the "Log opened" event into the fresh file.
//!
//! The command-line parsing the original did here (`-logFile` suffixes,
//! the multi-instance `_InstanceNN` suffix) belongs to the executable;
//! it fills in a [`Config`] instead.
//!
//! # Example
//!
//! ```no_run
//! # fn main() -> Result<(), rzh_log::InitError> {
//! let info = rzh_log::init(&rzh_log::Config::new("RustZeroHour"))?;
//! if let Some(paths) = &info.log_paths {
//!     println!("logging to {}", paths.current.display());
//! }
//! # Ok(())
//! # }
//! ```

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use tracing::Level;

use crate::logfile::{self, LogFilePaths};
use crate::panic;
use crate::writer::FanOutWriter;

/// Errors from [`init`].
#[derive(Debug)]
pub enum InitError {
    /// The log file could not be opened or rotated.
    Io(std::io::Error),
    /// A global subscriber is already installed: `init` works once per
    /// process, like the original's one-shot `GameDebugInit`.
    AlreadyInitialized,
}

impl std::fmt::Display for InitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(error) => write!(f, "log file error: {error}"),
            Self::AlreadyInitialized => {
                write!(f, "a global tracing subscriber is already installed")
            }
        }
    }
}

impl std::error::Error for InitError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            Self::AlreadyInitialized => None,
        }
    }
}

impl From<std::io::Error> for InitError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

/// The logging configuration, the parts `GameDebugInit` derived from the
/// executable and the command line.
#[derive(Debug, Clone)]
pub struct Config {
    /// Directory the log files live in — the executable's directory in
    /// the original (defaults to the current executable's directory).
    pub dir: PathBuf,
    /// The application prefix in the file names (`gAppPrefix`, which let
    /// "WB" builds pick a different log name in the original).
    pub prefix: String,
    /// The per-instance suffix, `_InstanceNN` when several clients run.
    pub suffix: String,
    /// Whether to open the rotating log file at all.
    pub file: bool,
    /// Whether to mirror events to stderr (the `OutputDebugString`
    /// stand-in).
    pub stderr: bool,
    /// The maximum level recorded by the subscriber; the build-variant
    /// policy of the original (Debug/Profile log, Release does not).
    pub max_level: Level,
}

impl Config {
    /// A configuration with the original's defaults: `prefix` in the
    /// executable's directory, both sinks, everything down to `DEBUG`
    /// recorded.
    #[must_use]
    pub fn new(prefix: impl Into<String>) -> Self {
        Self {
            dir: default_dir(),
            prefix: prefix.into(),
            suffix: String::new(),
            file: true,
            stderr: true,
            max_level: Level::DEBUG,
        }
    }

    /// Overrides the log directory.
    #[must_use]
    pub fn dir(mut self, dir: impl Into<PathBuf>) -> Self {
        self.dir = dir.into();
        self
    }

    /// Sets the per-instance suffix.
    #[must_use]
    pub fn suffix(mut self, suffix: impl Into<String>) -> Self {
        self.suffix = suffix.into();
        self
    }

    /// Enables or disables the log-file sink.
    #[must_use]
    pub fn file(mut self, file: bool) -> Self {
        self.file = file;
        self
    }

    /// Enables or disables the stderr sink.
    #[must_use]
    pub fn stderr(mut self, stderr: bool) -> Self {
        self.stderr = stderr;
        self
    }

    /// Sets the maximum recorded level.
    #[must_use]
    pub fn max_level(mut self, max_level: Level) -> Self {
        self.max_level = max_level;
        self
    }
}

/// The executable's directory, or the working directory when the
/// executable path cannot be determined (the original fell back to an
/// empty prefix in that case, landing the files in the working
/// directory too).
fn default_dir() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|parent| parent.to_path_buf()))
        .unwrap_or_default()
}

/// What [`init`] set up.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InitInfo {
    /// The log file pair when file logging is on.
    pub log_paths: Option<LogFilePaths>,
}

/// Builds the fmt subscriber over a writer — exposed for tests and for
/// binaries that compose their own subscriber stack.
#[must_use]
pub fn build_subscriber(
    writer: FanOutWriter,
    max_level: Level,
) -> impl tracing::Subscriber + Send + Sync + 'static {
    tracing_subscriber::fmt()
        .with_max_level(max_level)
        .with_writer(writer)
        .finish()
}

/// Initializes logging: log file, subscriber and panic hook.
///
/// Call once, early in `main` (a second call returns
/// [`InitError::AlreadyInitialized`] without touching anything).
///
/// # Errors
///
/// - [`InitError::Io`] when the log file cannot be opened or rotated;
/// - [`InitError::AlreadyInitialized`] when a global subscriber exists.
pub fn init(config: &Config) -> Result<InitInfo, InitError> {
    let mut log_paths = None;
    let writer = if config.file {
        let (file, paths) = logfile::open_log_file(
            &config.dir,
            &config.prefix,
            &config.suffix,
            cfg!(debug_assertions),
        )?;
        log_paths = Some(paths);
        let shared = Arc::new(Mutex::new(file));
        if config.stderr {
            FanOutWriter::file_and_stderr(shared)
        } else {
            FanOutWriter::file(shared)
        }
    } else if config.stderr {
        FanOutWriter::stderr()
    } else {
        FanOutWriter::none()
    };

    let subscriber = build_subscriber(writer, config.max_level);
    tracing::subscriber::set_global_default(subscriber)
        .map_err(|_| InitError::AlreadyInitialized)?;

    panic::install();

    // The original's DebugLog("Log %s opened: %s", name, time): the fmt
    // layer prefixes its own timestamp, so the event carries the path.
    if let Some(paths) = &log_paths {
        tracing::info!(target: "rzh_log", "Log {} opened", paths.current.display());
    }

    Ok(InitInfo { log_paths })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_the_original_wiring() {
        let config = Config::new("Generals");
        assert_eq!(config.prefix, "Generals");
        assert_eq!(config.suffix, "");
        assert!(config.file);
        assert!(config.stderr);
        assert_eq!(config.max_level, Level::DEBUG);
    }

    #[test]
    fn builders_override_every_field() {
        let config = Config::new("X")
            .dir("/tmp")
            .suffix("_Instance02")
            .file(false)
            .stderr(false)
            .max_level(Level::WARN);
        assert_eq!(config.dir, PathBuf::from("/tmp"));
        assert_eq!(config.suffix, "_Instance02");
        assert!(!config.file);
        assert!(!config.stderr);
        assert_eq!(config.max_level, Level::WARN);
    }

    #[test]
    fn init_opens_the_log_file_and_reports_its_paths() {
        // The one test allowed to call init(): the global subscriber is
        // process-wide. stderr stays off so the parallel test runners
        // stay quiet.
        let dir = std::env::temp_dir().join(format!(
            "rzh-log-init-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).expect("create dir");

        let info = init(&Config::new("InitTest").dir(&dir).stderr(false)).expect("init");

        let paths = info.log_paths.expect("log paths");
        assert!(
            paths.current.ends_with("InitTestDebugLogFileD.txt")
                || paths.current.ends_with("InitTestDebugLogFile.txt"),
            "unexpected current path: {}",
            paths.current.display()
        );
        assert!(paths.current.is_file(), "log file must exist");

        // A second init is rejected without breaking the first.
        let again = init(&Config::new("InitTest").dir(&dir).stderr(false));
        assert!(matches!(again, Err(InitError::AlreadyInitialized)));

        let _ = std::fs::remove_dir_all(&dir);
    }
}
