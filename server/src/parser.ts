import fs from 'fs'
import path from 'path'
import type { AgentNode, JobNode, SessionInfo, TimelineEntry } from './types.js'

export function parseSessionFile(filePath: string): SessionInfo | null {
  try {
    const raw = fs.readFileSync(filePath, 'utf-8')
    const data = JSON.parse(raw)
    return {
      id: data.sessionId,
      pid: String(data.pid),
      cwd: data.cwd,
      status: data.status,
      startedAt: data.startedAt,
      version: data.version,
      kind: data.kind,
    }
  } catch {
    return null
  }
}

export function parseJobState(stateFile: string): JobNode | null {
  try {
    const raw = fs.readFileSync(stateFile, 'utf-8')
    const data = JSON.parse(raw)
    const jobId = path.basename(path.dirname(stateFile))

    const timelineFile = path.join(path.dirname(stateFile), 'timeline.jsonl')
    const timeline: TimelineEntry[] = []
    if (fs.existsSync(timelineFile)) {
      const lines = fs.readFileSync(timelineFile, 'utf-8').split('\n').filter(Boolean)
      for (const line of lines) {
        try { timeline.push(JSON.parse(line)) } catch { /* skip */ }
      }
    }

    return {
      id: jobId,
      name: data.name || data.intent?.slice(0, 60) || 'Unknown task',
      intent: data.intent || '',
      cwd: data.cwd || '',
      status: data.state || data.status || 'unknown',
      detail: data.detail || data.needs || '',
      inFlight: data.inFlight || { tasks: 0, queued: 0, kinds: [] },
      tokens: data.tokens || 0,
      createdAt: data.createdAt || '',
      updatedAt: data.updatedAt || '',
      suggestedReply: data.suggestedReply,
      sessionId: data.sessionId,
      timeline,
    }
  } catch {
    return null
  }
}

// Per-file read positions for incremental JSONL reading
const filePositions = new Map<string, number>()

export function readNewLines(filePath: string): any[] {
  try {
    const stats = fs.statSync(filePath)
    const lastPos = filePositions.get(filePath) ?? Math.max(0, stats.size - 65536)

    if (stats.size <= lastPos) return []

    const fd = fs.openSync(filePath, 'r')
    const buf = Buffer.alloc(stats.size - lastPos)
    fs.readSync(fd, buf, 0, buf.length, lastPos)
    fs.closeSync(fd)

    filePositions.set(filePath, stats.size)

    return buf
      .toString('utf-8')
      .split('\n')
      .filter(Boolean)
      .map(l => { try { return JSON.parse(l) } catch { return null } })
      .filter(Boolean)
  } catch {
    return []
  }
}

export function parseAgentEvents(messages: any[]): {
  spawned: AgentNode[]
  completed: { id: string; failed: boolean }[]
} {
  const spawned: AgentNode[] = []
  const completed: { id: string; failed: boolean }[] = []

  for (const msg of messages) {
    if (msg.type === 'assistant' && Array.isArray(msg.message?.content)) {
      for (const block of msg.message.content) {
        if (block.type === 'tool_use' && block.name === 'Agent') {
          spawned.push({
            id: block.id,
            sessionId: msg.sessionId || '',
            parentId: null,
            description: block.input?.description || block.input?.prompt?.slice(0, 80) || 'Agent',
            prompt: block.input?.prompt || '',
            status: 'running',
            startTime: msg.timestamp ? new Date(msg.timestamp).getTime() : Date.now(),
            agentType: block.input?.subagent_type,
          })
        }
      }
    }

    if (msg.type === 'user' && Array.isArray(msg.message?.content)) {
      for (const block of msg.message.content) {
        if (block.type === 'tool_result' && block.tool_use_id) {
          completed.push({ id: block.tool_use_id, failed: !!block.is_error })
        }
      }
    }
  }

  return { spawned, completed }
}
