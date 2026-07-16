# kglite-datasets — Claude Code Conventions

Fetch-and-extract dataset loaders for [kglite](../kglite) — wrappers that pull a
public source (SEC EDGAR, Wikidata, Sodir / Norwegian Offshore Directorate, …)
and **emit** it to disk (CSV / blueprint JSON / dump), cached under a workdir.
This crate is deliberately **engine-free**: the CSV→graph / N-Triples→graph
*build* into a queryable kglite `KnowledgeGraph` is the caller's job via kglite's
`from_blueprint` / `load_ntriples`. Python + Rust (maturin), published to
crates.io + PyPI.

> This file holds the **standing process rules** and points at the two canonical
> layout maps. The dev working system lives in `dev-docs/` and `inbox/` (both
> gitignored) and is operated by six skills — see **The dev-docs / inbox / skills
> system** at the bottom.

## Build & test

```bash
source .venv/bin/activate && unset CONDA_PREFIX
make develop                 # maturin develop into .venv (add --release by hand for perf)
make lint                    # cargo fmt --check + clippy -D warnings (+ ruff if installed)
make test                    # cargo test --workspace (Rust: unit + csv_golden oracle)
make pytest                  # Python offline suite (needs `make develop`)
make bench                   # SEC-extract perf gate vs frozen baseline (needs venv)
make gate                    # the full CI-equivalent gate: lint → build → test → …
```

`make gate` is the single entry point mirroring kglite's gate discipline
(`make gate` runs `lint`, workspace build, workspace test, plus the honest
determinism/bench-smoke describe steps). The Rust gate is portable (no venv);
the Python `pytest` + `bench` gates need `make develop` + a kglite wheel in
`.venv`. Keep this list in sync with the `Makefile`.

## Working style

- **Understand before changing.** Reproduce a bug with evidence before fixing;
  probe real behaviour with a scratch script rather than trusting your mental
  model. For a loader, that means building a small dataset from a **cached**
  fixture and inspecting the resulting graph.
- **Offload, don't print.** Write long output (dumps, built-graph inspections,
  triage write-ups) to `dev-docs/temp/` and report the path — keep responses
  under the token gate. Heavy built graphs / raw fetches go to
  `dev-docs/bench/out/`.
- **Keep responses tight** (~400 tokens); link a file for detail.

## Code health

Each pass through a file should leave it more compartmentalised than you found it.

- **No bugs left behind.** When you encounter a pre-existing bug while working —
  even one unrelated to your task — fix it in the same change, or if it's
  genuinely out of scope, surface it explicitly (file a todo / route to kglite)
  rather than silently stepping over it. Before "fixing", confirm it's actually
  a bug and not deliberate behaviour: read the surrounding code and tests.
- **Is it ours or the engine's?** A wrong graph can come from our mapping *or*
  from a kglite engine bug. Isolate which before fixing — an engine bug is a
  `notify` to kglite, not a workaround buried in a loader.
- Factor a function when it grows past ~80 lines or handles 3+ unrelated
  concerns. Prefer small named strategy fns over long if/else chains.
- Fixing a bug — scan for the *class* of bug across the other loaders; the
  reported symptom is rarely the only instance.

## Datasets discipline

- **Tests are network-free.** A loader test builds from a small raw input cached
  under `tests/fixtures/`, never a live fetch. CI must never depend on an
  external source being up. Keep a golden-graph fixture (deterministic node/edge
  fingerprint) per bundled dataset — it's the net that catches a silent
  build-output regression.
- **Raw data and built graphs are disposable.** Never commit heavy fetched data
  or built `.kgl` graphs; they regenerate from the loader. Only small
  fixtures + small result numbers are durable. See `.gitignore`.
- **Determinism.** A loader must build the same graph from the same input —
  seed any RNG, sort where order isn't guaranteed by the source. A drifting
  golden digest at release time means either the source changed or a real
  regression; investigate, don't just re-baseline.
- **A loader isn't done until it's tested + typed + documented** — golden
  fixture, `.pyi` stub + docstring, a docs entry, a CHANGELOG line.

## Performance protocol

Before any perf-related change: baseline first (write/extend a bench covering the
touched path), **release build only** (`maturin develop --release`), trust `min`
over `median` for sub-ms benches. Measure a loader **build from a cached raw
input** — never re-download inside a bench (that measures the network). Also
watch **built-graph size** (node/edge counts + on-disk bytes): a size regression
on a bundled dataset is a real regression. Record rows in
`dev-docs/bench/results/results.csv`. See `dev-docs/bench/README.md`.

## Inbox hygiene

`inbox/unread/` (at the repo root) holds incoming feedback/bug/coordination
notes (named `YYYY-MM-DD-from-<sender>-<topic>.md`); `inbox/read/` is the
archive. The inbox is gitignored (`/inbox/`) — local working state, not
committed. Layout map: `inbox/README.md`.

**When a message has been actioned, move it from `inbox/unread/` to
`inbox/read/`.** "Actioned" means the work shipped, the bug was verified fixed,
or it's a no-action acknowledgement — not merely read. `unread/` must reflect
only what still needs doing. Append a one-line
`## Status (kglite-datasets, <date>): …` footer to substantive work-items before
moving.

**Route to the party who can act.** A note only belongs in another project's
inbox if it carries an *actionable* task for them. The most common route target
is **kglite** (`../kglite/inbox/`) — an engine bug/limitation a loader hit is the
engine's to fix. Operated via `read-inbox` (receive) + `notify` (send); don't
hand-edit the inbox.

## Commits & releases

- Never work the project directly on `main` — branch (`feat/…`, `refactor/…`,
  `fix/…`), open a draft PR (that's what runs CI), one commit per bisectable
  phase.
- Version source of truth: `[workspace.package] version` in the root
  `Cargo.toml` — all crates inherit via `version.workspace = true`. **One version
  bump per push** to `main`; fold follow-up work into the same `[x.y.z]` block if
  a release is already staged.
- Shipping is the **`release`** skill's job — it's the only thing that bumps the
  version and pushes `main` (which triggers the crates.io + PyPI publish). No
  other flow touches the version or CHANGELOG version block.
- Co-author trailer on commits per the harness convention.

## The dev-docs / inbox / skills system

This project runs a gitignored **working folder + cross-project inbox + six
skills** that operate them. Two canonical layout maps are the source of truth;
the skills point at them instead of re-describing the folders (so nothing
drifts):

- **`dev-docs/README.md`** — the working-folder map: every dir, its lifecycle
  (durable vs time-boxed/purged), and "where does X go". **Read it first.**
- **`inbox/README.md`** — the cross-project channel map: the `unread/`→`read/`
  lifecycle, the filename schema, the routing rule.

The six skills (`.claude/skills/`):
- **`add-todo`** — capture work into `todos.md` + a `plans/` detail doc (the
  single authority on todo-entry shape).
- **`phased-plan`** — run a large loader/pipeline change as gated phases:
  investigate → plan → branch + draft PR → autonomous test/commit loop →
  perf/size gate → hand to release.
- **`dev-docs-cleanup`** — purge the time-boxed dirs, tidy `todos.md`,
  soft-delete stale docs to `bin/`.
- **`read-inbox`** — triage `inbox/unread/` into durable dev-docs detail + lean
  `todos.md` backlinks, route items to the party who can act.
- **`notify`** — send a note to another project's inbox (usually kglite).
- **`release`** — ship: goal-check, gate, bump, refresh constants, publish,
  tidy.

**Skill mandates:** demand `phased-plan` for any large feature/refactor (not
plain plan mode); file backlog via `add-todo`; process the inbox via
`read-inbox` / `notify`; ship only via `release`. Durable detail lives in the
linked `plans/`/`designs/` doc, never inline in `todos.md`.
