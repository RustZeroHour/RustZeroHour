# ADR-003: Accepted §6 benchmark budget exceptions (Phase 1)

- **Status:** Accepted
- **Date:** 2026-11-03
- **Supersedes:** none
- **Superseded by:** none

## Context

The migration plan's §6 definition of done requires a ported unit's
benchmark to be "no more than 20 % slower than the C++ baseline unless an
ADR justifies it". The Phase 1 budget run
([docs/benchmarks.md](../benchmarks.md), 32 comparisons, identical corpora
and loop shapes on both sides) leaves **three** benchmarks above the
threshold:

| Benchmark | Ratio | Absolute gap | Path |
|---|---:|---:|---|
| `namekey/intern_hit/x512` | 1.36x | 3.7 ns per lookup | data load (boot) |
| `refpack/encode/random` | 1.46x | 1.9 ns per byte | tools / write path |
| `vector3/quick_distance2d/x256` | 1.36x | 0.3 ns per call | partition manager |

All three are non-frame-time paths, and none of them regressed the port
relative to the C++ by more than a few nanoseconds per operation. The
frame-time-critical kernels (RNG draws, matrix multiplies, quaternion
slerps, fast-trig lookups, RefPack **decode**, CRC, INI parsing) are all
within budget — most are faster than the C++ baseline.

## Options

| Option | Summary | Trade-offs |
|--------|---------|------------|
| A | Micro-optimize all three now | Delays Phase 2; the name-key and quick-distance gaps are codegen-level (20–30 cycles) and may not be recoverable without `unsafe` or contorted code in a P0-pinned crate |
| B | Accept and document as ADR-justified exceptions with follow-ups | Honest bookkeeping, keeps Phase 1 exit unblocked; the follow-ups are recorded and can be picked up when the affected paths matter (mod tools, map editor) |
| C | Drop the benchmarks from the budget table | Hides real data; violates the plan's own definition of done |

## Decision

Option **B**: accept the three exceptions, record them here and in
[docs/benchmarks.md](../benchmarks.md), and track the follow-ups.

1. **`namekey/intern_hit` (1.36x).** The lookup cost is dominated by the
   `calcHashForString` byte loop plus the socket probe; the C++ version
   is compared against `strcmp`/inline scan code that GCC optimizes
   slightly better for 20–25-byte names. Name→key lookups by *string*
   happen during data load and template wiring, not per frame (the frame
   path uses the already-interned `NameKey` values). *Follow-up:* revisit
   if profiling of the real Phase 2 data load shows the store parse
   bound here (the INI parse that contains it is within budget at 1.19x).
2. **`refpack/encode/random` (1.46x).** The encoder inserts every byte
   position into its hash chain (`hash3` + two stores per byte); in Rust
   those array accesses carry bounds checks the C pointer arithmetic does
   not. Encoding is a **write** path — the engine only decodes at runtime
   (all decode benchmarks are within budget, 0.23x–1.03x); encoding
   happens in the future WorldBuilder/mod tooling (Phase 8). *Follow-up:*
   restructure the insert loop over sub-slices (like `matchlen`'s fix)
   when the tools crate lands.
3. **`vector3/quick_distance2d` (1.36x).** 0.3 ns per call on a
   deliberately cheap "quick" estimate (three abs + a compare-swap in the
   original). The absolute difference is ~1 cycle. *Follow-up:* none
   planned; revisit only if the partition-manager port (Phase 3) shows it
   in a profile.

## Consequences

- Phase 1's §6 exit criterion is met: benchmarks exist for every
  foundation crate, and every over-budget result is justified here as the
  plan allows.
- The exceptions are explicitly *not* precedent: each new phase's budget
  run must again justify its own outliers (the ADR mechanism is per
  finding, not a blanket allowance).
- The two real follow-ups (encoder insert loop, name-key lookup) are
  recorded in this ADR and should be linked to their phases when those
  phases start.
