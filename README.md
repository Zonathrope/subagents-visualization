# Subagents Visualization

Desktop app that shows a live graph of Claude Code sessions, background jobs, and the subagents they spawn.

Built with [Tauri 2](https://tauri.app): a Rust backend watches the Claude Code data directory and pushes
changes to a React + React Flow UI.

## What it watches

The data directory is `$CLAUDE_CONFIG_DIR` if set, otherwise `~/.claude`:

- `sessions/*.json`: active sessions
- `jobs/*/state.json` + `timeline.jsonl`: background jobs
- `projects/*/*.jsonl`: session transcripts (`Agent` tool calls become graph nodes)

## Develop

Requires Node.js and a Rust toolchain ([Tauri prerequisites](https://tauri.app/start/prerequisites/)).

```sh
npm install
npm run dev     # launches the desktop app with hot reload
npm test        # Rust unit tests
npm run build   # release build + installers in src-tauri/target/release/bundle/
```

## Layout

- `src/`: React UI
- `src-tauri/src/watcher.rs`: file watching, debouncing, polling fallback
- `src-tauri/src/parser.rs`: lenient parsing of session, job and transcript files
- `src-tauri/src/state.rs`: in-memory graph state
- `src-tauri/src/types.rs`: wire types (mirrors `src/types.ts`)

## License

MIT
