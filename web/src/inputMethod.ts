/**
 * Input methods (Chinese pinyin and wubi, Japanese, Korean) in the text (plan "Doc usable by hand",
 * stage 3; walkthrough D16; WPS W-V1–W-V7). kit-editor's text input (`.kui-text-sink`), a hidden
 * text field, is where the input method composes; it hands the engine only the chosen text. Here:
 *
 * - what is being composed goes to the engine in the editor's input order (`setPreedit`): it shows
 *   in the text at the caret, underlined, the text after it moved on, over the selected text if
 *   any, without changing the document; an empty composition (Escape in the input method) takes it
 *   away, and the chosen text, which the text input types, ends it as one undo step of its own;
 * - the text input sits where the composition starts (`inspect().inputAt`), as tall as its line,
 *   so the input method's candidate window opens just under the line, lined up with the
 *   composition, at any zoom and scroll and on any page.
 *
 * The keys of a composition (Enter, Escape, the arrows, Tab, Backspace) are the input method's:
 * kit-editor keeps them from the text tool and the commands, and the editor's own key table
 * (keys.ts) and keyboard safety net (focus.ts) leave them alone.
 */
import { useCallback, useEffect, useRef, type RefObject } from 'react'
import type { KitCanvasHandle } from '@kition/sdk/editor'
import type { CaretRect } from './followCaret'

/** The longest composition sent to the engine, in characters (the engine shows no more). */
const LONGEST = 1000

/** What the input method needs of the editor's session and tools. */
export type Composer = {
  invoke: (<R>(method: string, ...args: unknown[]) => Promise<R>) | null
  inOrder: <R>(work: () => Promise<R>) => Promise<R>
  /** A settled call of the text tool: the editor's state and the pages brought up to date. */
  settle: () => Promise<unknown>
  /** Asks the text tool for its overlays again (the caret, the line under the composition). */
  refresh: () => void
}

/** kit-editor's text input, when `target` is it. */
function sinkOf(target: EventTarget | null): HTMLTextAreaElement | null {
  return target instanceof HTMLTextAreaElement && target.classList.contains('kui-text-sink') ? target : null
}

/**
 * The input method's cursor in the composition `text`, in characters (the engine's unit): where the
 * field has it once its value ends with the composition, else at its end (a composition event can
 * come before the field shows what it composes).
 */
function cursorIn(field: HTMLTextAreaElement, text: string) {
  const length = Array.from(text).length
  const start = field.value.length - text.length
  if (start < 0 || field.value.slice(start) !== text) return length
  const end = Math.min(Math.max((field.selectionEnd ?? field.value.length) - start, 0), text.length)
  return Math.min(Array.from(text.slice(0, end)).length, length)
}

/**
 * Sends the compositions of the text input to the engine and keeps the text input where a
 * composition starts (`at`, canvas points). Answers `place`, for the canvas to call when its view
 * moves.
 */
export function useInputMethod({ composer, canvas, at }: { composer: Composer; canvas: RefObject<KitCanvasHandle | null>; at: CaretRect | null | undefined }) {
  const latest = useRef(composer)
  latest.current = composer
  const where = useRef<CaretRect | null>(null)
  where.current = at ?? null

  useEffect(() => {
    // One update in flight at a time, the newest waiting: a burst of keys sends only the last.
    let queued = false
    let next: { text: string; cursor: number } | null = null
    // Compositions started, so a late cleanup never takes away a newer one.
    let started = 0
    const send = () => {
      if (queued || !next) return
      queued = true
      const { invoke, inOrder } = latest.current
      if (!invoke) {
        queued = false
        next = null
        return
      }
      void inOrder(async () => {
        queued = false
        const params = next
        next = null
        if (!params) return
        await invoke<boolean>('setPreedit', params.text, params.cursor)
        await latest.current.settle()
      })
        .then(() => latest.current.refresh())
        .catch(() => undefined)
    }
    const update = (event: CompositionEvent) => {
      const field = sinkOf(event.target)
      if (!field) return
      if (event.type === 'compositionstart') started += 1
      const text = Array.from(event.data ?? '').slice(0, LONGEST).join('')
      // A composition starts empty: nothing to show yet.
      if (event.type === 'compositionstart' && !text) return
      next = { text, cursor: cursorIn(field, text) }
      send()
    }
    const end = (event: CompositionEvent) => {
      if (!sinkOf(event.target)) return
      if (!event.data) {
        // Cancelled (Escape in the input method): nothing is typed, the composition goes now.
        next = { text: '', cursor: 0 }
        send()
        return
      }
      // Chosen: the text input types it (after this listener), which ends the composition in the
      // engine. Should it type nothing, the composition still goes, unless a new one has begun.
      next = null
      const mine = started
      window.setTimeout(() => {
        if (started !== mine) return
        next = { text: '', cursor: 0 }
        send()
      }, 0)
    }
    document.addEventListener('compositionstart', update, true)
    document.addEventListener('compositionupdate', update, true)
    document.addEventListener('compositionend', end, true)
    return () => {
      document.removeEventListener('compositionstart', update, true)
      document.removeEventListener('compositionupdate', update, true)
      document.removeEventListener('compositionend', end, true)
    }
  }, [])

  /** Puts the text input where a composition starts, as tall as its line, inside the canvas. */
  const place = useCallback((shown?: { x: number; y: number; zoom: number }) => {
    const rect = where.current
    const view = shown ?? canvas.current?.view()
    const field = document.querySelector<HTMLTextAreaElement>('textarea.kui-text-sink')
    const surface = document.querySelector<HTMLElement>('[data-testid=kit-canvas-surface]')
    if (!rect || !view || !(view.zoom > 0) || !field || !surface) return
    const box = surface.getBoundingClientRect()
    const holder = (field.offsetParent ?? document.body).getBoundingClientRect()
    const height = Math.max(1, Math.round(rect.height * view.zoom))
    // Kept inside the canvas, so the candidate window stays by the pages when the caret is out of view.
    const x = Math.min(Math.max(box.left + (rect.x - view.x) * view.zoom, box.left), box.right - 1)
    const y = Math.min(Math.max(box.top + (rect.top - view.y) * view.zoom, box.top), box.bottom - height)
    const next = { left: `${Math.round(x - holder.left)}px`, top: `${Math.round(y - holder.top)}px`, height: `${height}px` }
    if (!Object.values(next).every((value) => Number.isFinite(Number.parseFloat(value)))) return
    const style = field.style
    if (style.left === next.left && style.top === next.top && style.height === next.height) return
    Object.assign(style, next)
    // One line as tall as the text's, so the candidate window opens under the line.
    style.lineHeight = `${height}px`
    style.fontSize = `${Math.max(1, Math.round(height * 0.8))}px`
    style.whiteSpace = 'pre'
  }, [canvas])

  // After every change of where a composition starts (the caret moved, the text changed).
  useEffect(() => {
    place()
  }, [at?.page, at?.x, at?.top, at?.height, place])

  return place
}
