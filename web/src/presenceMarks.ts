import type { KitPeerPresence, KitPresence, KitPresenceCaret, PresenceRect, RasterRect } from '@kition/kit-editor'

/**
 * Presence (Kit UI protocol section 7e) in a word processor: the person's caret is a replica text
 * position (`{ object, field: "text", offset }`, the paragraph whose text holds it), the same on
 * every replica, which the engine's `presence.caret` answers; a selection's other end travels in
 * the viewport (`anchor`), and the paragraphs it covers as the selection's object ids. Each other
 * person's caret and selection are placed in this view by the engine (`presence.locate`) and drawn
 * over the pages in their color, with their name.
 */

/** `presence.caret`: where this person is, as replica positions. */
export type SharedCaret = { caret?: KitPresenceCaret | null; anchor?: KitPresenceCaret | null; selection?: string[]; page?: number }

/** One person's caret and selection on the pages, in page points (`presence.locate`). */
export type PlacedCaret = {
  caret: { page: number; x: number; y: number; height: number } | null
  rects: Array<{ page: number; x: number; y: number; width: number; height: number }>
}

/** What a peer shows here: their caret and their selection's rectangles, on the canvas. */
export type PeerMark = { principal: string; name: string; color: string; caret: (PresenceRect & { at: KitPresenceCaret }) | null; rects: PresenceRect[] }

const isRecord = (value: unknown): value is Record<string, unknown> => typeof value === 'object' && value !== null && !Array.isArray(value)

/** A well-formed text position, or null. */
export function textPosition(value: unknown): KitPresenceCaret | null {
  if (!isRecord(value)) return null
  const { object, field, offset } = value
  return typeof object === 'string' && typeof field === 'string' && typeof offset === 'number' && Number.isInteger(offset) && offset >= 0 ? { object, field, offset } : null
}

/** What this session shares, from the engine's answer (null before it answered). */
export function sharedPresence(shared: SharedCaret | null): KitPresence | null {
  if (!shared) return null
  const caret = textPosition(shared.caret)
  const anchor = textPosition(shared.anchor)
  const selection = Array.isArray(shared.selection) ? shared.selection.filter((id): id is string => typeof id === 'string') : []
  return { selection, ...(caret ? { caret } : {}), viewport: { page: typeof shared.page === 'number' ? shared.page : 0, ...(anchor ? { anchor } : {}) } }
}

/** What the engine places for each peer (`presence.locate`'s `places`), in the peers' order. */
export function placesOf(peers: readonly KitPeerPresence[]) {
  return peers.map((peer) => {
    const anchor = textPosition(peer.viewport?.anchor)
    return { caret: peer.caret ?? null, ...(anchor ? { anchor } : {}) }
  })
}

/** The peers' marks on the canvas: `placed` is the engine's answer for `placesOf(peers)`, `pages` where each page is. */
export function peerMarks(peers: readonly KitPeerPresence[], placed: readonly PlacedCaret[], pages: readonly RasterRect[]): PeerMark[] {
  const marks: PeerMark[] = []
  peers.forEach((peer, index) => {
    const where = placed[index]
    if (!where) return
    const on = (page: number) => pages[page] ?? null
    const caretPage = where.caret ? on(where.caret.page) : null
    const caret = where.caret && caretPage && peer.caret ? { x: caretPage.x0 + where.caret.x, y: caretPage.y0 + where.caret.y, width: 2, height: where.caret.height, at: peer.caret } : null
    const rects = where.rects.flatMap((rect) => {
      const page = on(rect.page)
      return page ? [{ x: page.x0 + rect.x, y: page.y0 + rect.y, width: rect.width, height: rect.height }] : []
    })
    if (caret || rects.length) marks.push({ principal: peer.principal, name: peer.name, color: peer.color, caret, rects })
  })
  return marks
}
