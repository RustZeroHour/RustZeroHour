// SPDX-License-Identifier: GPL-3.0-or-later
// This file is part of RustZeroHour, a derivative of the Zero Hour source
// release. Copyright 2025 Electronic Arts Inc.; Rust port Copyright 2026 the
// RustZeroHour contributors.
//
//! Drives the `rzh-log` macro family and the panic hook through a real
//! subscriber, capturing the formatted output in a temp file — the Rust
//! counterpart of the GoogleTest coverage the original had for its
//! DEBUG_LOG family (Tests/Google/Core/Libraries/Source/Lib, upstream).

use std::fs;
use std::io::{Read, Seek, SeekFrom};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

use rzh_log::init::build_subscriber;
use rzh_log::level;
use rzh_log::writer::FanOutWriter;
use tracing::Level;

/// Runs `body` with a capturing subscriber installed (thread-locally) and
/// returns everything it recorded.
fn with_capture(max_level: Level, body: impl FnOnce()) -> String {
    let path = std::env::temp_dir().join(format!(
        "rzh-log-macros-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let file = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(true)
        .open(&path)
        .expect("create capture file");
    let shared = Arc::new(Mutex::new(file));
    let writer = FanOutWriter::file(Arc::clone(&shared));
    let dispatch = tracing::Dispatch::new(build_subscriber(writer, max_level));

    tracing::dispatcher::with_default(&dispatch, body);

    let mut file = shared.lock().unwrap();
    let _ = file.seek(SeekFrom::Start(0));
    let mut contents = String::new();
    let _ = file.read_to_string(&mut contents);
    drop(file);
    let _ = fs::remove_file(&path);
    contents
}

#[test]
fn logging_macros_emit_and_gate_like_the_original() {
    // The only test in this binary that touches the global level mask,
    // so no locking is needed against parallel siblings.
    let captured = with_capture(Level::DEBUG, || {
        level::set_level_mask(0);

        rzh_log::debug_log!("plain message {}", 42);
        rzh_log::debug_log_raw!("raw is an alias {}", 43);

        // Gated off: the mask is empty.
        rzh_log::debug_log_level!(level::LEVEL_NET, "gated off {}", 44);

        // Gated on: the mask carries the NET bit.
        level::set_level_mask(level::LEVEL_NET);
        rzh_log::debug_log_level!(level::LEVEL_NET, "gated on {}", 45);
        level::set_level_mask(0);

        // DEBUG_ASSERTLOG fires only when the condition is false.
        rzh_log::debug_assertlog!(1 + 1 == 2, "must not appear {}", 46);
        rzh_log::debug_assertlog!(1 + 1 == 3, "assertlog fired {}", 47);
    });

    assert!(captured.contains("plain message 42"), "{captured}");
    assert!(captured.contains("raw is an alias 43"), "{captured}");
    assert!(!captured.contains("gated off"), "{captured}");
    assert!(captured.contains("gated on 45"), "{captured}");
    assert!(!captured.contains("must not appear"), "{captured}");
    assert!(captured.contains("assertlog fired 47"), "{captured}");

    level::set_level_mask(0);
}

#[test]
fn debug_events_are_dropped_at_info_filtering() {
    // The Release-build policy of the original: a subscriber recording
    // only INFO and above drops every DEBUG_LOG event.
    let captured = with_capture(Level::INFO, || {
        rzh_log::debug_log!("must not be recorded");
    });
    assert!(!captured.contains("must not be recorded"), "{captured}");
}

#[test]
fn crash_macros_log_and_panic() {
    let captured = with_capture(Level::ERROR, || {
        let debug = catch_unwind(AssertUnwindSafe(|| {
            rzh_log::debug_crash!("debug crash {}", 100);
        }));
        assert!(debug.is_err(), "debug_crash! must always panic");

        let release = catch_unwind(AssertUnwindSafe(|| {
            rzh_log::release_crash!("release crash {}", 101);
        }));
        assert!(release.is_err(), "release_crash! must always panic");

        let assert_crash = catch_unwind(AssertUnwindSafe(|| {
            rzh_log::debug_assertcrash!(1 + 1 == 3, "assertcrash fired {}", 102);
        }));
        assert!(assert_crash.is_err(), "debug_assertcrash! must panic");

        let assert_ok = catch_unwind(AssertUnwindSafe(|| {
            rzh_log::debug_assertcrash!(1 + 1 == 2, "assertcrash passed {}", 103);
        }));
        assert!(assert_ok.is_ok(), "a passing assertcrash must not panic");
    });

    assert!(captured.contains("debug crash 100"), "{captured}");
    assert!(captured.contains("release crash 101"), "{captured}");
    assert!(captured.contains("assertcrash fired 102"), "{captured}");
    assert!(!captured.contains("assertcrash passed"), "{captured}");
}

#[test]
fn the_panic_hook_logs_message_and_location() {
    rzh_log::panic::install();

    let captured = with_capture(Level::ERROR, || {
        let result = catch_unwind(AssertUnwindSafe(|| {
            panic!("hook test boom");
        }));
        assert!(result.is_err());
    });

    assert!(captured.contains("panic: hook test boom"), "{captured}");
    // The location line points at this file, like a stack dump would
    // (Windows locations carry backslashes — normalize before matching).
    let captured_norm = captured.replace('\\', "/");
    assert!(captured_norm.contains("tests/macros.rs"), "{captured}");
    // The backtrace is captured even without RUST_BACKTRACE set.
    assert!(
        captured.contains("panicked at")
            || captured.contains("stack backtrace")
            || captured.contains("rzh_log"),
        "expected backtrace or frames in: {captured}"
    );
}
