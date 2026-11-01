# Security Policy

## Supported stacks

| Stack | Supported |
|-------|-----------|
| Rust workspace (`crates/`) | Yes — this is where fixes land |
| Legacy C++ baseline (`Core/`, `GeneralsMD/`, …) | Best effort; treated as a frozen reference until the cut-over milestone (M5) |

## Reporting a Vulnerability

Please report vulnerabilities privately using
[GitHub security advisories](https://github.com/RustZeroHour/RustZeroHour/security/advisories/new)
("Report a vulnerability" on the Security tab of this repository).

- Do **not** open a public issue for anything exploitable.
- Reports about the Rust code, the differential tooling, or the repository
  workflows are all in scope.
- If the issue also affects the upstream C++ project
  [TheSuperHackers/GeneralsGameCode](https://github.com/TheSuperHackers/GeneralsGameCode),
  report it there as well; the C++ tree in this fork is a frozen copy of it.

## Scope notes

- This repository must never distribute EA-owned game assets; a report of
  committed assets is treated as a security issue — remove first, discuss
  after.
- The eventual multiplayer stack (plan Phase 7) is not implemented yet; when
  it is, its threat model and desync-resistance guarantees belong here.
