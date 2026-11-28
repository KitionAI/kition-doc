/**
 * The page view's zoom steps (plan "Doc usable by hand", stage 0A, section 2 rows 34, 35 and "not
 * copied" 7; walkthrough A5, A6, A8; WPS W-U1, W-U2, W-U4). A mouse wheel notch with Ctrl or Cmd
 * adds or takes 10 points about the pointer; a trackpad pinch stays continuous. The status bar's −
 * and + go 10 points at a time to a multiple of 10 (113% → 120% → 130%). Every way stays within
 * 10%–500%. The zoom is the view's (a person's setting, never a change of the document). The
 * percentage is CSS pixels per point: the page opens at 133% (`pageView.ts`).
 */
import type { KitCanvasHandle, RasterRect } from '@kition/sdk/editor'

export const MIN_ZOOM = 0.1
export const MAX_ZOOM = 5
/** The points one mouse wheel notch adds or takes. */
const NOTCH_POINTS = 10
/** A pinch's zoom per pixel of wheel delta (the shared canvas's continuous zoom). */
const PINCH_SPEED = 0.01
const EPSILON = 1e-6
/** The grey kept round a page fitted in view (One Page). */
const PAGE_MARGIN = 24

export const clampZoom = (zoom: number) => Math.min(MAX_ZOOM, Math.max(MIN_ZOOM, Number.isFinite(zoom) ? zoom : 1))

/** The status bar's − and +: the next multiple of 10% past `zoom`, up (`1`) or down (`-1`). */
export function stepZoom(zoom: number, direction: 1 | -1) {
  const tens = (zoom * 100) / 10
  const next = direction > 0 ? Math.floor(tens + EPSILON) + 1 : Math.ceil(tens - EPSILON) - 1
  return clampZoom((next * 10) / 100)
}

/** One mouse wheel notch from `zoom`: 10 points up (`1`) or down (`-1`). */
export function notchZoom(zoom: number, direction: 1 | -1) {
  const percent = Math.round(zoom * 100) + direction * NOTCH_POINTS
  return clampZoom(percent / 100)
}

/** A pinch (or a wheel too fine to be notches) zooms continuously, by its delta. */
export const pinchZoom = (zoom: number, deltaY: number) => clampZoom(zoom * Math.exp(-deltaY * PINCH_SPEED))

/**
 * How many mouse wheel notches a wheel event is, or 0 for a trackpad's scroll or pinch: a wheel
 * counted in lines or pages, or in whole multiples of about 100 pixels (a browser's notch).
 */
export function wheelNotches(event: Pick<WheelEvent, 'deltaMode' | 'deltaY'>) {
  const delta = Math.abs(event.deltaY)
  if (event.deltaMode === 1) return delta > 0 ? Math.max(1, Math.round(delta / 3)) : 0
  if (event.deltaMode === 2) return delta > 0 ? 1 : 0
  if (delta < 50 || !Number.isInteger(event.deltaY)) return 0
  const notches = Math.round(delta / 100)
  return notches >= 1 && Math.abs(delta - notches * 100) <= 10 * notches ? notches : 0
}

/** Zooms to `zoom`, keeping the document point under (`clientX`, `clientY`) on `surface` still. */
export function zoomAround(canvas: KitCanvasHandle, zoom: number, surface: Element, clientX: number, clientY: number) {
  const next = clampZoom(zoom)
  const view = canvas.view()
  if (!view) {
    canvas.zoomTo(next)
    return
  }
  const box = surface.getBoundingClientRect()
  const px = clientX - box.left
  const py = clientY - box.top
  const x = view.x + px / view.zoom - px / next
  const y = view.y + py / view.zoom - py / next
  canvas.zoomTo(next)
  canvas.panTo(x, y)
}

/** Zooms to `zoom` keeping the middle of the view where it is (the status bar, View › 100%). */
export function zoomKeepingMiddle(canvas: KitCanvasHandle, zoom: number) {
  const next = clampZoom(zoom)
  const view = canvas.view()
  if (!view) {
    canvas.zoomTo(next)
    return
  }
  const cx = view.x + view.width / (2 * view.zoom)
  const cy = view.y + view.height / (2 * view.zoom)
  canvas.zoomTo(next)
  canvas.panTo(cx - view.width / (2 * next), cy - view.height / (2 * next))
}

/** The whole page in view, with a little grey round it (One Page, the status bar's Fit). */
export const wholePage = (page: RasterRect): RasterRect => ({ x0: page.x0 - PAGE_MARGIN, y0: page.y0 - PAGE_MARGIN, x1: page.x1 + PAGE_MARGIN, y1: page.y1 + PAGE_MARGIN })

/** The top of a page at full width (Page Width). */
export function pageWidthArea(page: RasterRect): RasterRect {
  const width = page.x1 - page.x0
  return { x0: page.x0 - PAGE_MARGIN, y0: page.y0 - PAGE_MARGIN, x1: page.x1 + PAGE_MARGIN, y1: page.y0 + width * 0.55 }
}
