import chokidar from 'chokidar'
import path from 'path'
import os from 'os'
import { GraphStateManager } from './state.js'
import { parseAgentEvents, parseJobState, parseSessionFile, readNewLines } from './parser.js'
import type { WsMessage } from './types.js'

const HOME = os.homedir()
const CLAUDE_DIR = path.join(HOME, '.claude')
const SESSIONS_DIR = path.join(CLAUDE_DIR, 'sessions')
const JOBS_DIR = path.join(CLAUDE_DIR, 'jobs')
const PROJECTS_DIR = path.join(CLAUDE_DIR, 'projects')

export function startWatcher(
  state: GraphStateManager,
  broadcast: (msg: WsMessage) => void,
) {
  // --- Sessions (watch dir, filter .json) ---
  chokidar
    .watch(SESSIONS_DIR, { ignoreInitial: false, persistent: true })
    .on('add', f => { if (f.endsWith('.json')) onSessionFile(f, state, broadcast) })
    .on('change', f => { if (f.endsWith('.json')) onSessionFile(f, state, broadcast) })
    .on('unlink', f => {
      if (!f.endsWith('.json')) return
      const pid = path.basename(f, '.json')
      const id = state.removeSessionByPid(pid)
      if (id) broadcast({ type: 'session_removed', id })
    })

  // --- Jobs (watch dir, filter state.json at depth 2) ---
  chokidar
    .watch(JOBS_DIR, { ignoreInitial: false, persistent: true, depth: 2 })
    .on('add', f => { if (path.basename(f) === 'state.json') onJobFile(f, state, broadcast) })
    .on('change', f => { if (path.basename(f) === 'state.json') onJobFile(f, state, broadcast) })

  // --- Project transcripts (watch dir, filter .jsonl, depth 2) ---
  chokidar
    .watch(PROJECTS_DIR, { ignoreInitial: false, persistent: true, depth: 2 })
    .on('add', f => { if (f.endsWith('.jsonl')) onTranscript(f, state, broadcast) })
    .on('change', f => { if (f.endsWith('.jsonl')) onTranscript(f, state, broadcast) })

  console.log('Watching:', SESSIONS_DIR)
  console.log('Watching:', JOBS_DIR)
  console.log('Watching:', PROJECTS_DIR)
}

function onSessionFile(
  filePath: string,
  state: GraphStateManager,
  broadcast: (msg: WsMessage) => void,
) {
  const info = parseSessionFile(filePath)
  if (info) {
    state.updateSession(info)
    broadcast({ type: 'session_update', data: info })
  }
}

function onJobFile(
  filePath: string,
  state: GraphStateManager,
  broadcast: (msg: WsMessage) => void,
) {
  const job = parseJobState(filePath)
  if (job) {
    state.updateJob(job)
    broadcast({ type: 'job_update', data: job })
  }
}

function onTranscript(
  filePath: string,
  state: GraphStateManager,
  broadcast: (msg: WsMessage) => void,
) {
  const messages = readNewLines(filePath)
  if (messages.length === 0) return

  const { spawned, completed } = parseAgentEvents(messages)

  for (const agent of spawned) {
    const node = state.updateAgent(agent)
    broadcast({ type: 'agent_update', data: node })
  }

  for (const { id, failed } of completed) {
    const node = state.completeAgent(id, failed)
    if (node) broadcast({ type: 'agent_update', data: node })
  }
}
