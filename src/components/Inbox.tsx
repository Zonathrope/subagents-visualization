import type { AttentionItem, AttentionKind } from '../attention'
import { URGENT } from '../attention'
import type { JobNode } from '../types'

const LABEL: Record<AttentionKind, string> = {
  waiting: 'Waiting for you',
  blocked: 'Job blocked',
  failed: 'Failed',
  stuck: 'Possibly stuck',
  finished: 'Finished',
}

export default function Inbox({
  items,
  jobs,
  onOpen,
  onDismiss,
  onAttach,
}: {
  items: AttentionItem[]
  jobs: JobNode[]
  onOpen: (nodeId: string) => void
  onDismiss: (key: string) => void
  onAttach: (job: JobNode) => void
}) {
  return (
    <aside className="sidebar inbox">
      <div className="sidebar-title">
        Inbox{items.length > 0 && <span className="inbox-count">{items.length}</span>}
      </div>
      {items.length === 0 && <div className="inbox-empty">All clear. Nothing needs you right now.</div>}
      {items.map(item => (
        <div
          key={item.key}
          className={`inbox-item inbox-${item.kind} ${URGENT.has(item.kind) ? 'inbox-urgent' : ''}`}
          onClick={() => onOpen(item.nodeId)}
        >
          <div className="inbox-head">
            <span className="inbox-kind">{LABEL[item.kind]}</span>
            <span className="inbox-time">{new Date(item.at).toLocaleTimeString()}</span>
            <button
              className="inbox-dismiss"
              title="Dismiss"
              onClick={e => {
                e.stopPropagation()
                onDismiss(item.key)
              }}
            >
              ×
            </button>
          </div>
          <div className="inbox-title" title={item.title}>{item.title}</div>
          {item.detail && <div className="inbox-detail">{item.detail}</div>}
          {item.suggestedReply && <div className="inbox-reply">Suggested reply: “{item.suggestedReply}”</div>}
        </div>
      ))}

      {jobs.length > 0 && (
        <div className="inbox-jobs">
          <div className="sidebar-title">Background jobs</div>
          {jobs.map(job => (
            <div key={job.id} className="inbox-job" onClick={() => onOpen(`job-${job.id}`)} title={job.detail || job.name}>
              <span className={`inbox-job-state inbox-job-${job.status}`}>{job.status}</span>
              <span className="inbox-job-name">{job.name}</span>
              <button
                className="sidebar-attach"
                title="Open this background session in a terminal (claude attach)"
                onClick={e => {
                  e.stopPropagation()
                  onAttach(job)
                }}
              >
                Attach
              </button>
            </div>
          ))}
        </div>
      )}
    </aside>
  )
}
