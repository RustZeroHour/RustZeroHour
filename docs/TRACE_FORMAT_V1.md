# Oracle trace format v1

Status: **specification** (implementation pending — plan §3.2, Phase 0/1
task 7). This document defines what the instrumented C++ oracle emits so
that the Rust simulator's traces can be diffed against it (parity levels
P1/P2) and, after the cut-over (M5), Rust-vs-Rust golden traces can take
over the same format unchanged.

The oracle itself is a Release, Zero Hour build of the C++ tree with the
`RETAIL_COMPATIBLE_*` switches **off** (the same branches the Rust port
follows, ADR-000) plus the instrumentation described here, gated behind the
CMake option `RZH_ORACLE_TRACE`. Its acceptance test is *self-consistency*:
two runs of the same input produce identical traces.

## Design rules

1. **Append-only, line-oriented, ASCII.** One event per line, `\n`
   terminated, no CR. Line-oriented keeps traces diff-able with standard
   tools and crash-resilient (the last line may be partial).
2. **Deterministic content only.** No timestamps, no pointers, no
   addresses, no wall-clock, no thread ids. Order comes from the
   single-threaded simulation loop itself.
3. **Fixed-width integers, decimal or `0x`-prefixed hex** — no
   platform-dependent formatting. Floats are emitted as IEEE-754 hex
   bit patterns (`0x%08x` of the `f32` bits) so P2 comparisons never
   depend on printf float rounding.
4. **Every event names its source file conceptually**: the format is
   versioned (`v1`), and unknown event types must make the comparator
   fail loudly rather than skip.

## File layout

A trace run produces one file:

```
rzh-trace-v1 <build-id> <scenario-id> <crc-mode>
```

- `build-id`: the git commit of the C++ oracle build.
- `scenario-id`: stable identifier of the input (replay file or recorded
  scenario name, see §Inputs).
- `crc-mode`: `ignore-replay-crc` or `verify-replay-crc` — whether the
  `.rep` checksum in the input was honored (the oracle runs with it
  ignored by default, plan §3.2).

Then events follow, one per line.

## Events (v1)

| Event | Line | Fields |
|---|---|---|
| **Frame tick** | `frame <n>` | logic frame number (30 Hz) |
| **State hash** | `hash <n> <state-crc>` | per-frame CRC of the canonical state serialization (below) |
| **Snapshot** | `snap <n> <offset>` | a full state snapshot was written to the run's snapshot sidecar at byte `offset` (frame `n`) |
| **Object created** | `+obj <n> <id> <template> <owner>` | template name key and player slot |
| **Object destroyed** | `-obj <n> <id>` | |
| **Command** | `cmd <n> <player> <type> <args…>` | one line per executed GameMessage, in execution order |
| **Money** | `money <n> <player> <amount>` | after every credit/debit transaction |
| **Error** | `!err <n> <code>` | engine-detected anomaly (desync, script error, …) — a trace containing `!err` fails comparison |

`<n>` is the frame number. Objects are identified by their engine object
ID (stable, recycled never — IDs are monotonic in v1).

**State hash definition (v1).** The hash is the game's `CRC` (the
shift-add checksum, `rzh-util`'s `Crc`) computed over the canonical
`XferSave`-order serialization of:

- the frame number,
- the player list (money, energy, science/rank state, relationships),
- the object table in ID order: template key, owner, position (`f32`
  bits, x/y/z), orientation (facing angle), health, status bit-flags,
- the partition/visibility state is **excluded** in v1 (client-side),
- the script-engine state (variables, timers) is **excluded** in v1
  (until the script engine is ported; revisit when Phase 3 package 7
  lands).

The precise field order is the oracle's `XferSave` implementation; the
Rust side re-implements the same order. Any change to the order is a new
trace version (`v2`), never a silent edit of `v1`.

## Inputs

- The 10 committed Zero Hour replays (`GeneralsReplays`), run through
  `-headless -replay` with the trace enabled.
- Purpose-built scenarios (one mechanic each) recorded with the oracle
  *before* the C++ tree is deleted (plan §3.2); these become the P1/P2
  scenario suite for the Rust sim in Phase 3.
- A small tool converts legacy `.rep` command streams into the new
  command-stream format; used for oracle comparison only.

## Comparison

- **P0**: byte-exact sections (parsing, serialization) never appear in
  traces; they are golden-tested directly (see `tools/differential/`).
- **P1**: for scripted short-horizon scenarios, both traces must match
  event-for-event (object set, owners, money, commands).
- **P2**: positions/health may differ within ε; the comparator takes a
  tolerance for `+obj` positions and `hash` lines are compared only every
  K frames with a divergence budget (chaotic divergence is expected;
  plan §3.2).
- The comparator lives in `rzh-replay` when that crate exists; until
  then a small script in `tools/differential/` is acceptable.

## Lifetime

The oracle and this trace format exist until M5. At cut-over, the Rust
sim emits the *same* format for Rust-vs-Rust golden traces; the legacy
`.rep` import path is dropped. The oracle recipe (build flags,
instrumentation patch) is archived on the `oracle-recipe` branch when the
C++ tree is deleted.
