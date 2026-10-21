# Subagents Visualization

Live graph of Claude Code sessions, background jobs, and the subagents they spawn.

It watches the local Claude Code data directory (`~/.claude`):

- `sessions/*.json` — active sessions
- `jobs/*/state.json` + `timeline.jsonl` — background jobs
- `projects/*/*.jsonl` — session transcripts (`Agent` tool calls become graph nodes)

## Run (current web version)

```sh
npm install
npm run dev
```

Open http://localhost:3000.
