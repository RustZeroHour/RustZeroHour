# rzh-log

Logging and diagnostics for the
[RustZeroHour](https://github.com/RustZeroHour/RustZeroHour) engine: a
re-implementation of the original's debug stack on top of `tracing`, with
the log-file rotation, the level mask and a panic hook (the `MiniDumper`
replacement).

- **Status**: re-implementation per the migration plan's mapping table
  (`WWDebug` / `debug` / `profile` / `MiniDumper` → `tracing` + panic
  hook) — no parity level applies; see
  [ADR-002](../../docs/adr/002-logging-diagnostics.md) for the decision
  and the deliberate deviations.
- **Determinism**: `#![forbid(unsafe_code)]`; logging is a side-channel
  that must never feed gameplay decisions (plan §3.3).

## The macro family

| C++ original | here | behaviour |
|---|---|---|
| `DEBUG_LOG` / `DEBUG_LOG_RAW` | `debug_log!` / `debug_log_raw!` | `tracing` DEBUG event |
| `DEBUG_LOG_LEVEL(l, m)` | `debug_log_level!` | gated by the global level mask |
| `DEBUG_ASSERTLOG(c, m)` | `debug_assertlog!` | logs when `!c` (always evaluates `c`) |
| `DEBUG_CRASH(m)` | `debug_crash!` | ERROR event + **always panics** (deviation: the original compiled out in release and silently continued) |
| `DEBUG_ASSERTCRASH(c, m)` | `debug_assertcrash!` | as above when `!c` |
| `RELEASE_CRASH(m)` | `release_crash!` | ERROR event + panic in every build |

The Debug/Profile/Release build variants of the original map to the
subscriber's maximum level (a release binary records `warn`/`error` only,
or nothing) plus `tracing`'s static `max_level_*` features for zero
overhead — the policy belongs to the final executable.

## Log files

`logfile::open_log_file` keeps the original's naming and rotation:
`<dir>/<prefix>DebugLogFile<suffix>.txt` (a `D` on the stem in
`debug_assertions` builds), previous run renamed to the `Prev` name, the
oldest run discarded, warnings on rotation failures. The `_InstanceNN`
suffix the game appended for multi-instance runs is passed in by the
caller via [`Config::suffix`].

## Panics

`panic::install` (called by [`init`]) sets a hook that logs every panic
as an ERROR event — message, location and a **forced** backtrace — into
the active subscriber, then lets the unwind proceed. Panics inside the
hook itself fall back to a bare stderr write instead of recursing.

## Usage

```rust
fn main() -> Result<(), rzh_log::InitError> {
    let info = rzh_log::init(&rzh_log::Config::new("RustZeroHour"))?;

    rzh_log::debug_log!("engine starting");
    rzh_log::level::set_level_mask(rzh_log::level::LEVEL_NET);
    rzh_log::debug_log_level!(rzh_log::level::LEVEL_NET, "net up");
    Ok(())
}
```

## Dependencies

`tracing` and `tracing-subscriber` (fmt + std only) — the workspace's
first external dependencies, accepted by ADR-002: permissive licenses,
no unsafe below us, cross-platform, and a later Tracy integration can
attach to the same events without touching call sites.

Not ported (deferred, with their consumers): the WWDebug interactive
assert box (`DASSERT` dialogs — Rust has `debug_assert!` and panics),
the profiling timers of the `profile` library (arrive with the
simulation's performance work), network log streaming (`debug_io_net`)
and the Tracy subscriber itself.

See `docs/RUST_MIGRATION_PLAN.md` for the porting rules this crate
follows.
