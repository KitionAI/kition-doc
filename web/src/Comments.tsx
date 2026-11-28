import { useCallback, useEffect, useMemo, useState } from 'react'
import {
  Button,
  CommentsPanel,
  IconButton,
  useCommentAnchors,
  useKitEditor,
  type CommentMarker,
  type CommentMarkerRect,
  type CommentsPanelProps,
  type HostConnection,
  type RasterDecoration,
  type RasterRect,
} from '@kition/sdk/editor'
import { MessageSquarePlusIcon as MessageSquarePlus, XIcon as X } from '@kition/tessera/icons/react'
import { focusText } from './focus'

/**
 * Comment threads (Kition platform rule M23): kit-editor's `CommentsPanel` over the engine's
 * `comment.threads`, with a marker on the page where each open thread's range starts
 * (`useCommentAnchors`). The engine keeps each range in the text as it is edited, so markers
 * follow typing; the replica keeps the threads beside the content (`kdm-model`'s `comments`).
 */

/** `@kition/sdk/kdm`'s comment thread and anchor, as kit-editor's panel takes them. */
export type CommentThread = CommentsPanelProps['threads'][number]
type CommentAnchor = NonNullable<CommentsPanelProps['pendingAnchor']>

/** A thread's anchor as `comment.threads` answers it: where its range starts on its page. */
type Placed = CommentAnchor & { place?: { page: number; x: number; y: number; height: number } }

/** Who comments here and how (Kit UI protocol `document.comments`, 1.6); absent: unknown, as `readOnly` allows. */
export function commentAccess(host: HostConnection) {
  const comments = host.init.document.comments
  return {
    me: comments?.me ?? null,
    name: comments?.name,
    canComment: comments ? comments.canComment : host.readOnly !== true,
    othersReadOnly: comments?.othersReadOnly ?? false,
  }
}

/**
 * A commenter's session (read-only content, comments allowed): kit-editor's read-only session lets
 * only the manifest's `commentCommands` through, and the engine writes the comments and refuses
 * every other edit itself (`setCommentOnly`).
 */
export function commenting(host: HostConnection) {
  return host.readOnly === true && host.init.document.comments?.canComment === true
}

/** Where `anchor` is on the canvas, from the pages' places. */
export function anchorRect(anchor: CommentAnchor, pages: RasterRect[]): CommentMarkerRect | null {
  const place = (anchor as Placed).place
  const page = place ? pages[place.page] : undefined
  if (!place || !page) return null
  return { x: page.x0 + place.x, y: page.y0 + place.y, width: 1, height: place.height }
}

/** Draws each marker as a small flag beside its range's start. */
export function markerDecoration(markers: CommentMarker[]): RasterDecoration | null {
  if (!markers.length) return null
  return (context, toScreen) => {
    const accent = getComputedStyle(document.documentElement).getPropertyValue('--kit-primary').trim() || '#2b5cd9'
    for (const marker of markers) {
      const at = toScreen(marker.rect.x, marker.rect.y)
      const bottom = toScreen(marker.rect.x, marker.rect.y + marker.rect.height)
      context.save()
      context.globalAlpha = marker.active ? 1 : 0.8
      context.fillStyle = accent
      context.fillRect(at.x - 1, at.y, 2, Math.max(4, bottom.y - at.y))
      context.beginPath()
      context.moveTo(at.x + 1, at.y)
      context.lineTo(at.x + 9, at.y + 4)
      context.lineTo(at.x + 1, at.y + 8)
      context.closePath()
      context.fill()
      context.restore()
    }
  }
}

export function useCommentMarkers(threads: CommentThread[], pages: RasterRect[], active: string | null) {
  const locate = useCallback((anchor: CommentAnchor) => anchorRect(anchor, pages), [pages])
  return useCommentAnchors({ threads, locate, activeThread: active, revision: pages })
}

/** The thread `comment.add` answered (its result, as the engine's JSON). */
function threadOf(made: unknown): string | null {
  try {
    const value = (typeof made === 'string' ? JSON.parse(made) : made) as { thread?: unknown } | null
    return typeof value?.thread === 'string' ? value.thread : null
  } catch {
    return null
  }
}

/**
 * `onNew`: in a host that keeps comment threads (`HostComments.tsx`), New comment starts the host's
 * thread on the selection instead of a Word comment (null: this person starts none); the file's
 * own Word comments stay listed here, to reply to and resolve as before.
 */
export function CommentsPane({ threads, active, onActive, onClose, onNew, start = 0, strings }: {
  threads: CommentThread[]
  active: string | null
  onActive: (thread: string | null) => void
  onClose: () => void
  onNew?: (() => void) | null
  /**
   * Counts New Comment asked for (Cmd+Alt+A, Review, Insert, the right-click menu; stage 6A item
   * 5, walkthrough K3): each opens the new comment's box on the selection, which takes the keyboard
   * (the selected text stays selected, so typing goes into the comment, not over the text). Enter
   * is a new line, Cmd+Enter or Comment adds it, Escape or Cancel drops it (an empty one is never
   * added); the keyboard goes back to the text either way.
   */
  start?: number
  strings: { title: string; newComment: string; close: string; selectFirst: string; labels: CommentsPanelProps['labels'] }
}) {
  const { run, host } = useKitEditor()
  const access = useMemo(() => commentAccess(host), [host])
  const [pending, setPending] = useState<CommentAnchor | null>(null)
  useEffect(() => {
    if (start > 0 && access.canComment) setPending({ object: 'selection', field: 'text' })
  }, [start, access.canComment])
  const who = { ...(access.me ? { author: access.me } : {}), ...(access.name ? { name: access.name } : {}) }
  const created = () => new Date().toISOString()
  const go = (thread: string) => {
    onActive(thread)
    void run('comment.goto', { thread })
  }
  return (
    <aside className="kui-panel doc-comments" data-side="right" data-testid="doc-comments">
      <div className="doc-comments-header">
        <strong>{strings.title}</strong>
        <span style={{ flex: 1 }} />
        {access.canComment && onNew !== null ? (
          <Button
            variant="ghost"
            data-testid="doc-comments-new"
            onClick={() => (onNew ? onNew() : setPending({ object: 'selection', field: 'text' }))}
          >
            <MessageSquarePlus size={14} aria-hidden="true" /> {strings.newComment}
          </Button>
        ) : null}
        <IconButton icon={X} label={strings.close} onClick={onClose} />
      </div>
      <CommentsPanel
        threads={threads}
        me={access.me}
        canComment={access.canComment}
        othersReadOnly={access.othersReadOnly}
        pendingAnchor={pending}
        activeThread={active}
        onActivate={go}
        anchorLabel={(anchor) => (anchor.quote ? `“${anchor.quote}”` : anchor.object === 'selection' ? strings.selectFirst : null)}
        onAdd={(_, body) => {
          setPending(null)
          void run('comment.add', { text: body, ...who, created: created() }).then((made) => {
            const thread = threadOf(made)
            if (thread) onActive(thread)
            window.requestAnimationFrame(() => focusText())
          })
        }}
        onCancelNew={() => {
          setPending(null)
          window.requestAnimationFrame(() => focusText())
        }}
        onReply={(thread, body) => void run('comment.reply', { thread, text: body, ...who, created: created() })}
        onResolve={(thread, resolved) => void run('comment.resolve', { thread, resolved })}
        onDelete={(thread, comment) => void run('comment.delete', comment === thread ? { thread } : { thread, comment })}
        labels={strings.labels}
        testId="doc-comments-panel"
      />
    </aside>
  )
}
