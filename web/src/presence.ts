import { drawPresence, peerColor, type RasterDecoration, type RemoteCaret } from '@kition/kit-editor'
import type { PeerMark } from './presenceMarks'

export { peerMarks, placesOf, sharedPresence, type PlacedCaret, type SharedCaret } from './presenceMarks'

/** Draws each peer's selection as a tint in their color, and their caret with their name. */
export function presenceDecoration(marks: readonly PeerMark[]): RasterDecoration | null {
  if (!marks.length) return null
  return (context, toScreen) => {
    context.save()
    context.globalAlpha = 0.22
    for (const mark of marks) {
      context.fillStyle = peerColor(mark.color)
      for (const rect of mark.rects) {
        const a = toScreen(rect.x, rect.y)
        const b = toScreen(rect.x + rect.width, rect.y + rect.height)
        context.fillRect(a.x, a.y, b.x - a.x, b.y - a.y)
      }
    }
    context.restore()
    const carets: RemoteCaret[] = marks.flatMap((mark) => (mark.caret ? [{ principal: mark.principal, name: mark.name, color: mark.color, caret: mark.caret.at, rect: mark.caret }] : []))
    if (carets.length) drawPresence(context, { carets }, toScreen)
  }
}

/** Both decorations, in order (either may be absent). */
export function bothDecorations(first: RasterDecoration | null, second: RasterDecoration | null): RasterDecoration | null {
  if (!first || !second) return first ?? second
  return (context, toScreen, view) => {
    first(context, toScreen, view)
    second(context, toScreen, view)
  }
}
