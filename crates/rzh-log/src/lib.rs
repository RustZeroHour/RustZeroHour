// SPDX-License-Identifier: GPL-3.0-or-later
// This file is part of RustZeroHour, a derivative of the Zero Hour source
// release. Copyright 2025 Electronic Arts Inc.; Rust port Copyright 2026 the
// RustZeroHour contributors.
//
// Re-implements the diagnostics surface of the following files at baseline
// commit 571a7c5, Zero Hour configuration (see ADR-002):
//
//   Core/Libraries/Source/Lib/Include/Lib/Debug.h
//   Core/Libraries/Source/Lib/Debug.cpp
//   Core/GameEngine/Source/Common/System/Debug.cpp
//   Core/Libraries/Source/debug, profile, MiniDumper (the WWDebug stack)
//
// The originals carry the following notices, preserved here:
//
//   (c) 2003 Electronic Arts. Debugging module. (debug.h, M. Hoffe)
//
// This program is free software: you can redistribute it and/or modify it
// under the terms of the GNU General Public License as published by the
// Free Software Foundation, either version 3 of the License,, or (at your
// option) any later version.
//
//! Logging and diagnostics for the engine, replacing the C++ debug stack.
//!
//! The original had three layers: the game-side `Lib/Debug.h` macros
//! (`DEBUG_LOG`, `DEBUG_CRASH`, …) used throughout the GameEngine, the
//! lower-level WWDebug library (`DASSERT`/`DLOG` with log groups and an
//! interactive assert box) and the `MiniDumper` crash handler. Per the
//! migration plan's mapping table this is a **re-implementation**, not a
//! port: no parity level applies, and the Rust idioms win where they
//! differ. [ADR-002](../../docs/adr/002-logging-diagnostics.md) records
//! the decision and the deviations.
//!
//! # Layout
//!
//! - [`init`]: subscriber setup — the `GameDebugInit` equivalent: a
//!   `tracing` fmt layer writing to stderr and/or the rotating log file,
//!   plus the panic hook.
//! - [`logfile`]: the log-file naming and rotation semantics of
//!   `DebugOpenLogFile` (`<prefix>DebugLogFile<suffix>.txt`, previous run
//!   renamed to the `Prev` name).
//! - [`level`]: the `DebugLevelMask` bitmask that gates
//!   [`debug_log_level!`].
//! - [`panic`]: the panic hook — the `MiniDumper` replacement; every
//!   panic is logged at `ERROR` with its location and a forced
//!   backtrace before the process dies.
//! - [`writer`]: the multi-sink writer the subscriber formats into.
//!
//! # The macro family
//!
//! The C++ macros map as follows (all logging macros emit `tracing`
//! events; how much of them is *recorded* is a subscriber decision —
//! see below):
//!
//! | C++ | Rust | Debug/Profile build | Release build |
//! |-----|------|---------------------|---------------|
//! | `DEBUG_LOG(m)` / `DEBUG_LOG_RAW(m)` | [`debug_log!`] | logged | not recorded |
//! | `DEBUG_LOG_LEVEL(l, m)` | [`debug_log_level!`] | mask-gated | not recorded |
//! | `DEBUG_ASSERTLOG(c, m)` | [`debug_assertlog!`] | logs when `!c` | not recorded |
//! | `DEBUG_CRASH(m)` | [`debug_crash!`] | logs + panics | **still panics** (deviation, below) |
//! | `DEBUG_ASSERTCRASH(c, m)` | [`debug_assertcrash!`] | as above when `!c` | as above |
//! | `RELEASE_CRASH(m)` | [`release_crash!`] | logs + panics | logs + panics |
//!
//! The `RAW`/non-`RAW` distinction (trailing newline) lives in the
//! formatter in Rust: one event, one line. Both macro names exist so
//! ported call sites keep their shape.
//!
//! ## Build variants
//!
//! The original's Debug/Profile/Release variants strip logging at
//! compile time (`DEBUG_LOGGING` etc. undefined). The Rust equivalent is
//! the subscriber plus `tracing`'s static filtering: a release binary
//! installs a subscriber with a `warn`/`error` maximum level (or none at
//! all), at which point the `debug`-level events above cost one cached
//! interest check each, and a binary that wants zero overhead can compile
//! with `tracing`'s `max_level_info` feature to remove them entirely.
//! The policy belongs to the final executable, not to this crate.
//!
//! ## Deviations from the original (deliberate, ADR-002)
//!
//! - [`debug_crash!`] **always panics**, including release builds, where
//!   the original compiled `DEBUG_CRASH` out and silently continued past
//!   its "unreachable" markers. A panic that names the invariant is
//!   strictly more debuggable than falling through.
//! - [`level::LEVEL_NET`] is the bit pattern `1 << 0`, not the original's
//!   `DEBUG_LEVEL_NET = 0`: with the enum value `0`, the original's
//!   `l & DebugLevelMask` gate could never fire (a dead-code quirk, kept
//!   only as a note here). The re-implementation uses real bits.
//! - The panic diagnostics go into the same log stream (`ERROR` events);
//!   the original `MiniDumper` wrote a separate `.dmp` file. Rust
//!   backtraces carry the equivalent information.
//!
//! # Determinism
//!
//! This crate is a side-channel, never part of the simulation's
//! observable state (plan §3.3): log output must not feed gameplay
//! decisions, and the level mask must not gate anything but logging.
//!
//! # Example
//!
//! ```
//! use rzh_log::level;
//!
//! // Emits a DEBUG event — recorded or not depends on the subscriber.
//! rzh_log::debug_log!("frame {} complete", 42);
//!
//! // Gated by the level mask, like DEBUG_LOG_LEVEL.
//! level::set_level_mask(level::LEVEL_NET);
//! rzh_log::debug_log_level!(level::LEVEL_NET, "packet: {} bytes", 128);
//!
//! // Both are no-ops for a naked panic-free doctest: without a subscriber
//! // installed, events are dropped.
//! assert_eq!(level::level_mask(), level::LEVEL_NET);
//! ```

#![forbid(unsafe_code)]

pub mod init;
pub mod level;
pub mod logfile;
pub mod panic;
pub mod writer;

// Re-exported so the macros below can refer to `tracing` through `$crate`
// without requiring the caller to depend on it.
pub use tracing;

pub use init::{Config, InitError, InitInfo, init};
pub use level::{LEVEL_MAX, LEVEL_NAMES, LEVEL_NET};
pub use logfile::LogFilePaths;
pub use writer::FanOutWriter;

/// `DEBUG_LOG(m)` / `DEBUG_LOG_RAW(m)`: log a message (with the trailing
/// newline a formatter concern rather than a call-site one, so the `RAW`
/// spelling is an alias).
///
/// Emits a `tracing` event at the `DEBUG` level.
#[macro_export]
macro_rules! debug_log {
    ($($arg:tt)+) => {
        $crate::tracing::debug!($($arg)+)
    };
}

/// `DEBUG_LOG_RAW(m)`: see [`debug_log!`] — identical in Rust.
#[macro_export]
#[doc(alias = "DEBUG_LOG_RAW")]
macro_rules! debug_log_raw {
    ($($arg:tt)+) => {
        $crate::tracing::debug!($($arg)+)
    };
}

/// `DEBUG_LOG_LEVEL(l, m)`: log a message only when `l` is enabled in
/// the global level mask ([`level::set_level_mask`]).
///
/// The mask check happens at runtime, per call, exactly like the
/// original's `if (l & DebugLevelMask)`.
#[macro_export]
macro_rules! debug_log_level {
    ($level:expr, $($arg:tt)+) => {
        if $crate::level::enabled($level) {
            $crate::tracing::debug!($($arg)+)
        }
    };
}

/// `DEBUG_ASSERTLOG(c, m)`: log a message when the condition is false.
///
/// The condition is always evaluated (like the original, and like
/// `DCHECK`), even when the event itself is not recorded.
#[macro_export]
macro_rules! debug_assertlog {
    ($cond:expr, $($arg:tt)+) => {
        if !($cond) {
            $crate::tracing::debug!($($arg)+)
        }
    };
}

/// `DEBUG_CRASH(m)`: log an `ERROR` and panic — the "this should never
/// happen" marker.
///
/// **Deviation from the original (ADR-002):** the C++ macro compiled out
/// in release builds and silently continued past the marker; the Rust
/// re-implementation always panics.
#[macro_export]
macro_rules! debug_crash {
    ($($arg:tt)+) => {{
        $crate::tracing::error!($($arg)+);
        panic!($($arg)+);
    }};
}

/// `DEBUG_ASSERTCRASH(c, m)`: [`debug_crash!`] when the condition is
/// false.
#[macro_export]
macro_rules! debug_assertcrash {
    ($cond:expr, $($arg:tt)+) => {
        if !($cond) {
            $crate::debug_crash!($($arg)+);
        }
    };
}

/// `RELEASE_CRASH(m)`: log an `ERROR` and panic in **every** build —
/// the one crash macro the original also kept in final release builds.
#[macro_export]
macro_rules! release_crash {
    ($($arg:tt)+) => {{
        $crate::tracing::error!($($arg)+);
        panic!($($arg)+);
    }};
}
