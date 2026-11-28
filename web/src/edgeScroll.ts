/**
 * Scrolling at the window's edge during a drag (plan "Doc usable by hand", stage 1, section 2 row
 * 4; walkthrough B8; WPS W-B5). While a press on the pages is held and the pointer is past an edge
 * of the canvas (below or right of it, or above or left of it while the view can go back that
 * way), a timer pans the view every frame, the pointer still or not, and sends the drag again from
 * the pointer's last place held at the edge, so the selection (characters, words, lines from the
 * selection bar, the place carried text lands) reaches the text that comes into view. The further
 * past the edge, the faster, at WPS's measured pace: about 100 pixels a second 2 pixels out, 175
 * at 5, 350 at 9, 700 at 14, 1000 at 19 and 2000 from 32. Inside the canvas nothing scrolls, nor
 * while a pen, the eraser or a shape to draw is held (the engine's cursor is the crosshair then).
 *
 * The engine's text tool decides what the drag is; this only wraps kit-editor's canvas tool, so
 * every sample still goes through the tool bridge's one ordered queue (the latest drag kept). The
 * view follows the caret only once the press is let go (followCaret.ts): during the drag the
 * view is this timer's.
 */
import type { KitPointer, KitTool } from '@kition/sdk/editor'

type Context = Parameters<KitTool['begin']>[1]
type Gesture = NonNullable<ReturnType<KitTool['begin']>>

/** What the wrapper reads at each frame: the engine tool's cursor now and the canvas's size (CSS pixels). */
export type EdgeScrollRead = () => { cursor: string; size: { width: number; height: number } | null }

/** WPS's pace: screen pixels a second at so many pixels past the edge (W-B5). */
const PACE: Array<[number, number]> = [
  [0, 60],
  [2, 100],
  [5, 175],
  [9, 350],
  [14, 700],
  [19, 1000],
  [32, 2000],
]

/** Screen pixels a second at `over` CSS pixels past the edge: WPS's pace, straight between its points. */
export function edgeSpeed(over: number): number {
  if (!(over > 0)) return 0
  for (let index = 1; index < PACE.length; index += 1) {
    const [x1, y1] = PACE[index]
    if (over <= x1) {
      const [x0, y0] = PACE[index - 1]
      return y0 + ((over - x0) / (x1 - x0)) * (y1 - y0)
    }
  }
  return PACE[PACE.length - 1][1]
}

/** A frame that came late scrolls for at most this long (seconds), so a stall never jumps far. */
const MOST_FRAME = 0.1

export function edgeScrolling(inner: KitTool, read: EdgeScrollRead): KitTool {
  return {
    ...inner,
    begin(pointer: KitPointer, context: Context) {
      const gesture = inner.begin(pointer, context) as Gesture | void
      if (!gesture) return gesture
      let last = pointer
      let held: Context = context
      let frame = 0
      let then = 0
      /** The pointer's distance past each edge of the canvas (CSS pixels), and where it is held. */
      const edges = (at: KitPointer) => {
        const { size, cursor } = read()
        if (!size || cursor === 'crosshair') return null
        const origin = { x: at.clientX - at.canvasX, y: at.clientY - at.canvasY }
        const box = { left: origin.x, top: origin.y, right: origin.x + size.width, bottom: origin.y + size.height }
        const { x, y } = held.viewport.view
        const over = {
          right: at.clientX - (box.right - 1),
          down: at.clientY - (box.bottom - 1),
          left: x > 0 ? box.left + 1 - at.clientX : 0,
          up: y > 0 ? box.top + 1 - at.clientY : 0,
        }
        const clientX = over.right > 0 ? box.right - 1 : over.left > 0 ? box.left + 1 : at.clientX
        const clientY = over.down > 0 ? box.bottom - 1 : over.up > 0 ? box.top + 1 : at.clientY
        return { over, clientX, clientY }
      }
      const outside = (edge: ReturnType<typeof edges>) => edge != null && (edge.over.right > 0 || edge.over.down > 0 || edge.over.left > 0 || edge.over.up > 0)
      /** The sample with the pointer held at the edge, in the document's pixels as the view is now. */
      const place = (at: KitPointer): KitPointer => {
        const edge = edges(at)
        if (!edge || (edge.clientX === at.clientX && edge.clientY === at.clientY)) return at
        const point = held.viewport.toImage(edge.clientX, edge.clientY)
        return { ...at, x: point.x, y: point.y }
      }
      const stop = () => {
        if (frame) window.cancelAnimationFrame(frame)
        frame = 0
      }
      const tick = (now: number) => {
        frame = 0
        const edge = edges(last)
        if (!edge || !outside(edge)) return
        const seconds = then ? Math.min(MOST_FRAME, Math.max(0, (now - then) / 1000)) : 1 / 60
        then = now
        const view = held.viewport.view
        const zoom = view.zoom > 0 ? view.zoom : 1
        const dx = ((edgeSpeed(edge.over.right) - edgeSpeed(edge.over.left)) * seconds) / zoom
        const dy = ((edgeSpeed(edge.over.down) - edgeSpeed(edge.over.up)) * seconds) / zoom
        const x = Math.max(0, view.x + dx)
        const y = Math.max(0, view.y + dy)
        if (x !== view.x || y !== view.y) {
          held.viewport.panTo(x, y)
          gesture.move?.(place(last), held)
        }
        frame = window.requestAnimationFrame(tick)
      }
      const schedule = () => {
        if (frame || !outside(edges(last))) return
        then = 0
        frame = window.requestAnimationFrame(tick)
      }
      return {
        move(at: KitPointer, now: Context) {
          last = at
          held = now
          gesture.move?.(place(at), now)
          schedule()
        },
        end(at: KitPointer, now: Context) {
          stop()
          gesture.end?.(place(at), now)
        },
        cancel(now: Context) {
          stop()
          gesture.cancel?.(now)
        },
      }
    },
  }
}
