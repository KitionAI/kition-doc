/**
 * A mocked host that keeps comment threads itself (Kit UI protocol 2.6 section 7k), for
 * `tests/host-comments.mjs`: it serves `comment.create` (a new thread on the anchor the kit sends,
 * shown back with `comments/show`), hears `comments/activated`, and keeps the document's replica.
 * `?role=viewer` or `?role=commenter` opens the document read-only as that person would.
 */
import { mountKit, type KitConnection } from '@kition/kit-host'
import type { KitCommentAnchor, KitShownThread, StoredReplica } from '@kition/kit-ui-protocol'

declare global {
  interface Window {
    mock: {
      connection: KitConnection | null
      created: Array<{ anchor: KitCommentAnchor; text?: string }>
      activated: string[]
      threads: KitShownThread[]
      show: (threads: KitShownThread[]) => void
    }
  }
}

const role = new URLSearchParams(location.search).get('role')
const frame = document.getElementById('kit') as HTMLIFrameElement
const status = document.getElementById('status') as HTMLElement
const theme = { colorScheme: 'light' as const, tokens: { '--kit-primary': '#5645d4', '--kit-surface': '#ffffff', '--kit-surface-muted': '#fafaf9', '--kit-canvas': '#f6f5f4', '--kit-text': '#1a1a1a', '--kit-text-muted': '#5d5b54', '--kit-border': '#e5e3df' } }
const mock: Window['mock'] = {
  connection: null,
  created: [],
  activated: [],
  threads: [],
  show: (threads) => {
    mock.threads = threads
    mock.connection?.showComments(threads)
  },
}
window.mock = mock
let replica: StoredReplica | null = null

const bytes = await (await fetch('/sample/Sample.docx')).arrayBuffer()
frame.src = `/index.html?${Date.now()}`
mock.connection = await mountKit(frame, {
  origin: location.origin,
  host: { name: 'Comment host', version: '1' },
  document: {
    path: 'Sample.docx',
    name: 'Sample.docx',
    bytes,
    version: '1',
    ...(role === 'viewer' || role === 'commenter' ? { readOnly: true, comments: { me: `mock-${role}`, name: role, canComment: role === 'commenter', othersReadOnly: false } } : {}),
  },
  theme,
  locale: 'en-US',
  shortcuts: [],
  ui: { modes: { allowed: ['classic'], default: 'classic' } },
  handlers: {
    read: () => ({ bytes: bytes.slice(0), version: '1', size: bytes.byteLength }),
    stamp: () => ({ version: '1' }),
    write: () => ({ path: 'Sample.docx', version: '2' }),
    changed: () => {},
    replicaRead: () => replica,
    replicaWrite: ({ snapshot, blobs, fileStamp }) => {
      replica = { snapshot, blobs, fileStamp }
    },
    services: {
      'comment.create': ({ anchor, text }) => {
        mock.created.push({ anchor, ...(text ? { text } : {}) })
        const id = `t${mock.created.length}`
        mock.show([...mock.threads, { id, anchor, resolved: false, count: 1 }])
        return { thread: id }
      },
    },
    comments: { activated: (id) => mock.activated.push(id) },
  },
})
status.textContent = 'ready'
