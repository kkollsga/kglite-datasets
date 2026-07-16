# benchmarks — perf-parity gate (GATE #2)

`bench_sec_extract.py` runs our extracted SEC loader
(`kglite_datasets._sec_internal`) on a synthetic 2-CIK raw/ fixture and compares
its median against the **frozen baseline** in `baseline.json`. Reports median +
min (min = least-noise best case, per kglite's performance protocol). Offline.

```bash
make bench          # or: .venv/bin/python benchmarks/bench_sec_extract.py [--json]
```

Fails (non-zero exit) if our median regresses beyond `tolerance` x the baseline
median (1.5x — the same guard the old two-builder bench used). kglite's in-tree
loader has been removed, so there is no second builder to A/B against at
runtime; the gate anchors to `baseline.json` instead. This is deliberate: the
old script skipped its comparison whenever in-tree was absent, which silently
disabled the perf gate post-removal.

## Frozen baseline (`baseline.json`)

Captured **2026-07-16 from kglite's in-tree loader** while our extracted loader
was proven byte-identical to it — so the number is the historically-correct
perf anchor. Absolute ms are machine-specific; the load-bearing invariant is the
ratio.

| date | machine | fixture | baseline median | baseline min | tolerance |
|------|---------|---------|-----------------|--------------|-----------|
| 2026-07-16 | Apple M4 | synthetic 2-CIK (50 iters) | 3.369 ms | 3.252 ms | 1.5x |

Re-baseline only for a deliberate, measured perf change (a real improvement or
an accepted cost), by editing `baseline.json` in the same commit — never to
paper over a regression. On a different machine the absolute ms will differ;
judge by the ratio, and re-capture the baseline on that machine if you intend it
as the new anchor.
