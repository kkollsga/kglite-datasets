"""GATE #1 — frozen golden for the SEC extractor (single-builder oracle).

The load-bearing gate of the whole extraction (codingest handover lesson). The
digest was frozen 2026-07-16 from kglite's in-tree loader (`kglite._sec_internal`)
while a two-builder equivalence check proved our extracted loader produced
byte-identical output on the same input. **On 2026-07-16 kglite deleted its
in-tree loaders** (`kglite._sec_internal` / `kglite.datasets` are gone), so that
cross-builder comparison is impossible forever — the two-builder test has been
retired. The frozen golden carries the authority forward: our loader
(`kglite_datasets._sec_internal`) is checked against the anchored digest.

The builder runs offline on a synthetic `raw/` fixture (no network). The
volatile `source_extracted_at` provenance column (wall clock) is excluded from
the canonical rendering — it can never match across two runs; every other column
is deterministic and goldened.
"""

from __future__ import annotations

import hashlib
import os
from pathlib import Path

import pytest

from kglite_datasets import _sec_internal as ours
from kglite_datasets.tests.synth_sec import write_synth_raw

GOLDEN = Path(__file__).resolve().parents[2] / "tests" / "goldens" / "sec-extract-csv.sha256"

# Provenance columns stamped with the wall clock — excluded from the digest so
# the rendering is deterministic across runs/builders.
VOLATILE_COLUMNS = {"source_extracted_at"}


def _canonical_processed(workdir: Path) -> str:
    """Deterministic rendering of the processed/ CSV set: files sorted, the
    volatile provenance column dropped, data rows sorted, tab-joined."""
    processed = workdir / "processed"
    out: list[str] = []
    for csv_path in sorted(processed.glob("*.csv")):
        lines = csv_path.read_text().splitlines()
        out.append(f"## {csv_path.name}")
        if not lines:
            continue
        header = lines[0].split(",")
        keep = [i for i, col in enumerate(header) if col not in VOLATILE_COLUMNS]
        out.append("H\t" + "\t".join(header[i] for i in keep))
        rows = ["\t".join(row.split(",")[i] if i < len(row.split(",")) else "" for i in keep)
                for row in lines[1:]]
        out.extend(f"R\t{r}" for r in sorted(rows))
    return "\n".join(out) + "\n"


def _digest(s: str) -> str:
    return hashlib.sha256(s.encode()).hexdigest()


def _build(mod, tmp_path: Path) -> str:
    wd = write_synth_raw(tmp_path)
    mod.extract_all_py(str(wd), force=True)
    return _canonical_processed(wd)


def test_sec_extract_golden(tmp_path: Path) -> None:
    """Assert our loader's output against the frozen digest.

    The digest was anchored 2026-07-16 to kglite's in-tree authority (verified
    byte-identical to ours at freeze time). kglite's in-tree loader has since
    been removed, so the authority transferred to this workspace's builder:
    ``UPDATE_GOLDEN=1`` now (re)captures from ``kglite_datasets._sec_internal``.
    Regenerate deliberately only for an intended builder-behavior change, in the
    same commit as that change."""
    render = _build(ours, tmp_path / "authority")
    got = _digest(render)
    if os.environ.get("UPDATE_GOLDEN") or not GOLDEN.exists():
        GOLDEN.write_text(got + "\n")
        pytest.skip(f"froze golden {GOLDEN.name} = {got}")
    want = GOLDEN.read_text().strip()
    assert got == want, (
        f"SEC extract golden drifted ({GOLDEN.name}). A digest change means the "
        f"emitted CSVs changed. If intentional, rerun with --update-golden and "
        f"commit the new .sha256 in the same change."
    )
