# Code inventory

Reproducible, versioned census of the C++ baseline that the Rust migration
works from (migration plan §4, Phase 0, task 5). Each report records the
commit it was generated from.

Regenerate with:

```sh
python3 tools/inventory/inventory.py
```

| Report | Contents |
|--------|----------|
| [loc.md](loc.md) | Line census by area; porting strategy per area |
| [singletons.md](singletons.md) | The `TheXxx` global singleton declarations the Rust design replaces with explicit contexts |
| [retail-compat-switches.md](retail-compat-switches.md) | The `RETAIL_COMPATIBLE_*` switches and the branch each port follows (always non-retail, ADR-000) |
| [platform-coupling.md](platform-coupling.md) | Platform/technology coupling motivating the re-implement strategy |
| [rts-forks.md](rts-forks.md) | `RTS_GENERALS` vs `RTS_ZEROHOUR` forks; only the Zero Hour branch is ported |

The plan's §2 measurements were taken at the baseline commit `571a7c5`
(tag `legacy-cpp-baseline`); these reports measure the current tree, so
counts drift slightly as upstream logic fixes are cherry-picked (allowed
until M2, plan §4 Phase 0).
