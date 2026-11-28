/**
 * The wheel over the pages with Ctrl or Cmd (plan "Doc usable by hand", stage 0A, section 2 row 34
 * and "not copied" 7; walkthrough A8; WPS W-U1): a mouse notch zooms 10 points about the pointer, a
 * trackpad pinch continuously, within 10%–500% (`zoom.ts`). Taken in the capture phase, before the
 * shared canvas's own wheel handler, which zoomed 2.72 times a notch. The wheel without them
 * scrolls as the canvas does.
 */
import { useEffect, type RefObject } from 'react'
import type { KitCanvasHandle } from '@kition/sdk/editor'
import { notchZoom, pinchZoom, wheelNotches, zoomAround } from './zoom'

export function useDocWheel(region: RefObject<HTMLElement | null>, canvas: RefObject<KitCanvasHandle | null>) {
  useEffect(() => {
    const element = region.current
    if (!element) return undefined
    const wheel = (event: WheelEvent) => {
      if (!event.ctrlKey && !event.metaKey) return
      const target = event.target instanceof Element ? event.target : null
      const surface = target?.closest('.kui-canvas')?.querySelector('canvas') ?? element.querySelector('.kui-canvas-surface')
      const handle = canvas.current
      const view = handle?.view()
      if (!surface || !handle || !view) return
      event.preventDefault()
      event.stopPropagation()
      const notches = wheelNotches(event)
      let zoom = view.zoom
      if (notches) for (let index = 0; index < Math.min(notches, 5); index += 1) zoom = notchZoom(zoom, event.deltaY < 0 ? 1 : -1)
      else zoom = pinchZoom(view.zoom, event.deltaY)
      zoomAround(handle, zoom, surface, event.clientX, event.clientY)
    }
    element.addEventListener('wheel', wheel, { capture: true, passive: false })
    return () => element.removeEventListener('wheel', wheel, { capture: true })
  }, [region, canvas])
}
