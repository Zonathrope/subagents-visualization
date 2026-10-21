import express from 'express'
import { createServer } from 'http'
import { WebSocketServer, WebSocket } from 'ws'
import cors from 'cors'
import { GraphStateManager } from './state.js'
import { startWatcher } from './watcher.js'
import type { WsMessage } from './types.js'

const app = express()
app.use(cors())
app.use(express.json())

const server = createServer(app)
const wss = new WebSocketServer({ server })
const state = new GraphStateManager()
const clients = new Set<WebSocket>()

function broadcast(msg: WsMessage) {
  const data = JSON.stringify(msg)
  for (const client of clients) {
    if (client.readyState === WebSocket.OPEN) {
      client.send(data)
    }
  }
}

wss.on('connection', ws => {
  clients.add(ws)
  ws.send(JSON.stringify({ type: 'snapshot', data: state.getSnapshot() }))
  ws.on('close', () => clients.delete(ws))
  ws.on('error', () => clients.delete(ws))
})

app.get('/api/snapshot', (_req, res) => {
  res.json(state.getSnapshot())
})

startWatcher(state, broadcast)

const PORT = 3001
server.listen(PORT, () => {
  console.log(`Subagents viz server → http://localhost:${PORT}`)
})
