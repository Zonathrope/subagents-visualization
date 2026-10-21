# Roadmap: making the tool more efficient

Status: milestone 1 agreed (2026-10-21); see **Milestone 1: decisions** at the end.

## Framing

While several agents run, the user spends time on four jobs:

1. **Noticing** when something needs them.
2. **Checking** that agents aren't stuck or wasting tokens.
3. **Reviewing** what agents changed.
4. **Stepping in**: redirecting, stopping or re-running an agent.

Today the tool helps with seeing and reading, but the user still has to scan the graph to find
what matters. The biggest gains come from the app pointing at what needs attention.

## Ideas

### 1. Noticing (highest value)

| Idea | What it does | Data source | Effort |
|---|---|---|---|
| **Attention inbox** | One queue of things needing the user: session waiting for input, blocked background job, agent failed or finished. Click jumps to the node. | `sessions/*.json` `status` (`busy` → `idle`), `jobs/*/state.json` `state: blocked` + `needs`, agent status | Medium |
| **Desktop notifications** | Same events as the inbox, delivered while the user works elsewhere. | Same as inbox | Small (Tauri notification plugin) |
| **Permission-prompt alerts** | Flag a session blocked on a permission prompt. | **Unverified**: need to check whether the session file or transcript exposes this state | Unknown |

### 2. Checking health

| Idea | What it does | Data source | Effort |
|---|---|---|---|
| **Tokens / cost per agent and session** | Show token usage on cards and in the panel; spot runaway agents. | `message.usage` on assistant records | Small |
| **Stuck detection** | Flag an agent that is "running" but silent for N minutes, or repeats the same failing tool call. | Transcript timestamps, tool calls + `is_error` | Small–medium |
| **Timeline view** | Per session, one bar per agent over time; shows real parallelism and what held things up. | Agent `startTime` / `endTime` | Medium |

### 3. Reviewing changes

| Idea | What it does | Data source | Effort |
|---|---|---|---|
| **File-conflict warnings** | Mark agents that ran at the same time and edited the same file. | Edit/Write/MultiEdit `file_path` per agent + run intervals | Small–medium |
| **Combined changes per session** | One diff view across all agents of a session, grouped by file. | `structuredPatch` from each agent's transcript | Medium |
| **Result first** | Show the subagent's final report at the top of its panel. | Parent's `tool_result` / `<task-notification>` for the agent | Small |
| **Open in editor** | Click a hunk to open the file at that line (e.g. `code -g file:line`). | Patch `newStart` + file path | Small |

### 4. Stepping in

| Idea | What it does | Data source | Effort |
|---|---|---|---|
| **Stop an agent** | Ask the hosting parent session to stop it (same relay as messages). | Hosted terminal + agent id | Small |
| **Re-run with a tweak** | Edit a finished agent's prompt; the parent starts it again. | Agent `prompt` | Small–medium |
| **Search across transcripts** | Find which agent touched a file or ran a command. | Transcripts in the history window | Medium |

### Also planned earlier

Tray icon with running-agent count, always-on-top mini window, start at login.

## Recommended order

1. **Attention inbox + notifications**: changes the tool from something you watch into something
   that tells you when to look.
2. **File-conflict warnings**: cheap from data already parsed, and catches real damage in parallel runs.
3. **Tokens on cards + stuck detection**: small changes that make the graph informative at a glance.

Timeline, combined diffs, search and re-run are worthwhile but matter less day to day.

## Open questions (answered)

- Which job costs the most time? Noticing.
- Parallel agents on one repo? Yes, mostly in separate worktrees, so overlap matters at merge time.
- Can a permission-prompt wait be detected from disk? No; stuck detection is the stand-in.

## Milestone 1: decisions (agreed 2026-10-21)

**Context.** 1–2 sessions, each often with parallel subagents; agents mostly in separate worktrees;
the app stays visible on a second monitor; the main pain is noticing what needs attention.
Billing varies between subscription and API.

**Facts from disk** (checked against `~/.claude`):
- A pending permission prompt is not recorded anywhere while it is pending; it looks like a long-running tool.
- Sessions only report `busy` / `idle`. Background jobs report `state: blocked` with `needs` and `suggestedReply`.
- `message.usage` repeats on every line of one response (same `message.id`); count each response once,
  taking the line with a non-null `stop_reason` (subagent transcripts stream partial counts first).

**Attention inbox**
- Replaces the jobs sidebar on the right; blocked jobs appear there as items (job nodes stay in the graph).
- Events: session waiting (busy → idle), job blocked, agent failed, agent stuck, agent finished.
- Delivery: in-app highlight + sound, no OS toasts. Urgent sound for the first four; finished only highlights.
- An item clears when the situation resolves or when its node is opened; manual dismiss too.
- Only transitions seen while the app runs create items (no flood of old events at startup);
  jobs already blocked at startup are listed without a sound.

**Stuck detection.** A running agent with no new transcript line for N minutes; default 3, configurable.
Also the only way to surface a hidden permission prompt.

**Tokens.** Cards show `42k ctx · 8.1k out` (latest context size; total output). Optional cost estimate
from a built-in price table, off by default, labeled as an estimate.

**Worktree overlap.** Badge on agents whose edits touched the same file as another agent or the main
session; the session panel lists overlapping files and who touched them. Paths are compared relative to the
repo root (`.claude/worktrees/<name>/` stripped).

**Extras in milestone 1**
- Result first: an agent's final report pinned at the top of its panel.
- Open file: the system default app, no line number (script and executable types are revealed in
  Explorer instead of opened, so clicking never runs code).
- Stop agent: the hosting parent session is asked to stop it (in-app sessions only).
- Timeline: a tab in the session panel, one bar per agent; clicking a bar opens the agent.

**Settings.** Small settings popover (stuck threshold, sounds, cost estimate), stored locally. Sounds are
generated in-app.

## Milestone 2 (built 2026-10-21)

- **Tray icon**: tooltip with running / needs-you counts, amber dot on the icon while something needs
  attention; menu with Show, Mini window, Start at login, Quit. Closing the main window hides it to the
  tray (hosted sessions keep running, alerts keep coming); quit from the tray.
- **Start at login**: in the tray menu and the settings popover.
- **Mini window**: compact, always on top; mirrors the main window's inbox (dismissals sync both ways,
  only the main window plays sounds) and lists running agents; clicking opens the node in the main window.
- **Search**: header box, case-insensitive over prompts, replies, tool inputs and results in the history
  window (24 h); newest first, max 200 hits.
- **Combined changes**: a session's Changes tab shows its own and all its agents' edits, labeled by
  author, grouped by repo-relative file.
- **Re-run with a tweak**: edit an agent's prompt; the hosting parent session starts a new agent with it.

Also fixed: the inbox no longer reports history loaded at startup as new events (it waits for the
backend's startup scan to finish).
