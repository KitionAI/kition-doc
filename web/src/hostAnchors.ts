import type { KitCommentAnchor, KitShownThread } from '@kition/kit-ui-protocol'
import type { RasterRect } from '@kition/kit-editor'

/**
 * Comment threads a host keeps itself (Kit UI protocol 2.6 section 7k, capability
 * `comment-anchors`; managed co-editing card D4). The kit lists `comment-anchors`; when the host
 * does too, a new comment on the selection is the host's (`host/service` `comment.create` with the
 * selection's anchor, from the engine's `comment.anchor`), not a Word comment written into the
 * file, and the kit marks each open thread the host shows (`comments/show`) where the engine finds
 * its anchor now (`comment.locate`: through edits and pagination), answers `comments/locate`, and
 * sends `comments/activated` when the person clicks a thread's marker. The file's own Word comments
 * stay the kit's (`Comments.tsx`): listed in its Comments pane, marked, replied to and resolved as
 * before.
 */

/** `comment.anchor`: the selection as an anchor, and where it starts in this view. */
export type SelectionAnchor = KitCommentAnchor & { at?: PlacedLine | null }

/** A place on a page (page index, page points): the top of a line and its height. */
export type PlacedLine = { page: number; x: number; y: number; height: number }

/** `comment.locate`: where one anchor is now. `page` counts from 1; rects' `page` is an index. */
export type LocatedAnchor = {
  found: boolean
  exact: boolean
  text: string
  page: number | null
  at: PlacedLine | null
  rects: Array<{ page: number; x: number; y: number; width: number; height: number }>
}

/** A shown thread on the canvas: its highlight, and the line its marker sits beside. */
export type HostMark = {
  id: string
  count: number
  active: boolean
  rects: Array<{ x: number; y: number; width: number; height: number }>
  line: { x: number; y: number; height: number }
  /** The right edge of its page, where the marker goes. */
  right: number
}

/** The protocol's limits (UI-V3): ids, and characters of a quote or a label. */
const IDS_LIMIT = 256
const TEXT_LIMIT = 500

const isRecord = (value: unknown): value is Record<string, unknown> => typeof value === 'object' && value !== null && !Array.isArray(value)
const isCount = (value: unknown): value is number => typeof value === 'number' && Number.isSafeInteger(value) && value >= 0

function placedLine(value: unknown): PlacedLine | null {
  if (!isRecord(value)) return null
  const { page, x, y, height } = value
  return isCount(page) && typeof x === 'number' && typeof y === 'number' && typeof height === 'number' ? { page, x, y, height } : null
}

/**
 * The anchor `comment.create` sends for what `comment.anchor` answered: its ids, range, quote and
 * page within the protocol's limits, with `label` (what the host's list shows); null for a caret
 * or an answer that is no anchor.
 */
export function anchorOf(answer: unknown, label?: string): KitCommentAnchor | null {
  if (!isRecord(answer)) return null
  const ids = Array.isArray(answer.ids) ? answer.ids.filter((id): id is string => typeof id === 'string' && id.length > 0 && id.length <= 256).slice(0, IDS_LIMIT) : []
  if (!ids.length) return null
  const anchor: KitCommentAnchor = { ids }
  const range = answer.range
  if (isRecord(range) && isCount(range.start) && isCount(range.end) && range.end >= range.start) anchor.range = { start: range.start, end: range.end }
  if (typeof answer.quote === 'string' && answer.quote) anchor.quote = answer.quote.slice(0, TEXT_LIMIT)
  if (isCount(answer.page)) anchor.page = answer.page
  if (label) anchor.label = label.slice(0, TEXT_LIMIT)
  return anchor
}

/** Where `comment.anchor` says the selection starts, or null. */
export function selectionLine(answer: unknown): PlacedLine | null {
  return isRecord(answer) ? placedLine(answer.at) : null
}

/** `comment.locate`'s answer for `count` anchors, one well-formed entry each (not found when malformed). */
export function locatedList(answer: unknown, count: number): LocatedAnchor[] {
  const list = Array.isArray(answer) ? answer : []
  return Array.from({ length: count }, (_, index) => {
    const value = list[index]
    if (!isRecord(value) || value.found !== true) return { found: false, exact: false, text: '', page: null, at: null, rects: [] }
    const rects = Array.isArray(value.rects)
      ? value.rects.flatMap((rect) => (isRecord(rect) && isCount(rect.page) && [rect.x, rect.y, rect.width, rect.height].every((n) => typeof n === 'number') ? [{ page: rect.page, x: rect.x as number, y: rect.y as number, width: rect.width as number, height: rect.height as number }] : []))
      : []
    return { found: true, exact: value.exact === true, text: typeof value.text === 'string' ? value.text : '', page: isCount(value.page) ? value.page : null, at: placedLine(value.at), rects }
  })
}

/**
 * The marks to draw: each open thread the engine found, on the canvas (page rects to canvas
 * coordinates). Resolved threads and threads whose anchor is gone have no mark; the host still
 * lists them.
 */
export function hostMarks(threads: readonly KitShownThread[], located: readonly LocatedAnchor[], pages: readonly RasterRect[], active: string | null): HostMark[] {
  const marks: HostMark[] = []
  threads.forEach((thread, index) => {
    const place = located[index]
    if (thread.resolved || !place?.found) return
    const first = place.rects[0]
    const line = place.at ?? (first ? { page: first.page, x: first.x, y: first.y, height: first.height } : null)
    const page = line ? pages[line.page] : undefined
    if (!line || !page) return
    const rects = place.rects.flatMap((rect) => {
      const on = pages[rect.page]
      return on ? [{ x: on.x0 + rect.x, y: on.y0 + rect.y, width: rect.width, height: rect.height }] : []
    })
    marks.push({ id: thread.id, count: thread.count, active: thread.id === active, rects, line: { x: page.x0 + line.x, y: page.y0 + line.y, height: line.height }, right: page.x1 })
  })
  return marks
}

/**
 * Where each mark's marker goes, in canvas units: in its page's right margin beside its line,
 * moved down below the one before when two would overlap (`size` canvas units tall at this zoom).
 */
export function markerPlaces(marks: readonly HostMark[], size: number, inset: number): Array<{ id: string; x: number; y: number }> {
  const order = marks.map((mark, index) => ({ mark, index })).sort((a, b) => a.mark.right - b.mark.right || a.mark.line.y - b.mark.line.y || a.index - b.index)
  const places: Array<{ id: string; x: number; y: number }> = new Array(marks.length)
  let previous: { right: number; bottom: number } | null = null
  for (const { mark, index } of order) {
    const centered = mark.line.y + mark.line.height / 2 - size / 2
    const y: number = previous && previous.right === mark.right ? Math.max(centered, previous.bottom + size * 0.15) : centered
    places[index] = { id: mark.id, x: mark.right - inset - size, y }
    previous = { right: mark.right, bottom: y + size }
  }
  return places
}

/** Whether each anchor is found now, for `comments/locate`. */
export function foundList(located: readonly LocatedAnchor[]): boolean[] {
  return located.map((place) => place.found)
}
