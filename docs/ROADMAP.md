# Roadmap: making the tool more efficient

Status: proposal (2026-10-21). Nothing here is built yet.

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

## Open questions

- Which of the four jobs takes most of the user's time in practice?
- How often do agents run in parallel on the same repo? (Drives the priority of conflict warnings.)
- Can a permission-prompt wait be detected from files on disk?
