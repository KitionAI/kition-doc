/**
 * The view follows the caret (plan "Doc usable by hand", stage 0B, section 2 row 10; walkthrough
 * A10, D5, A11, C7, Q2; WPS W-D6, W-U6). After the person's own key, press, or command (a ribbon
 * button, a dialog, Undo, Find Next, Go To), a caret whose line is out of view brings the view just
 * far enough that the line shows, a line's height to spare above and below and 40 pixels across;
 * never centred, never at another zoom. Scrolling alone leaves the caret where it is, and the next
 * key brings the view back to it. A caret moved by an agent's or the host's edit (text inserted
 * before it) does not move the person's view, nor does Select All (C10), nor a drag still under way
 * (the edge scroll of stage 1 is the drag's).
 */
import { useCallback, useEffect, useRef, type RefObject } from 'react'
import type { KitCanvasHandle } from '@kition/sdk/editor'

/** Where the caret is on the canvas, in points (`inspect().caret`). */
export type CaretRect = { page: number; x: number; top: number; height: number }

/** The room kept round the caret across, in CSS pixels. */
const ACROSS = 40

/**
 * Pans `canvas` the least that shows `caret`'s line with a line above and below it (and 40 pixels
 * either side), at the same zoom. Answers whether it moved.
 */
export function showCaret(canvas: KitCanvasHandle, caret: CaretRect) {
  const view = canvas.view()
  if (!view || !(view.zoom > 0) || view.height <= 0 || view.width <= 0) return false
  const high = view.height / view.zoom
  const wide = view.width / view.zoom
  const room = Math.min(caret.height, Math.max(0, (high - caret.height) / 2))
  const across = Math.min(ACROSS / view.zoom, Math.max(0, wide / 2))
  let { x, y } = view
  if (caret.top - room < y) y = caret.top - room
  else if (caret.top + caret.height + room > y + high) y = caret.top + caret.height + room - high
  if (caret.x - across < x) x = caret.x - across
  else if (caret.x + across > x + wide) x = caret.x + across - wide
  if (Math.abs(x - view.x) < 0.5 && Math.abs(y - view.y) < 0.5) return false
  canvas.panTo(x, y)
  return true
}

/**
 * Keeps the caret in view after the person's own moves (see the file's comment). `editedBy` is the
 * session's: an agent's or the host's edit comes with one of its own. `held` says the view stays
 * put for now (after Select All). Answers `acted`, for the editor's own runs (the ribbon, the keys
 * table, dialogs), which come with no key or press on the text.
 */
export function useFollowCaret({ canvas, caret, editedBy, held }: { canvas: RefObject<KitCanvasHandle | null>; caret: CaretRect | null | undefined; editedBy: unknown; held: () => boolean }) {
  // Whether the person did something since they last moved the view themselves.
  const acted = useRef(false)
  // A press on the pages not let go yet (a click, a drag).
  const pressed = useRef(false)
  const latest = useRef<CaretRect | null>(null)
  latest.current = caret ?? null
  const follow = useCallback(() => {
    const handle = canvas.current
    const at = latest.current
    if (!handle || !at || !acted.current || pressed.current || held()) return
    showCaret(handle, at)
  }, [canvas, held])
  useEffect(() => {
    let pressedAt: CaretRect | null = null
    const onSurface = (target: EventTarget | null) => target instanceof Element && target.classList.contains('kui-canvas-surface')
    const key = () => {
      acted.current = true
    }
    const down = (event: PointerEvent) => {
      // A press on the pages is the person's (the caret goes where they press); any other press
      // (a scroll bar, the zoom control) leaves the caret to them, and a button's command says so
      // itself (`acted`).
      acted.current = onSurface(event.target)
      pressed.current = acted.current
      pressedAt = latest.current
    }
    // Let go: the caret the gesture made, once the engine has answered it (until then the caret
    // is the one from before the press, and the next answer brings the view).
    const up = () => {
      if (!pressed.current) return
      pressed.current = false
      if (latest.current !== pressedAt) follow()
    }
    // The person scrolled (or zoomed): the caret stays where it is, out of view if so.
    const wheel = () => {
      acted.current = false
    }
    document.addEventListener('keydown', key, true)
    document.addEventListener('pointerdown', down, true)
    document.addEventListener('pointerup', up, true)
    document.addEventListener('pointercancel', up, true)
    document.addEventListener('wheel', wheel, { capture: true, passive: true })
    return () => {
      document.removeEventListener('keydown', key, true)
      document.removeEventListener('pointerdown', down, true)
      document.removeEventListener('pointerup', up, true)
      document.removeEventListener('pointercancel', up, true)
      document.removeEventListener('wheel', wheel, true)
    }
  }, [follow])
  // An agent's or the host's edit: what the person did before it is not what moved the caret.
  const seenEdit = useRef(editedBy)
  useEffect(() => {
    if (editedBy != null && editedBy !== seenEdit.current) acted.current = false
    seenEdit.current = editedBy
    follow()
  }, [caret, editedBy, follow])
  return useCallback(() => {
    acted.current = true
  }, [])
}
