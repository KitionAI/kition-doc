import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import { IconButton, toast, type HostConnection, type KitCanvasView, type RasterDecoration, type RasterRect } from '@kition/sdk/editor'
import type { KitShownThread } from '@kition/sdk/protocol'
import { MessageSquareIcon as MessageSquare, MessageSquarePlusIcon as MessageSquarePlus } from '@kition/tessera/icons/react'
import { anchorOf, foundList, hostMarks, locatedList, markerPlaces, selectionLine, type HostMark, type LocatedAnchor, type PlacedLine } from './hostAnchors'

/** A marker's size and its distance from the page's right edge, in CSS pixels. */
const MARKER = 24
const INSET = 12

type Query = <R>(id: string, params?: Record<string, unknown>) => Promise<R | null>

/**
 * Comment threads the host keeps (Kit UI protocol 2.6 section 7k, `hostAnchors.ts`): the host's
 * shown threads, where each is now, the selection's anchor, and creating and opening a thread.
 * `on` is false unless the host lists `comment-anchors`; `canCreate` also needs the host to serve
 * `comment.create`, which it offers only to a person with comment rights (UI-V4), so a viewer
 * sees the markers and opens threads but makes none.
 */
export function useHostComments({ host, query, doc, revision, pages, label }: {
  host: HostConnection
  query: Query
  doc: unknown
  revision: unknown
  pages: RasterRect[]
  label: (page: number) => string
}) {
  const on = host.commentAnchors
  const canCreate = on && host.hasService('comment.create')
  const [threads, setThreads] = useState<KitShownThread[]>([])
  const [located, setLocated] = useState<{ threads: KitShownThread[]; places: LocatedAnchor[] }>({ threads: [], places: [] })
  const [selection, setSelection] = useState<PlacedLine | null>(null)
  const [active, setActive] = useState<string | null>(null)
  const latestQuery = useRef(query)
  latestQuery.current = query

  useEffect(() => (on ? host.onComments(setThreads) : undefined), [host, on])
  // `comments/locate`: whether each anchor is in the document now (registered once).
  useEffect(() => {
    if (!on) return
    host.locateComments(async (anchors) => (anchors.length ? foundList(locatedList(await latestQuery.current('comment.locate', { anchors }), anchors.length)) : []))
  }, [host, on])

  // Where each shown thread is, again after every change (typing moves it, pages break anew).
  useEffect(() => {
    if (!on || !doc) return undefined
    if (!threads.length) {
      setLocated({ threads, places: [] })
      return undefined
    }
    let stale = false
    void query<unknown>('comment.locate', { anchors: threads.map((thread) => thread.anchor) }).then((answer) => {
      if (!stale) setLocated({ threads, places: locatedList(answer, threads.length) })
    })
    return () => {
      stale = true
    }
  }, [doc, on, query, revision, threads])

  // Where the selection starts, for the Comment button beside it.
  useEffect(() => {
    if (!canCreate || !doc) return undefined
    let stale = false
    void query<unknown>('comment.anchor').then((answer) => {
      if (!stale) setSelection(selectionLine(answer))
    })
    return () => {
      stale = true
    }
  }, [canCreate, doc, query, revision])

  const marks = useMemo(() => (located.threads === threads ? hostMarks(threads, located.places, pages, active) : []), [active, located, pages, threads])

  /** Starts the host's thread on the selection (from a person's gesture, UI-V1). */
  const create = useCallback(
    async (messages: { selectFirst: string; failed: string }) => {
      const answer = await latestQuery.current<unknown>('comment.anchor')
      const anchor = anchorOf(answer, isPage(answer) ? label(answer.page) : undefined)
      if (!anchor) {
        toast('info', messages.selectFirst)
        return
      }
      try {
        const { thread } = await host.service('comment.create', { anchor })
        if (thread) setActive(thread)
      } catch (error) {
        if (reasonOf(error) !== 'cancelled') toast('error', messages.failed)
      }
    },
    [host, label],
  )

  /** Opens a thread in the host (`comments/activated`). */
  const open = useCallback(
    (id: string) => {
      setActive(id)
      host.activateComment(id)
    },
    [host],
  )

  return { on, canCreate, threads, marks, selection, active, create, open }
}

const isPage = (value: unknown): value is { page: number } => typeof value === 'object' && value !== null && typeof (value as { page?: unknown }).page === 'number'

function reasonOf(error: unknown) {
  const data = (error as { data?: { reason?: unknown } } | null)?.data
  return typeof data?.reason === 'string' ? data.reason : null
}

/** Tints each open thread's text in the accent color, more for the active one. */
export function hostHighlightDecoration(marks: readonly HostMark[]): RasterDecoration | null {
  if (!marks.some((mark) => mark.rects.length)) return null
  return (context, toScreen) => {
    const accent = getComputedStyle(document.documentElement).getPropertyValue('--kit-primary').trim() || '#2b5cd9'
    context.save()
    context.fillStyle = accent
    for (const mark of marks) {
      context.globalAlpha = mark.active ? 0.3 : 0.16
      for (const rect of mark.rects) {
        const a = toScreen(rect.x, rect.y)
        const b = toScreen(rect.x + rect.width, rect.y + rect.height)
        context.fillRect(a.x, a.y, b.x - a.x, b.y - a.y)
        context.fillRect(a.x, b.y - 1.5, b.x - a.x, 1.5)
      }
    }
    context.restore()
  }
}

/**
 * The markers over the canvas, in the right margin beside each thread's line, and the Comment
 * button beside the selection: kit-editor's icon buttons, positioned from the canvas view so they
 * follow scrolling and zooming.
 */
export function HostCommentLayer({ view, marks, pages, selection, canCreate, onCreate, onOpen, strings }: {
  view: KitCanvasView
  marks: HostMark[]
  pages: RasterRect[]
  selection: PlacedLine | null
  canCreate: boolean
  onCreate: () => void
  onOpen: (id: string) => void
  strings: { comment: string; open: (count: number) => string }
}) {
  const { zoom } = view
  const size = MARKER / zoom
  const places = markerPlaces(marks, size, INSET / zoom)
  const screen = (x: number, y: number) => ({ left: (x - view.x) * zoom, top: (y - view.y) * zoom })
  const selectionPage = selection ? pages[selection.page] : undefined
  const add = selection && selectionPage ? screen(selectionPage.x1 - (INSET + MARKER) / zoom, selectionPage.y0 + selection.y + selection.height / 2 - size / 2) : null
  // The Comment button takes the margin beside the selection; a marker on that line moves aside.
  const shown = (top: number) => top > -MARKER && top < view.height + MARKER
  return (
    <div className="doc-host-comments" data-testid="doc-host-comments">
      {marks.map((mark, index) => {
        const place = places[index]
        if (!place) return null
        const at = screen(place.x, place.y)
        if (!shown(at.top)) return null
        const left = add && Math.abs(add.top - at.top) < MARKER ? at.left - MARKER - 4 : at.left
        return (
          <span key={mark.id} className="doc-host-comment-marker" data-active={mark.active || undefined} style={{ left, top: at.top }}>
            <IconButton icon={MessageSquare} label={strings.open(mark.count)} onClick={() => onOpen(mark.id)} data-testid="doc-host-comment-marker" data-thread={mark.id} pressed={mark.active} />
          </span>
        )
      })}
      {canCreate && add && shown(add.top) ? (
        <span className="doc-host-comment-add" style={{ left: add.left, top: add.top }}>
          <IconButton icon={MessageSquarePlus} label={strings.comment} onPointerDown={(event) => event.preventDefault()} onClick={onCreate} data-testid="doc-host-comment-add" />
        </span>
      ) : null}
    </div>
  )
}
