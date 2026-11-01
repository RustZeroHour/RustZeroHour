# RustZeroHour

[![Rust CI](https://github.com/RustZeroHour/RustZeroHour/actions/workflows/rust-ci.yml/badge.svg)](https://github.com/RustZeroHour/RustZeroHour/actions/workflows/rust-ci.yml)

**An independent, open-source RTS inspired by *Command & Conquer: Generals –
Zero Hour*, being rebuilt in Rust from the released Zero Hour source code.**

This repository is a fork of
[TheSuperHackers/GeneralsGameCode](https://github.com/TheSuperHackers/GeneralsGameCode)
(the EA source release of *Generals* and *Zero Hour*, maintained by the
community). We are porting the engine to a Rust workspace and evolving it
into a game with its own identity, its own multiplayer, and no dependency on
EA-owned content.

The full strategy, phases and milestones live in the
[migration plan](docs/RUST_MIGRATION_PLAN.md). The headline decisions are
recorded in [ADR-000](docs/adr/000-migrate-to-rust.md):

- The Rust game is **not compatible with retail clients**: no retail
  multiplayer, replay or save compatibility.
- **Zero Hour is the only target**; the base-game `Generals/` tree is out of
  scope.
- The C++ tree stays in place, untouched, until the cut-over milestone (M5),
  so the differential-testing oracle keeps working.
- The port keeps the **GPL-3.0-or-later** license and never commits EA-owned
  assets.

## Current status

Early days — [Phase 0/1 of the plan](docs/RUST_MIGRATION_PLAN.md):
the Cargo workspace exists with its first foundation crate,
[`rzh-util`](crates/rzh-util/) (the game CRC and the deterministic
random-number generators), ported bit-exactly and validated against golden
vectors generated from a C++ reference harness
([`tools/differential/`](tools/differential/)).

| Stack | State |
|-------|-------|
| Rust (`crates/`) | Active development; CI: format + clippy (`-D warnings`) + tests + license check on Linux/Windows/macOS |
| C++ (`Core/`, `GeneralsMD/`, …) | Frozen baseline; kept for the oracle and reference; still builds with its original CI |

Progress by milestone: **M0** groundwork in progress → M1 data loads → M2
headless sim → M3 first pixels → M4 playable skirmish → M5 Rust-only → M6
online → M7 independent (see the plan's §4).

## Building the Rust workspace

Requires [rustup](https://rustup.rs); the toolchain is pinned in
[`rust-toolchain.toml`](rust-toolchain.toml) and installs automatically.

```sh
cargo fmt --all -- --check   # formatting
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace       # includes the differential golden vectors
```

See [docs/DEVELOPMENT.md](docs/DEVELOPMENT.md) for the full developer guide
(game data setup, regenerating golden vectors, porting rules).

## Running the game

Not yet. The Rust stack currently provides foundation crates only; a
playable headless simulation is milestone **M2** and the first visual client
is **M3**. The legacy C++ build still runs the original game (see below).

## The legacy C++ build

Until the cut-over (M5) the original C++ code remains in the repository and
remains buildable with the toolchain it shipped with (Visual Studio 2022,
CMake + vcpkg, or the provided Docker setup):

```sh
cmake --preset win32
cmake --build build/win32 --config Release
# or on Linux, via Docker:
./scripts/docker-build.sh
```

Detailed guides for the C++ build live in the
[upstream wiki](https://github.com/TheSuperHackers/GeneralsGameCode/wiki).
To run *Zero Hour* today, use the upstream releases of
[GeneralsGameCode](https://github.com/TheSuperHackers/GeneralsGameCode/releases)
— our fork does not produce game binaries yet.

## Documentation

- [Migration plan](docs/RUST_MIGRATION_PLAN.md) — the master plan (phases,
  parity levels, porting rules, risks).
- [Architecture decision records](docs/adr/README.md) — accepted decisions,
  starting with [ADR-000](docs/adr/000-migrate-to-rust.md).
- [Developer guide](docs/DEVELOPMENT.md) — environment, workflow, tests.
- [Code inventory](docs/inventory/) — measured census of the C++ baseline
  the port is working from.
- [Contributing](CONTRIBUTING.md) and the
  [AI contribution policy](AI_POLICY.md).

## Legal

- **License:** GPL-3.0-or-later (inherited from the EA source release; the
  Rust port is a derivative work). See [LICENSE.md](LICENSE.md).
- **Assets:** art, audio, maps, INI data and other EA-owned content are
  **never committed** to this repository. During development the game reads
  them from a user-owned install via `RZH_GAME_DIR`.
- **Trademarks:** "Command & Conquer" and "Zero Hour" are EA trademarks; this
  project is not affiliated with or endorsed by EA. The project will be
  renamed before its first public release (plan §7).

## Acknowledgements

- The original *Generals* and *Zero Hour* developers at EA Los Angeles.
- [TheSuperHackers](https://github.com/TheSuperHackers), whose years of
  community engineering on the released source made this fork possible.
