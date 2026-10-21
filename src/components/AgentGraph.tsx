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

import type { GraphState } from '../types'
import { AgentNodeComponent, JobNodeComponent, SessionNodeComponent } from './nodes'
import { applyDagreLayout } from './layout'

const nodeTypes = {
  session: SessionNodeComponent,
  job: JobNodeComponent,
  agent: AgentNodeComponent,
}

function buildElements(state: GraphState): { nodes: Node[]; edges: Edge[] } {
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

  for (const a of state.agents) {
    nodes.push({ id: `agent-${a.id}`, type: 'agent', data: a, position: { x: 0, y: 0 } })

    if (a.parentId) {
      edges.push({
        id: `e-agent-${a.parentId}-${a.id}`,
        source: `agent-${a.parentId}`,
        target: `agent-${a.id}`,
        animated: a.status === 'running',
      })
    } else {
      const parentSession = state.sessions.find(s => s.id === a.sessionId)
      if (parentSession) {
        edges.push({
          id: `e-session-${parentSession.id}-agent-${a.id}`,
          source: `session-${parentSession.id}`,
          target: `agent-${a.id}`,
          animated: a.status === 'running',
        })
      }
    }
  }

  return { nodes: applyDagreLayout(nodes, edges), edges }
}

export default function AgentGraph({
  state,
  selectedId,
  onSelect,
}: {
  state: GraphState
  selectedId: string | null
  onSelect: (nodeId: string) => void
}) {
  const [nodes, setNodes, onNodesChange] = useNodesState<Node>([])
  const [edges, setEdges, onEdgesChange] = useEdgesState<Edge>([])
  const prevRef = useRef<string>('')
  const [needsFit, setNeedsFit] = useState(false)
  const rfInstance = useRef<any>(null)

  const { nodes: nextNodes, edges: nextEdges } = useMemo(() => buildElements(state), [state])

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

  const isEmpty = state.sessions.length === 0 && state.jobs.length === 0 && state.agents.length === 0

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
  if (node.type === 'job') return '#06b6d4'
  return '#a855f7'
}
