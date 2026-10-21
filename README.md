# Subagents Visualization

Desktop app that shows a live graph of Claude Code sessions, background jobs, and the subagents they spawn.

Built with [Tauri 2](https://tauri.app): a Rust backend watches the Claude Code data directory and pushes
changes to a React + React Flow UI.

## Features

- **Live graph** of sessions, background jobs and subagents (including agents spawned by subagents).
- **Detail panel**: click a session or agent to read its transcript: prompts, replies, stored reasoning,
  every tool call with its output, and a **Changes** tab with per-file diffs.
- **Built-in terminals**: start Claude Code sessions from the app (**+ New session**) or attach to
  background sessions. Sessions hosted in the app can be prompted from the detail panel, and so can
  their subagents (the message is relayed by the parent session with `SendMessage`).
  Sessions running in other terminals are view-only.
- **Inbox**: what needs you (waiting sessions, blocked jobs, failed/finished/stuck agents), with a sound.
- **Health and overlap**: tokens per agent, optional cost estimate, warnings when agents edit the same files.
- **Search** across recent transcripts, **combined changes** per session, **re-run** an agent with an edited prompt.
- **Tray icon, mini window, start at login**: closing the window keeps the app in the tray.

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
- `src-tauri/src/transcript.rs`: transcript → display items for the detail panel
- `src-tauri/src/pty.rs`: pseudo-terminals hosting `claude` sessions
- `src-tauri/src/types.rs`: wire types (mirrors `src/types.ts`)

## License

MIT
