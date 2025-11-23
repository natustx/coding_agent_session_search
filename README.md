# coding-agent-search

Unified TUI for local coding-agent history search (Codex, Claude Code, Gemini CLI, Cline, OpenCode, Amp).

## Toolchain & dependency policy
- Toolchain: pinned to latest Rust nightly via `rust-toolchain.toml` (rustfmt, clippy included).
- Crates: track latest releases with wildcard constraints (`*`). Run `cargo update` regularly to pick up fixes.
- Edition: 2024.

## Env loading
Load `.env` at startup using dotenvy (see `src/main.rs`); do not use `std::env::var` without calling `dotenvy::dotenv().ok()` first.

## Dev commands (nightly)
- `cargo check --all-targets`
- `cargo clippy --all-targets -- -D warnings`
- `cargo fmt --check`

## Install
- Shell (Linux/macOS): `curl -fsSL https://raw.githubusercontent.com/coding-agent-search/coding-agent-search/main/install.sh | sh` (supports `--version`, `--dest`, `--easy-mode`, `OWNER`, `REPO`, `CHECKSUM`). Provide `CHECKSUM` for release tarballs if you want verification; otherwise it will skip.
- PowerShell (Windows): `irm https://raw.githubusercontent.com/coding-agent-search/coding-agent-search/main/install.ps1 | iex`.
- Binaries are built via cargo-dist (`.github/workflows/dist.yml`).

## Usage (TUI help)
- Footer lists main hotkeys; toggle detailed legend with `?` inside TUI.
- `coding-agent-search index --full` – rebuild SQLite + Tantivy (no source file deletion).
- `coding-agent-search index --watch` – watch known agent roots and re-index touched connectors.
- `coding-agent-search tui` – launch TUI.
- `coding-agent-search completions <shell>` – emit shell completions.
- `coding-agent-search man` – emit man page.

## Structure (scaffold)
- `src/main.rs` – entrypoint wiring tracing + dotenvy
- `src/lib.rs` – library entry
- `src/config/` – configuration layer
- `src/storage/` – SQLite backend
- `src/search/` – Tantivy/FTS
- `src/connectors/` – agent log parsers
- `src/indexer/` – indexing orchestration
- `src/ui/` – Ratatui interface
- `src/model/` – domain types

## Connectors & watch coverage
- Codex CLI: `~/.codex/sessions/**/rollout-*.jsonl`
- Cline: VS Code globalStorage `saoudrizwan.claude-dev` task dirs
- Gemini CLI: `~/.gemini/tmp/**`
- Claude Code: `~/.claude/projects/**` + `.claude` / `.claude.json`
- OpenCode: `.opencode` SQLite DBs (project/local/global)
- Amp: `sourcegraph.amp` globalStorage + `~/.local/share/amp` JSON caches

Watch mode listens on these roots and triggers connector-specific scans; `--full` truncates DB/index before ingest (non-destructive to log files).
