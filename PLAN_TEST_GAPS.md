# Test Coverage Gap Report (bd-tests-foundation)

## Current coverage snapshot (Nov 30, 2025)
- Connectors: `tests/connector_{codex,cline,gemini,claude,opencode,amp}.rs` on real-ish fixtures; only Claude still uses a `mock-claude` temp path helper; since_ts/dedupe/external_id edges not covered.
- Search/Query: `tests/ranking.rs` exercises filters/pagination and cache basics; no coverage for wildcard fallback or new detail-find highlight path.
- UI/TUI: `tests/ui_{footer,help,hotkeys,snap}.rs`, `ui_components.rs`; no automated coverage for detail find (/ n/N), breadcrumbs, bulk actions, or pane filter coexistence.
- Storage: `tests/storage.rs` happy-path only; migration/append-only and rollback not covered.
- CLI/Robot: `tests/cli_robot.rs` basic contract checks; no negative/error-path assertions.
- E2E: `e2e_index_tui.rs`, `watch_e2e.rs`, `e2e_install_easy.rs` exist but are narrow (smoke only, sparse logging assertions).
- Install scripts: `install_scripts.rs` covers checksum happy path; no bad-checksum/DEST overrides.
- Logging: `tests/logging.rs` light; no span/key-event assertions.
- Benchmarks: present (cache/index/runtime/search perf) but not tied to CI assertions.

## High-priority gaps (mapped to beads)
1) TST.1 Coverage inventory (in progress)
   - Deliver: module→test map, mock usage list, gap/fixture table.
2) TST.2 Unit: search/query + detail find (real fixtures)
   - Add coverage for wildcard fallback, cache shard eviction, agent/workspace filters, detail-find match counting and scroll targeting; assert cache/log stats.
3) TST.3 Unit: UI interactions
   - Headless ratatui tests for detail find (/ n/N), pane filter coexistence, breadcrumbs, bulk actions, focus toggles, tab cycling; verify status strings/title badges.
4) TST.4 Unit: connectors + storage (real edge fixtures)
   - since_ts routing, external_id dedupe, idx resequencing, timestamp parsing; append-only and migration guards; no mocks.
5) TST.5 E2E: CLI/TUI flows with rich logging
   - Robot/headless scripts covering search→detail find→bulk actions→filters; structured logs/traces; assert outputs.
6) TST.6 E2E: install/index/watch pipeline logging
   - install.sh/ps1 checksum good+bad, DEST override; index --full, watch-once targeted reindex; watch_state bump; detailed logs.
7) Logging assertions
   - Cross-cutting: span/key-event checks for connectors/indexer/search/watch; reusable util in tests/util.
8) Docs/help alignment
   - README/env knobs/help text kept in sync with new tests; add testing matrix section.

## Proposed test tasks (beads)
- bd-unit-connectors: fixtures + per-connector tests (see below).
- bd-unit-storage: Sqlite schema/version/transaction tests.
- bd-unit-indexer: full vs incremental vs append-only coverage.
- bd-unit-search: filter/highlight/pagination tests.
- bd-unit-tui-components: snapshot tests for bar/pills/detail tabs.
- bd-e2e-index-tui-smoke: seed fixtures, run index --full, launch tui --once, assert logs.
- bd-e2e-watch-incremental: watch run + file touch, assert targeted reindex + watch_state bump.
- bd-e2e-install-scripts: checksum pass/fail, DEST install.
- bd-logging-coverage: tracing span assertions.
- bd-ci-e2e-job: wire above into CI with timeouts.
- bd-docs-testing: README testing matrix + env knobs.

## Fixture plan
- Extend existing fixtures instead of mocks:
  - Add since_ts/append-only variants for each connector (Codex, Cline, Gemini, Claude, OpenCode, Amp).
  - Replace `mock-claude` temp paths with real fixture dir naming.
- Add installer tar/zip + matching `.sha256` pairs for positive/negative checksum tests (local `file://`, <50KB).
- Provide mini watch playground under tests/fixtures for targeted reindex checks with watch_state.json expectations.
- Shared conversation fixtures for UI/detail-find tests (messages + snippets + raw metadata).

## Next immediate steps (TST.1 → downstream)
1) Finish TST.1 inventory write-up (module→test map, mock list, gap/fixture table) and attach to bead yln.1.
2) Draft fixture matrix for connectors (since_ts + dedupe + malformed) and UI detail-find conversation set.
3) Add tracing/log capture helper in `tests/util` to support TST.5/TST.6 logging assertions.
4) Prioritize implementation order: TST.2 (search/detail-find), TST.3 (UI interactions), TST.4 (connectors/storage), then TST.5/TST.6 e2e.
