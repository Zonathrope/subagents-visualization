import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import {
  Background,
  Controls,
  MiniMap,
  ReactFlow,
  useEdgesState,
  useNodesState,
} from '@xyflow/react'
import type { Edge, Node } from '@xyflow/react'
import '@xyflow/react/dist/style.css'

import type { GraphState, SessionSummary } from '../types'
import { DIM_AFTER_MS, liveSessionIds } from '../liveness'
import { AgentNodeComponent, JobNodeComponent, PastSessionNodeComponent, SessionNodeComponent } from './nodes'
import { applyDagreLayout } from './layout'

const nodeTypes = {
  session: SessionNodeComponent,
  pastSession: PastSessionNodeComponent,
  job: JobNodeComponent,
  agent: AgentNodeComponent,
}

/**
 * Live sessions, jobs and their agents. With `showHistory`, sessions that have ended
 * (within the backend's history window) are added as faded nodes with their agents.
 */
function buildElements(state: GraphState, showHistory: boolean, now: number): { nodes: Node[]; edges: Edge[] } {
  const nodes: Node[] = []
  const edges: Edge[] = []

  for (const s of state.sessions) {
    nodes.push({ id: `session-${s.id}`, type: 'session', data: s, position: { x: 0, y: 0 } })
  }

  for (const j of state.jobs) {
    nodes.push({ id: `job-${j.id}`, type: 'job', data: j, position: { x: 0, y: 0 } })

    // Connect to session if known, else create a synthetic root
    const parentSession = j.sessionId
      ? state.sessions.find(s => s.id === j.sessionId)
      : undefined

    if (parentSession) {
      edges.push({
        id: `e-session-${parentSession.id}-job-${j.id}`,
        source: `session-${parentSession.id}`,
        target: `job-${j.id}`,
        animated: j.status === 'running' || j.inFlight.tasks > 0,
      })
    } else {
      // Orphan job — try to match by cwd to a session
      const matchSession = state.sessions.find(s => j.cwd && s.cwd === j.cwd)
      if (matchSession) {
        edges.push({
          id: `e-session-${matchSession.id}-job-${j.id}`,
          source: `session-${matchSession.id}`,
          target: `job-${j.id}`,
          animated: j.status === 'running',
        })
      }
    }
  }

  const live = liveSessionIds(state)
  const pastNodes = new Set<string>()
  const addPastSession = (summary: SessionSummary) => {
    const id = `past-${summary.id}`
    if (pastNodes.has(id)) return id
    pastNodes.add(id)
    nodes.push({ id, type: 'pastSession', data: summary, position: { x: 0, y: 0 } })
    return id
  }

  if (showHistory) {
    for (const summary of state.summaries) {
      if (!live.has(summary.id)) addPastSession(summary)
    }
  }

  /** The node a top-level agent hangs off: its live session, its background job, or its ended session. */
  const rootFor = (sessionId: string): string | undefined => {
    if (state.sessions.some(s => s.id === sessionId)) return `session-${sessionId}`
    const job = state.jobs.find(j => j.sessionId === sessionId || j.resumeSessionId === sessionId)
    if (job) return `job-${job.id}`
    if (!showHistory) return undefined
    const summary = state.summaries.find(s => s.id === sessionId)
    return addPastSession(summary ?? { id: sessionId, cwd: '', lastActivity: 0, transcriptPath: '' })
  }

  for (const a of state.agents) {
    if (!live.has(a.sessionId) && !showHistory) continue

    const finishedLongAgo = a.status !== 'running' && now - (a.endTime ?? a.startTime) > DIM_AFTER_MS
    nodes.push({
      id: `agent-${a.id}`,
      type: 'agent',
      data: a,
      position: { x: 0, y: 0 },
      className: finishedLongAgo ? 'dimmed' : undefined,
    })

    const source = a.parentId ? `agent-${a.parentId}` : rootFor(a.sessionId)
    if (source) {
      edges.push({
        id: `e-${source}-agent-${a.id}`,
        source,
        target: `agent-${a.id}`,
        animated: a.status === 'running',
      })
    }
  }

  return { nodes: applyDagreLayout(nodes, edges), edges }
}

export default function AgentGraph({
  state,
  showHistory,
  now,
  selectedId,
  onSelect,
}: {
  state: GraphState
  showHistory: boolean
  now: number
  selectedId: string | null
  onSelect: (nodeId: string) => void
}) {
  const [nodes, setNodes, onNodesChange] = useNodesState<Node>([])
  const [edges, setEdges, onEdgesChange] = useEdgesState<Edge>([])
  const prevRef = useRef<string>('')
  const [needsFit, setNeedsFit] = useState(false)
  const rfInstance = useRef<any>(null)

  const { nodes: nextNodes, edges: nextEdges } = useMemo(
    () => buildElements(state, showHistory, now),
    [state, showHistory, now],
  )

  useEffect(() => {
    const key = JSON.stringify(nextNodes.map(n => n.id))
    if (key !== prevRef.current) {
      prevRef.current = key
      setNeedsFit(true)
    }
    setNodes(nextNodes.map(n => ({ ...n, selected: n.id === selectedId })))
    setEdges(nextEdges)
  }, [nextNodes, nextEdges, selectedId, setNodes, setEdges])

  useEffect(() => {
    if (needsFit && rfInstance.current) {
      setTimeout(() => rfInstance.current?.fitView({ padding: 0.15, duration: 400 }), 50)
      setNeedsFit(false)
    }
  }, [needsFit])

  // The detail panel narrows the canvas, so bring the selected node into view.
  useEffect(() => {
    const rf = rfInstance.current
    const node = selectedId ? rf?.getNode(selectedId) : undefined
    if (!node) return
    const t = setTimeout(() => {
      rf.setCenter(node.position.x + 140, node.position.y + 55, {
        zoom: Math.max(rf.getZoom(), 0.9),
        duration: 300,
      })
    }, 50)
    return () => clearTimeout(t)
  }, [selectedId])

  const onInit = useCallback((instance: any) => {
    rfInstance.current = instance
  }, [])

  const isEmpty = nextNodes.length === 0

  return (
    <div style={{ flex: 1, position: 'relative' }}>
      <ReactFlow
        nodes={nodes}
        edges={edges}
        onNodesChange={onNodesChange}
        onEdgesChange={onEdgesChange}
        nodeTypes={nodeTypes}
        onInit={onInit}
        onNodeClick={(_, node) => onSelect(node.id)}
        fitView
        fitViewOptions={{ padding: 0.15 }}
        minZoom={0.1}
        maxZoom={2}
        colorMode="dark"
      >
        <Background color="#1e2130" gap={24} />
        <Controls />
        <MiniMap nodeColor={miniMapColor} maskColor="rgba(0,0,0,0.6)" />
      </ReactFlow>

      {isEmpty && (
        <div className="empty-state">
          <div className="empty-icon">◎</div>
          <div className="empty-title">No active Claude sessions</div>
          <div className="empty-sub">Start Claude Code in any directory to see sessions appear here.</div>
        </div>
      )}
    </div>
  )
}

function miniMapColor(node: Node) {
  if (node.type === 'session') return '#3b82f6'
  if (node.type === 'pastSession') return '#334155'
  if (node.type === 'job') return '#06b6d4'
  return '#a855f7'
}
