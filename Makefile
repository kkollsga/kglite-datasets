# kglite-datasets — CI-equivalent local gate
#
# No CI yet (no git remote). `make gate` mirrors kglite's gate discipline —
# a `lint` aggregate (cargo fmt --check + clippy -D warnings, plus ruff when
# it's installed), workspace build, workspace test — plus two describe-only
# steps that point at the real Python-side gates:
#   * [5/6] determinism — the frozen-digest oracles (Rust csv_golden under
#     step 4; the SEC golden in `make pytest`), and
#   * [6/6] bench-smoke — the retargeted SEC-extract perf gate (`make bench`).
# Steps 5 & 6 are describe-only in this portable Rust gate because their real
# checks need the venv + a kglite wheel; run `make pytest` / `make bench` for
# them. If this workspace gets a GitHub remote later, lint+build+test map 1:1
# onto kglite's `Rust checks` job and pytest+bench onto its Python job.
#
# Run `make gate` before every commit. Individual steps are runnable too
# (`make lint`, `make test`, …). Python helpers: `make develop`, `make pytest`.

SHELL := /bin/bash

.PHONY: gate lint fmt fmt-check clippy ruff-check build test determinism bench-smoke bench develop pytest clean

## Full CI-equivalent gate — the single entry point. Runs every step in
## order and stops at the first failure.
gate: lint build test determinism bench-smoke
	@echo ""
	@echo "=================================================="
	@echo " gate: ALL STEPS PASSED"
	@echo "=================================================="

## 1. Lint aggregate (matches kglite's `make lint`): formatting must be clean
##    and clippy warning-free. ruff runs too when it's installed in the venv.
lint: fmt-check clippy ruff-check
	@echo "== lint: OK =="

## Formatting must be clean (matches kglite `cargo fmt -- --check`).
fmt-check:
	@echo "== [1/6] cargo fmt --check =="
	cargo fmt --check

## Auto-format (convenience; not part of the gate).
fmt:
	cargo fmt

## Clippy, warnings-as-errors (matches kglite, widened to --workspace).
clippy:
	@echo "== [2/6] cargo clippy --workspace --all-targets -- -D warnings =="
	cargo clippy --workspace --all-targets -- -D warnings

## Ruff over the Python sources — runs only if ruff is installed (no ruff
## config is committed yet, so this uses ruff's defaults). Skips cleanly
## otherwise so the portable gate never depends on a Python toolchain.
ruff-check:
	@if command -v ruff >/dev/null 2>&1; then \
		echo "== ruff check + format --check =="; \
		ruff check kglite_datasets benchmarks && ruff format --check kglite_datasets benchmarks; \
	else \
		echo "== ruff: not installed — skipping (install into .venv to enable) =="; \
	fi

## 3. Build every crate + binary in the workspace.
build:
	@echo "== [3/6] cargo build --workspace =="
	cargo build --workspace

## 4. Test the workspace — includes tests/csv_golden.rs (the Rust output-boundary
##    oracle: the sodir preprocess FK-joins digest to a frozen golden).
test:
	@echo "== [4/6] cargo test --workspace =="
	cargo test --workspace

## 5. Determinism / self-consistency. This is NOT a separate run — it names
##    the frozen-digest oracles already exercised elsewhere: the Rust
##    csv_golden (sodir) runs under step 4; the SEC golden
##    (test_parity_golden, single-builder vs frozen digest) runs under
##    `make pytest`. Both need no second builder — kglite's in-tree loader is
##    gone, so the frozen goldens are the sole authority.
determinism:
	@echo "== [5/6] determinism / self-consistency =="
	@echo "  Rust sodir golden: covered by csv_golden in step 4."
	@echo "  Python SEC golden: make pytest (test_parity_golden, golden-only)."

## 6. Bench / perf-parity. The SEC extract is exercised through Python (the
##    Rust surface ends at CSV emit), so the perf gate is the Python bench,
##    which now asserts our builder against a frozen baseline JSON (kglite's
##    in-tree copy is gone). It needs the venv + a kglite wheel, so it is NOT
##    in the portable Rust gate — run it with `make bench`.
bench-smoke:
	@echo "== [6/6] bench / perf-parity =="
	@echo "  Python SEC-extract bench: make bench (needs .venv + kglite)."
	@echo "  Baseline: benchmarks/baseline.json + benchmarks/README.md."

## SEC-extract perf gate. Needs `make develop` + a kglite wheel in .venv.
## Fails if our builder regresses beyond 1.5× the frozen baseline median.
bench:
	.venv/bin/python benchmarks/bench_sec_extract.py

## Build the Python extension into the local venv.
develop:
	.venv/bin/maturin develop

## Run the offline Python test suite (live-API suites self-skip without their
## integration env vars). Needs `make develop` + a kglite wheel in .venv.
pytest:
	.venv/bin/python -m pytest

## Remove build artifacts.
clean:
	cargo clean
