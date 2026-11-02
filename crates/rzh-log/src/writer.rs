// SPDX-License-Identifier: GPL-3.0-or-later
// This file is part of RustZeroHour, a derivative of the Zero Hour source
// release. Copyright 2025 Electronic Arts Inc.; Rust port Copyright 2026 the
// RustZeroHour contributors.
//
//! The multi-sink writer the fmt subscriber formats into.
//!
//! The original wrote each log line to the log file and to the Windows
//! debug output (`OutputDebugString`, the `debug_io_ods` module); the
//! re-implementation fans out to an optional shared log file and an
//! optional stderr stream. stderr is the cross-platform stand-in for
//! `OutputDebugString`.

use std::fs;
use std::io::{self, Write};
use std::sync::{Arc, Mutex};

use tracing_subscriber::fmt::MakeWriter;

/// A `MakeWriter` that writes every event to the log file and/or stderr.
///
/// The file is shared behind a `Mutex` so that all events — including
/// those from the panic hook — serialize into one stream, mirroring the
/// original's single `FILE *theLogFile`.
#[derive(Debug, Clone)]
pub struct FanOutWriter {
    file: Option<Arc<Mutex<fs::File>>>,
    stderr: bool,
}

impl FanOutWriter {
    /// Log file only (the plain `DebugLog` to `theLogFile` case).
    #[must_use]
    pub fn file(file: Arc<Mutex<fs::File>>) -> Self {
        Self {
            file: Some(file),
            stderr: false,
        }
    }

    /// stderr only (the `OutputDebugString` case without a log file).
    #[must_use]
    pub fn stderr() -> Self {
        Self {
            file: None,
            stderr: true,
        }
    }

    /// Both sinks, like the original's file + debug-output combination.
    #[must_use]
    pub fn file_and_stderr(file: Arc<Mutex<fs::File>>) -> Self {
        Self {
            file: Some(file),
            stderr: true,
        }
    }

    /// No sinks: events formatted into this writer are dropped.
    ///
    /// Useful for tests that need the event machinery without output.
    #[must_use]
    pub fn none() -> Self {
        Self {
            file: None,
            stderr: false,
        }
    }
}

impl MakeWriter<'_> for FanOutWriter {
    type Writer = Sink;

    fn make_writer(&self) -> Self::Writer {
        Sink {
            file: self.file.clone(),
            stderr: self.stderr,
        }
    }
}

/// The per-event [`io::Write`] that fans out. Errors from the file sink
/// abort the write (a full or unreadable log file must not silently eat
/// diagnostics); errors from stderr are ignored like the original's
/// `OutputDebugString` failures.
pub struct Sink {
    file: Option<Arc<Mutex<fs::File>>>,
    stderr: bool,
}

impl Write for Sink {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if let Some(file) = &self.file {
            // Recover from a poisoned lock: a panic during somebody
            // else's write must not stop the log; the file itself is
            // still usable.
            let mut guard = match file.lock() {
                Ok(guard) => guard,
                Err(poisoned) => poisoned.into_inner(),
            };
            guard.write_all(buf)?;
        }
        if self.stderr {
            let _ = io::stderr().write_all(buf);
        }
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        if let Some(file) = &self.file {
            let mut guard = match file.lock() {
                Ok(guard) => guard,
                Err(poisoned) => poisoned.into_inner(),
            };
            guard.flush()?;
        }
        let _ = io::stderr().flush();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Seek, SeekFrom};

    /// Test-only helper: read a written file back from its start.
    fn read_all(file: &mut fs::File) -> String {
        let _ = file.seek(SeekFrom::Start(0));
        let mut bytes = Vec::new();
        let _ = file.read_to_end(&mut bytes);
        String::from_utf8_lossy(&bytes).into_owned()
    }

    fn shared_buffer_file() -> (Arc<Mutex<fs::File>>, PathBufForTest) {
        // An in-memory file is not available in std, so tests use a temp
        // file and read it back.
        let path = std::env::temp_dir().join(format!(
            "rzh-log-writer-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let file = fs::File::options()
            .read(true)
            .write(true)
            .create(true)
            .truncate(true)
            .open(&path)
            .expect("create temp file");
        (Arc::new(Mutex::new(file)), PathBufForTest(path))
    }

    struct PathBufForTest(std::path::PathBuf);

    impl Drop for PathBufForTest {
        fn drop(&mut self) {
            let _ = fs::remove_file(&self.0);
        }
    }

    #[test]
    fn the_file_sink_receives_the_bytes() {
        let (file, _guard) = shared_buffer_file();
        let mut sink = FanOutWriter::file(Arc::clone(&file)).make_writer();
        sink.write_all(b"line one\n").expect("write");
        sink.flush().expect("flush");
        drop(sink);

        let mut guard = file.lock().unwrap();
        let contents = read_all(&mut guard);
        assert_eq!(contents, "line one\n");
    }

    #[test]
    fn two_sinks_share_one_file_and_interleave_in_order() {
        let (file, _guard) = shared_buffer_file();
        let writer = FanOutWriter::file(Arc::clone(&file));

        let mut a = writer.make_writer();
        let mut b = writer.make_writer();
        a.write_all(b"first\n").expect("write a");
        b.write_all(b"second\n").expect("write b");
        a.flush().expect("flush a");
        b.flush().expect("flush b");
        drop((a, b));

        let mut guard = file.lock().unwrap();
        let contents = read_all(&mut guard);
        assert_eq!(contents, "first\nsecond\n");
    }
}
