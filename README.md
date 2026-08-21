# Mohawk

Local-first desktop study app for fraud and risk engineering. Mohawk runs as a Tauri app with a SQLite cognitive core, FSRS spaced repetition, a knowledge graph, transaction rule sandbox, and a Threat Intel Desk that ingests public RSS feeds through local Ollama synthesis.

## Prerequisites

- **Rust** (1.77+) and **Cargo**
- **Node.js** 20+ and **npm**
- **[Ollama](https://ollama.com/)** running locally with a chat model pulled, e.g.:

```bash
ollama pull llama3.2
```

## Quick start

```bash
npm install
npm run dev
```

This starts the Vite dev server and opens the **Mohawk** desktop window. On Linux, `npm run dev` needs the usual local Tauri system packages (glib, webkitgtk, and related `.pc` files). Those packages are not installed by CI.

> **Important:** Do not open `http://localhost:1420` in a browser tab. IPC commands only work inside the Tauri webview. If you see `Cannot read properties of undefined (reading 'invoke')`, you are in the wrong shell.

## Threat Intel Desk workflow

1. Open **[🗲] THREAT INTEL DESK** in the sidebar.
2. Confirm **OLLAMA: ONLINE** (start Ollama if offline).
3. Optionally adjust host/port/model under **INFERENCE SETTINGS** and save.
4. Click **FORCE SYNC** to fetch RSS feeds, chunk prose, and stage courses.
5. Expand a pending course to preview nodes, edges, and cards.
6. **ACCEPT** persists the course into the graph + flashcards, or **REJECT** discards it.

Staged courses survive app restarts (stored in SQLite).

## Study + export

- **SIGNAL TRIAGE CIRCUIT** — FSRS review queue; use **EXPORT CSV** or **EXPORT ANKI** to download cards for external tools.
- **INGEST RISK INTEL** — paste raw intel text for one-off card generation.

## Scripts

| Command | Description |
|---------|-------------|
| `npm run dev` | Tauri desktop window (needs local Tauri Linux deps) |
| `npm run dev:web` | Vite only (no IPC; UI preview only) |
| `npm run build` | Typecheck + production frontend build |
| `npm run lint` | ESLint |
| `npm test` | Vitest unit tests |
| `cargo test -p mohawk-fsrs` | FSRS domain unit tests (workspace crate `mohawk-fsrs`; no Tauri) |

CI rust runs `cargo fetch` then `cargo test --offline -p mohawk-fsrs`. That job does not compile `src-tauri` / `mohawk_lib`.

## Configuration

Ollama connection settings are stored in SQLite (`app_config` keys: `ollama_host`, `ollama_port`, `ollama_model`). Defaults: `127.0.0.1:11434`, model `llama3.2`.

Set `RUST_LOG=debug` before launch for structured backend tracing.

## Architecture

- **Frontend:** React + Vite + Tailwind (`src/`)
- **Backend:** Rust + Tauri 2 + sqlx/SQLite (`src-tauri/`)
- **FSRS math:** `crates/mohawk-fsrs` (chrono + serde only)
- **Sync:** HTTPS RSS allowlist → chunk → Ollama JSON extraction → analyst review → persist

## License

Private / local use — see repository owner for terms.
