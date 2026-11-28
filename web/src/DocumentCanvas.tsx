import { useId, useLayoutEffect, useRef, useState, type ComponentProps, type ReactNode, type RefObject } from 'react'
import { KitDocumentCanvas, useKitEditor, type KitCanvasView, type RasterRect, type RasterView } from '@kition/sdk/editor'
import { Scrollbar } from '@kition/tessera/ui'
import { initialPageView } from './pageView'
import { useDocWheel } from './useDocWheel'
import { onMac } from './keys'

/** Whether `target` is the canvas's drawing surface (the pages and the pasteboard around them). */
const onSurface = (target: EventTarget | null): target is HTMLCanvasElement => target instanceof HTMLCanvasElement && target.classList.contains('kui-canvas-surface')

/**
 * Place the first page at reading scale before mounting the canvas; retain later user views.
 * `overlay` draws controls over the pages (comment markers), placed from the canvas's current view.
 * Ctrl or Cmd with the wheel zooms 10 points a notch (`useDocWheel.ts`).
 * `onMenu`: a right-click on the pages (Ctrl+click on a Mac), at its client point; the canvas
 * gives its tool no right button, so the editor asks the engine itself (`ContextMenu.tsx`).
 */
export function DocumentCanvas({ page, viewMemory, canvas, panBounds, onView, overlay, onMenu, ...props }: Omit<ComponentProps<typeof KitDocumentCanvas>, 'viewMemory'> & {
  page: RasterRect | undefined
  viewMemory: RefObject<RasterView | null>
  overlay?: (view: KitCanvasView) => ReactNode
  onMenu?: (client: { x: number; y: number }) => void
}) {
  const region = useRef<HTMLDivElement>(null)
  const [ready, setReady] = useState(false)
  const [view, setView] = useState<KitCanvasView | null>(null)
  const { session, t } = useKitEditor()
  const viewportId = useId()
  useDocWheel(region, canvas)
  const horizontalPage = view ? view.width / view.zoom : 0
  const verticalPage = view ? view.height / view.zoom : 0
  const horizontalMax = panBounds ? Math.max(0, panBounds.x1 - panBounds.x0 - horizontalPage) : 0
  const verticalMax = panBounds ? Math.max(0, panBounds.y1 - panBounds.y0 - verticalPage) : 0
  useLayoutEffect(() => {
    if (ready) return undefined
    const element = region.current
    if (!element || !page) return undefined
    const place = () => {
      const { width, height } = element.getBoundingClientRect()
      if (width <= 0 || height <= 0) return
      if (!viewMemory.current) viewMemory.current = initialPageView(page, width)
      setReady(true)
    }
    place()
    const observer = new ResizeObserver(place)
    observer.observe(element)
    return () => observer.disconnect()
  }, [page, ready, viewMemory])

  return (
    <div className="doc-page-workspace">
      <div
        className="doc-page-viewport"
        id={viewportId}
        ref={region}
        // Ctrl+click on a Mac is a right-click: its context menu follows, and the text tool gets no press.
        onPointerDownCapture={(event) => {
          if (onMenu && onMac() && event.button === 0 && event.ctrlKey && !event.metaKey && onSurface(event.target)) {
            event.stopPropagation()
            event.preventDefault()
          }
        }}
        // A right press keeps the keyboard in the text (the menu gives it back there).
        onMouseDownCapture={(event) => {
          if (onMenu && (event.button === 2 || (onMac() && event.ctrlKey)) && onSurface(event.target)) event.preventDefault()
        }}
        onContextMenu={(event) => {
          if (!onMenu || !onSurface(event.target)) return
          event.preventDefault()
          onMenu({ x: event.clientX, y: event.clientY })
        }}
      >
        {ready || session.failure ? <KitDocumentCanvas {...props} canvas={canvas} panBounds={panBounds} viewMemory={viewMemory} onView={(next, stats) => {
          setView((previous) => previous && previous.x === next.x && previous.y === next.y && previous.zoom === next.zoom && previous.width === next.width && previous.height === next.height ? previous : next)
          onView?.(next, stats)
        }} /> : null}
        {view && overlay && !session.failure ? overlay(view) : null}
      </div>
      {view && panBounds && !session.failure ? <>
        <Scrollbar label={t('verticalScroll')} controls={viewportId} value={view.y - panBounds.y0} max={verticalMax} page={verticalPage} onChange={(value) => canvas.current?.panTo(view.x, panBounds.y0 + value)} />
        {horizontalMax > 0.5 ? <Scrollbar label={t('horizontalScroll')} controls={viewportId} orientation="horizontal" value={view.x - panBounds.x0} max={horizontalMax} page={horizontalPage} onChange={(value) => canvas.current?.panTo(panBounds.x0 + value, view.y)} /> : null}
      </> : null}
    </div>
  )
}
