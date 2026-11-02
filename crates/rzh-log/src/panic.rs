// SPDX-License-Identifier: GPL-3.0-or-later
// This file is part of RustZeroHour, a derivative of the Zero Hour source
// release. Copyright 2025 Electronic Arts Inc.; Rust port Copyright 2026 the
// RustZeroHour contributors.
//
// Re-implements the crash-diagnostics role of MiniDumper (and the stack
// dump handler wired by GameDebugInit) at baseline 571a7c5.
//
//! The panic hook: the `MiniDumper` replacement.
//!
//! [`install`] sets a `std::panic` hook that logs every panic as an
//! `ERROR` event — message, location and a **forced** backtrace (the
//! original always dumped, so the port does not honour
//! `RUST_BACKTRACE=0` either) — and then lets the normal unwinding
//! proceed. The diagnostics land in whatever subscriber is installed
//! (the rotating log file when [`crate::init`] was used), which is the
//! Rust analogue of the original writing the `.dmp` next to the
//! executable.
//!
//! A panic in the *panic hook itself* must not recurse: the hook guards
//! itself with a thread-local flag and falls back to a plain stderr
//! write, mirroring how the original's crash handler avoided re-entering
//! itself.

use std::cell::Cell;
use std::panic::PanicHookInfo;
use std::sync::OnceLock;

thread_local! {
    /// Guards against a panic inside the hook itself.
    static IN_HOOK: Cell<bool> = const { Cell::new(false) };
}

/// The installed hook, so tests can restore the previous one.
static INSTALLED: OnceLock<()> = OnceLock::new();

/// The message a non-string panic payload produces.
const NON_STRING_PAYLOAD: &str = "<non-string panic payload>";

/// Formats the panic message from its payload (`&str`, `String` or
/// anything else).
#[must_use]
pub fn payload_message(payload: &(dyn std::any::Any + Send)) -> String {
    if let Some(text) = payload.downcast_ref::<&str>() {
        (*text).to_owned()
    } else if let Some(text) = payload.downcast_ref::<String>() {
        text.clone()
    } else {
        NON_STRING_PAYLOAD.to_owned()
    }
}

/// The event the hook emits: one `ERROR` line with the message and
/// location, followed by the backtrace.
fn log_panic(info: &PanicHookInfo<'_>) {
    let message = payload_message(info.payload());
    let location = info
        .location()
        .map_or_else(|| "<unknown>".to_owned(), |location| location.to_string());
    let backtrace = std::backtrace::Backtrace::force_capture();
    tracing::error!(
        target: "rzh_log::panic",
        "panic: {message}\n  at {location}\n{backtrace}"
    );
}

/// The hook body: guarded against re-entry.
fn hook(info: &PanicHookInfo<'_>) {
    let already_in_hook = IN_HOOK.with(Cell::get);
    if already_in_hook {
        // A panic inside the hook: write what we can, directly.
        let message = payload_message(info.payload());
        eprintln!("panic inside the rzh-log panic hook: {message}");
        return;
    }
    IN_HOOK.with(|flag| flag.set(true));
    log_panic(info);
    IN_HOOK.with(|flag| flag.set(false));
}

/// Installs the panic hook (the `MiniDumper` replacement).
///
/// Idempotent: a second call is a no-op, which keeps [`crate::init`]
/// and explicit early installation from fighting over the hook.
pub fn install() {
    let first = INSTALLED.set(());
    if first.is_ok() {
        std::panic::set_hook(Box::new(hook));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn payload_messages_cover_the_three_shapes() {
        assert_eq!(payload_message(&"boom"), "boom");
        assert_eq!(payload_message(&String::from("boom")), "boom");
        assert_eq!(payload_message(&7_u32), NON_STRING_PAYLOAD);
    }
}
