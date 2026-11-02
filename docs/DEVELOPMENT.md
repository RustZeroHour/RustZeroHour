# Developer guide

This guide covers the Rust side of the repository. The legacy C++ build keeps
its own instructions in the
[upstream wiki](https://github.com/TheSuperHackers/GeneralsGameCode/wiki).

## Prerequisites

- [rustup](https://rustup.rs). The toolchain is pinned in
  [`rust-toolchain.toml`](../rust-toolchain.toml) (currently
  `1.99.1`, plus `clippy` and `rustfmt`); `cargo` installs it automatically on
  first use.
- A C++ compiler (g++ or clang++) if you want to regenerate the differential
  golden vectors.
- Python 3 if you want to regenerate the [code inventory](inventory/).

## Workspace layout

```
crates/            Rust workspace (grows as the migration proceeds, plan §3.4)
  rzh-util/        CRC + RNG + name-key interning (P0 ported, golden-vector tested)
  rzh-math/        WWMath math: scalars, vectors, gmath, matrices, quaternions
                   (P0 ported, golden-vector tested)
  rzh-compress/    EA RefPack codec (P0 ported, byte-exact both directions)
  rzh-vfs/         virtual file system: BIG archives, loose-file overlay,
                   EA compression dispatch (P0 ported)
tools/
  differential/    C++ reference harness that generates the golden vectors
  inventory/       reproducible census of the C++ baseline
docs/
  RUST_MIGRATION_PLAN.md   the master plan
  adr/             architecture decision records
  inventory/       generated inventory reports
  DEVELOPMENT.md   this file
Core/ GeneralsMD/ Generals/ Dependencies/ Tests/ Benchmarks/
                   the frozen C++ baseline (stays until M5)
```

## Everyday commands

```sh
cargo fmt --all                                   # format
cargo fmt --all -- --check                        # CI check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace                            # unit + golden + doc tests
```

`cargo test --workspace` runs the differential golden-vector tests of all
four crates against the committed fixtures in `crates/*/tests/golden/`.
These prove byte-exact parity (level **P0**) with the C++ algorithms at
the baseline commit `571a7c5` (tag `legacy-cpp-baseline`):

- `rzh-util`: CRC, RNG streams and the name-key interning subsystem
  (`crc.txt`, `rng.txt`, `namekey.txt`),
- `rzh-math`: the WWMath scalar helpers, the Vector2/3/4 arithmetic, the
  pinned GameMath (msun) transcendental entry points, `Matrix3x3`, the
  quaternions and the `Fast_*` tables (`scalar.txt`, `vectors.txt`,
  `matrices.txt`, `gmath.txt`; see ADR-001 for the numerics scope),
- `rzh-compress`: the RefPack codec, byte-exact in both directions
  (`refpack.txt`),
- `rzh-vfs`: a golden *script* (`vfs.txt`) replayed against the port —
  BIG parsing, listings, instance routing, overwrite stacking, the
  INIZH skip, path keys and the `EAR\0` decompress-on-read flow, with
  compressed payloads produced by the real EAC encoder.

To regenerate the golden vectors after (re)proving a transcription:

```sh
tools/differential/regenerate.sh
cargo test --workspace      # must still pass
git diff crates/*/tests/golden    # review the diff, then commit
```

## Game data and `RZH_GAME_DIR`

The engine must **never** commit EA-owned assets (art, audio, maps, INI data,
…). Later phases read them at runtime from an original *Zero Hour*
installation owned by the developer:

```sh
export RZH_GAME_DIR=/path/to/your/ZeroHourInstall
```

Tests that need real game data are tagged `needs-game-data` and are skipped
in public CI (migration plan §6). `rzh-vfs` reads the game directory only
when a caller mounts it (`Vfs::new(root)` + `load_archives_from_directory`);
no test touches `RZH_GAME_DIR` yet — the first consumer is the Phase 2 INI
loader.

## Porting rules (summary)

The full rules are §3.3 of the [migration plan](RUST_MIGRATION_PLAN.md); the
ones you touch every day:

1. Port the Zero Hour branch and the **non-retail** branch of every
   `RETAIL_COMPATIBLE_*` switch; drop dead `#if` code.
2. Record provenance in every ported file:
   `Ported from <path> @ 571a7c5`, and preserve the original copyright
   notices.
3. Sim-bound crates use `#![forbid(unsafe_code)]`; `unsafe` lives only in
   platform/FFI crates with `// SAFETY:` comments.
4. No global mutable state — explicit contexts own their state (e.g. the C++
   RNG globals became `GameRng` values).
5. Determinism: single-threaded sim, no wall-clock or OS randomness, no
   `HashMap` iteration in sim code, `f32` with the original operation order,
   fixed-width integers in anything hashed or serialized.
6. Every ported unit ships Rust unit tests **and** a differential/golden test
   that states its parity level (P0–P3).

The definition of done for a ported unit is the checklist in plan §6.

## Status of Phase 0 (groundwork)

| Item | Status |
|------|--------|
| Migration plan committed under `docs/` | Done |
| Baseline tagged `legacy-cpp-baseline` | Done |
| ADR process + ADR-000 | Done |
| Cargo workspace, pinned toolchain, fmt/clippy/deny config | Done |
| Rust CI (Linux/Windows/macOS) | Done — `rust-ci.yml` |
| Governance docs rewritten for the fork | Done |
| Inventory tooling + published reports | Done — `tools/inventory/` |
| First foundation crate (`rzh-util`: CRC, RNG) | Done, P0-verified |
| Name-key interning in `rzh-util` | Done, P0-verified (`namekey` module) |
| C++ oracle build with trace instrumentation (plan §3.2) | **Deferred** — needs the Windows/vcpkg toolchain; next step, tracked with the Phase 0 exit criteria |

## Status of Phase 1 (foundation crates)

| Item | Status |
|------|--------|
| `rzh-math` first slice: WWMath scalars + Vector2/2i/3/3i/4 | Done, P0-verified; quirks preserved and documented |
| `rzh-math` trigonometric entry points (`gmath` port of the pinned GameMath) | Done, P0-verified; see ADR-001 |
| `rzh-math` matrices/quaternions: `Matrix3x3`, `Quaternion`, `Fast_*` tables | Done, P0-verified (mixed-precision literal quirks preserved) |
| `rzh-math` `Matrix3D` (4x3 transform) | Done, P0-verified (2885 golden records; the `Solve_Linear_System` W-column quirk pinned) |
| `rzh-math` `Matrix4x4` | Done, P0-verified (813 golden records; the fov `Init_Perspective` documented as platform-libm, not pinned) |
| `rzh-compress`: RefPack codec | Done, P0-verified, byte-exact in both directions |
| `rzh-vfs`: BIG archives, loose-file overlay, `CompressionManager` dispatch | Done, P0-verified (golden script + unit tests) |
| `AsciiString` port | Planned together with the INI work |
| Logging/diagnostics via `tracing` | Planned |

## Getting help

- Open a GitHub issue for bugs and questions.
- Architecture questions go through the
  [ADR process](adr/README.md#architecture-decision-records).
