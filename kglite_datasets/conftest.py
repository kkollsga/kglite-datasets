"""Suite-honesty accounting.

Before this file the default run reported ``4 passed, 24 skipped`` — a suite
that spends 86% of its collection doing nothing while printing a green line.
Two different things were hiding in that number: tests that genuinely need the
live SEC API, and a module that had been wholesale-skipped since a rewrite and
no longer even imported. Nothing distinguished them, so neither got looked at.

Two mechanisms fix that, and they are deliberately different in kind:

* **A summary line, always printed.** Every run states how many tests were
  skipped and why, so "N passed" is never the whole story a reader gets.
* **A gate.** Every skip must be *accounted for* — either the test carries the
  ``live`` marker (it needs a network service, and the env var that enables it
  is named in the summary), or its reason is on the short allow-list below.
  An unaccounted skip fails the run.

The gate is the part that matters. A skip is the one test outcome that costs
nothing to add and never turns red, so left ungoverned it accumulates: a
``@pytest.mark.skip("pending the rewrite")`` outlives the rewrite by years and
quietly subtracts coverage that the suite still claims to have. Making it fail
forces the choice at the moment it is made — mark it as opt-in with a real
enabling condition, or delete it, because a test that can never run is not a
test.
"""

from __future__ import annotations

from typing import Any

import pytest

# Skips that are legitimate but cannot carry a marker, because the item never
# gets created — a module-level `pytest.importorskip` aborts collection.
# Keep this list short; every entry is coverage nobody is watching.
ALLOWED_SKIP_REASONS: tuple[str, ...] = (
    # `kglite` is a hard runtime dependency (pyproject `dependencies`), so this
    # only fires in an environment that is already broken. Allowed so the suite
    # reports the missing engine instead of an avalanche of import errors.
    "kglite engine not installed",
)

# Env vars that turn opt-in suites on, named in the summary so a reader knows
# how to run what was skipped rather than having to go find out.
LIVE_ENV_VARS: tuple[str, ...] = ("KGLITE_SEC_INTEGRATION",)

_LIVE_NODEIDS: set[str] = set()
_SKIPPED: list[tuple[str, str]] = []


def pytest_collection_modifyitems(config: pytest.Config, items: list[pytest.Item]) -> None:
    _LIVE_NODEIDS.clear()
    _LIVE_NODEIDS.update(item.nodeid for item in items if item.get_closest_marker("live"))


def pytest_runtest_logreport(report: Any) -> None:
    if report.skipped and report.when == "setup":
        reason = report.longrepr[2] if isinstance(report.longrepr, tuple) else str(report.longrepr)
        _SKIPPED.append((report.nodeid, reason))


def pytest_collectreport(report: Any) -> None:
    # Module-level skips (importorskip, a module-scope `pytestmark`) surface
    # here rather than as test items.
    if report.skipped:
        reason = report.longrepr[2] if isinstance(report.longrepr, tuple) else str(report.longrepr)
        _SKIPPED.append((report.nodeid, reason))


def _unaccounted(nodeid: str, reason: str) -> bool:
    if nodeid in _LIVE_NODEIDS:
        return False
    # A module-level skip reports the module's nodeid, not the item's.
    if any(live.startswith(nodeid + "::") for live in _LIVE_NODEIDS):
        return False
    return not any(allowed in reason for allowed in ALLOWED_SKIP_REASONS)


def pytest_terminal_summary(terminalreporter: Any, exitstatus: int, config: pytest.Config) -> None:
    passed = len(terminalreporter.stats.get("passed", []))
    unaccounted = [(n, r) for n, r in _SKIPPED if _unaccounted(n, r)]
    live = len(_SKIPPED) - len(unaccounted)

    detail = f"{passed} passed"
    if live:
        env = " / ".join(f"{v}=1" for v in LIVE_ENV_VARS)
        detail += f", {live} skipped (opt-in live-network suites; enable with {env})"
    else:
        detail += ", 0 accounted skips"
    terminalreporter.write_sep("=", f"suite accounting: {detail}", bold=True)

    if unaccounted:
        terminalreporter.write_line("")
        terminalreporter.write_line(
            "UNACCOUNTED SKIPS — a skipped test must carry @pytest.mark.live "
            "(needs a network service) or be deleted. A test that can never run "
            "is not coverage:",
            red=True,
        )
        for nodeid, reason in unaccounted:
            terminalreporter.write_line(f"  {nodeid}: {reason}", red=True)


def pytest_sessionfinish(session: pytest.Session, exitstatus: int) -> None:
    if any(_unaccounted(n, r) for n, r in _SKIPPED):
        session.exitstatus = pytest.ExitCode.TESTS_FAILED
