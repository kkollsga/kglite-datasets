# kglite-datasets — CI-equivalent local gate
#
# `make gate` mirrors kglite's gate discipline — a `lint` aggregate
# (cargo fmt --check + clippy -D warnings + ruff check/format --check),
# workspace build, workspace test — plus two describe-only steps that point at
# the real Python-side gates:
#   * [5/6] determinism — the frozen-digest oracles (Rust csv_golden under
#     step 4; the SEC goldens in `make pytest`), and
#   * [6/6] bench-smoke — the retargeted SEC-extract perf gate (`make bench`).
# Steps 5 & 6 are describe-only here because their real checks need a kglite
# wheel in the venv; run `make pytest` / `make bench` for them.
#
# `.github/workflows/ci.yml` runs the same three surfaces: `rust-checks`
# (fmt/clippy/test), `python-lint` (ruff), and `python` (pytest, on both ends
# of the declared kglite range).
#
# Run `make gate` before every commit. Individual steps are runnable too
# (`make lint`, `make test`, …). Python helpers: `make venv`, `make develop`,
# `make pytest`.

SHELL := /bin/bash

# Python tooling lives in the repo venv. `make venv` provisions it; every
# Python-side target resolves through these paths so nothing depends on the
# venv being *activated*.
VENV      := .venv
PY        := $(VENV)/bin/python
RUFF      := $(VENV)/bin/ruff
MATURIN   := $(VENV)/bin/maturin
# Dev toolchain. `kglite` is the runtime engine dependency (see pyproject
# `dependencies`); the rest are what the gate itself needs. Keep in sync with
# the CI workflow's install steps.
DEV_DEPS  := maturin pytest ruff "kglite>=0.16.6"
# Every Python path ruff owns. One list, referenced by check and format alike,
# so the two can never drift apart and silently stop covering a directory.
PY_PATHS  := kglite_datasets benchmarks

.PHONY: gate lint lint-rust lint-py fmt fmt-check clippy ruff-check ruff-fix \
        build test determinism bench-smoke bench venv develop pytest clean \
        check-dev-docs

## Full CI-equivalent gate — the single entry point. Runs every step in
## order and stops at the first failure.
gate: check-dev-docs lint build test determinism bench-smoke
	@echo ""
	@echo "=================================================="
	@echo " gate: ALL STEPS PASSED"
	@echo "=================================================="

## 1. Lint aggregate (matches kglite's `make lint`): Rust formatting clean,
##    clippy warning-free, and ruff clean over the Python sources.
lint: lint-rust lint-py
	@echo "== lint: OK =="

lint-rust: fmt-check clippy
lint-py: ruff-check

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

## Ruff over the Python sources — lint + format check, configured in
## pyproject.toml's `[tool.ruff]` (deliberately mirroring ../kglite).
##
## This target HARD-FAILS when ruff is missing. It used to skip with a notice,
## which meant that in practice it never ran once: nobody installed ruff, so
## the gate reported "skipping" forever while 42 findings accumulated. A gate
## that can silently no-op is not a gate — so ruff is part of `make venv` now,
## and its absence is an error with the fix printed.
ruff-check:
	@echo "== ruff check + format --check =="
	@if [ ! -x "$(RUFF)" ]; then \
		echo "ERROR: ruff is not installed in $(VENV)."; \
		echo "       Run 'make venv' (provisions the dev toolchain) and retry."; \
		exit 1; \
	fi
	$(RUFF) check $(PY_PATHS)
	$(RUFF) format --check $(PY_PATHS)

## Apply ruff's autofixes + formatting (convenience; not part of the gate).
ruff-fix:
	$(RUFF) check --fix $(PY_PATHS)
	$(RUFF) format $(PY_PATHS)

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
##    csv_golden (sodir) runs under step 4; the SEC goldens
##    (test_parity_golden, test_graph_build_golden) run under `make pytest`.
##    Both need no second builder — kglite's in-tree loader is gone, so the
##    frozen goldens are the sole authority.
determinism:
	@echo "== [5/6] determinism / self-consistency =="
	@echo "  Rust sodir golden: covered by csv_golden in step 4."
	@echo "  Python SEC goldens: make pytest (test_parity_golden + test_graph_build_golden)."

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
	$(PY) benchmarks/bench_sec_extract.py

## Provision the dev venv (idempotent). This is the single place that decides
## what the Python-side gates need installed — if you add a tool to a gate,
## add it to DEV_DEPS in the same change, or the gate will no-op for everyone
## who has not installed it by hand.
venv:
	@if [ ! -x "$(PY)" ]; then \
		if command -v uv >/dev/null 2>&1; then uv venv $(VENV); \
		else python3 -m venv $(VENV); fi; \
	fi
	@if command -v uv >/dev/null 2>&1; then \
		uv pip install --python $(PY) --upgrade $(DEV_DEPS); \
	else \
		$(PY) -m pip install --upgrade $(DEV_DEPS); \
	fi
	@echo "== venv: $(VENV) provisioned =="

## Build the Python extension into the local venv.
develop:
	VIRTUAL_ENV=$(abspath $(VENV)) $(MATURIN) develop

## Run the offline Python test suite (live-API suites self-skip without their
## integration env vars). Needs `make develop` + a kglite wheel in .venv.
pytest:
	$(PY) -m pytest

## Remove build artifacts.
clean:
	cargo clean

## Mechanical bound on the gitignored dev-docs/ working folder — the one
## accumulation with no reviewer, no CI and no remote watching it grow. Unlike
## prune-target this NEVER deletes: which tier a file belongs in, and whether
## it is reproducible, is a judgement call, so the gate FAILS and hands the
## decision back. Stale purge-tier entries are reported as a warning (temp/bin
## churn is normal working state; failing on it would only teach people to
## bypass the gate). Tier lifecycles: dev-docs/README.md.
DEV_DOCS_MAX_MB := 256
.PHONY: check-dev-docs
check-dev-docs:
	@[ -d dev-docs ] || { echo "no dev-docs/ — nothing to bound"; exit 0; }; \
	mb=$$(du -sm dev-docs | cut -f1); \
	stale=$$( { find dev-docs/bench/out -mindepth 1 -maxdepth 1 -mtime +14; \
	            find dev-docs/temp      -mindepth 1 -maxdepth 1 -mtime +1;  \
	            find dev-docs/bin       -mindepth 1 -maxdepth 1 -mtime +7;  \
	          } 2>/dev/null ); \
	if [ "$${mb:-0}" -ge $(DEV_DOCS_MAX_MB) ]; then \
		echo "FAIL: dev-docs/ is $${mb} MB (>= $(DEV_DOCS_MAX_MB) MB)"; \
		echo "  largest tiers:"; \
		du -sm dev-docs/* dev-docs/bench/* 2>/dev/null | sort -rn | head -8 | sed 's/^/    /'; \
		[ -z "$$stale" ] || { echo "  past their documented lifetime:"; echo "$$stale" | sed 's/^/    /'; }; \
		echo "  -> reclaim, or move anything irreproducible to a durable tier (dev-docs/README.md)"; \
		exit 1; \
	fi; \
	echo "dev-docs/ is $${mb} MB (limit $(DEV_DOCS_MAX_MB) MB)"; \
	[ -z "$$stale" ] || { echo "WARN: past their documented lifetime (dev-docs/README.md):"; \
	                      echo "$$stale" | sed 's/^/    /'; }
