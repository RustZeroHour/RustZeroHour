# ADR-002: Logging and diagnostics — `tracing`, log-file rotation, panic hook

- **Status:** Accepted
- **Date:** 2026-11-02
- **Supersedes:** none
- **Superseded by:** none

## Context

The original code base carries three diagnostics layers: the game-side
macros of `Core/Libraries/Source/Lib/Include/Lib/Debug.h` (`DEBUG_LOG`,
`DEBUG_LOG_LEVEL`, `DEBUG_ASSERTLOG`, `DEBUG_CRASH`, `RELEASE_CRASH`) used
throughout the GameEngine; the lower-level WWDebug stack
(`Core/Libraries/Source/debug`, `profile`) with log groups, an interactive
assert dialog and `OutputDebugString` output; and the `MiniDumper` crash
handler that wrote `.dmp` files with stack traces. All are Windows-centric
and none survives the cross-platform, no-proprietary-code constraints of
the port. The migration plan's mapping table (§2) already routes them to
"`tracing`, panic hook, optional Tracy" as a **re-implementation** — this
ADR records what that means concretely and the deviations it commits us
to. It also introduces the workspace's **first external dependencies**,
which per the ADR rules (docs/adr/README.md) needs a record of its own.

The simulation itself is unaffected: logging is a side-channel (plan
§3.3), never part of the observable game state.

## Options

| Option | Summary | Trade-offs |
|--------|---------|------------|
| A | Port the WWDebug stack as-is (dialogs, log groups, ODS) | Windows-shaped API; the assert dialog is meaningless in a headless sim; large port for zero parity value |
| B | `log` facade + a hand-rolled file writer | Smallest tree, but re-invents levels/threads/panic handling and forecloses Tracy |
| C | `tracing` + `tracing-subscriber` (fmt) with the original's file rotation, level mask and a panic hook re-implemented on top | Two permissively-licensed deps; keeps call-site names shaped like the original's macros; Tracy can attach later without code changes |

## Decision

Option C, as the crate `rzh-log`:

- the `DEBUG_*` macro family is re-implemented as `debug_log!`,
  `debug_log_level!`, `debug_assertlog!`, `debug_crash!`,
  `debug_assertcrash!` and `release_crash!`, emitting `tracing` events;
- `DebugOpenLogFile`'s naming and rotation semantics are kept
  (`<prefix>DebugLogFile<suffix>.txt`, `Prev` rename, `D` stem in
  debug builds);
- `DebugLevelMask` is re-implemented as a global atomic bitmask gating
  `debug_log_level!`;
- the `MiniDumper` role moves into a `std::panic` hook that logs message,
  location and a forced backtrace as an ERROR event;
- the build-variant policy (Debug/Profile log, Release does not) moves to
  the subscriber's maximum level and, for zero-overhead release builds,
  `tracing`'s static `max_level_*` features — decided by the final
  executable, not by this crate.

## Consequences

- **Deviation 1 — `debug_crash!` always panics.** The original compiled
  `DEBUG_CRASH` out of release builds and silently continued past its
  "unreachable" markers. A panic that names the broken invariant is
  strictly more debuggable; any ported call site that relied on falling
  through needs re-review when it is ported.
- **Deviation 2 — the level constants are real bits.** The original's
  `DEBUG_LEVEL_NET = 0` could never pass its own `l & DebugLevelMask`
  gate (dead code since 2003); `LEVEL_NET = 1 << 0` makes the mask work.
- The workspace now depends on `tracing`/`tracing-subscriber` (MIT,
  no `unsafe` below us); `cargo-deny` covers them like every future dep.
- A later Tracy integration is a subscriber change in the executable plus
  an optional feature on this crate — no call-site churn.
- Still deferred with their consumers: the WWDebug interactive assert
  box (Rust has `debug_assert!` and panics), the `profile` timers
  (arrive with simulation performance work), network log streaming, and
  the Tracy subscriber itself.
