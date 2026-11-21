import type { RasterRect, RasterView } from '@kition/sdk/editor'

const PAGE_GAP = 20

/** Document coordinates are points: 96 CSS pixels per inch is 4/3 pixels per point. */
export function initialPageView(page: RasterRect, width: number): RasterView {
  const zoom = Math.min(96 / 72, Math.max(0.01, (width - 64) / (page.x1 - page.x0 + 48)))
  return { zoom, x: (page.x0 + page.x1) / 2 - width / (2 * zoom), y: page.y0 - PAGE_GAP / zoom }
}

/** Short documents stay at the top instead of being centred in a tall viewport. */
export function pagePanBounds(pages: RasterRect[], height: number, zoom: number): RasterRect | null {
  if (!pages.length) return null
  const gap = PAGE_GAP / zoom
  const top = Math.min(...pages.map((page) => page.y0)) - gap
  return {
    x0: Math.min(...pages.map((page) => page.x0)) - 24,
    y0: top,
    x1: Math.max(...pages.map((page) => page.x1)) + 24,
    y1: Math.max(Math.max(...pages.map((page) => page.y1)) + gap, top + height / zoom),
  }
}
