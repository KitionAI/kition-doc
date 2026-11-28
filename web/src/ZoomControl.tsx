/**
 * The page view's zoom in the status bar (plan "Doc usable by hand", stage 0A; walkthrough A5, A6;
 * WPS W-U2, W-U3): − and + go 10 points at a time to a multiple of 10, the slider covers 10%–500%,
 * the percentage opens the choices (200%, 100%, 75%, Page Width, One Page, a percentage typed in),
 * and Fit shows the current page whole, never every page squeezed into the window.
 */
import { useState } from 'react'
import { MenuButton, MenuItem, MenuSeparator, useKitEditor } from '@kition/sdk/editor'
import { MAX_ZOOM, MIN_ZOOM, clampZoom, stepZoom } from './zoom'
import { focusText } from './focus'

/** The zoom presets the choices offer, in percent. */
const PRESETS = [200, 100, 75]

export type ZoomActions = {
  /** The view's zoom now (1 is 100%: a CSS pixel per point). */
  zoom: number
  /** Zooms the view to `zoom` (clamped to 10%–500%). */
  zoomTo: (zoom: number) => void
  /** The current page's width fills the view. */
  pageWidth: () => void
  /** The current page whole in view. */
  onePage: () => void
}

const toSlider = (zoom: number) => Math.log(zoom / MIN_ZOOM) / Math.log(MAX_ZOOM / MIN_ZOOM)
const fromSlider = (value: number) => MIN_ZOOM * Math.pow(MAX_ZOOM / MIN_ZOOM, value)

/** The choices under the percentage. */
function ZoomChoices({ zoom, zoomTo, pageWidth, onePage }: ZoomActions) {
  const { t } = useKitEditor()
  const percent = Math.round(zoom * 100)
  const [typed, setTyped] = useState(String(percent))
  const done = (action: () => void) => () => {
    action()
    focusText()
  }
  const apply = () => {
    const value = Number(typed.replace('%', '').trim())
    if (Number.isFinite(value) && value > 0) zoomTo(value / 100)
    focusText()
  }
  return (
    <>
      {PRESETS.map((value) => (
        <MenuItem key={value} label={`${value}%`} checked={percent === value} onSelect={done(() => zoomTo(value / 100))} testId={`doc-zoom-menu-${value}`} />
      ))}
      <MenuItem label={t('pageWidth')} onSelect={done(pageWidth)} testId="doc-zoom-menu-width" />
      <MenuItem label={t('onePage')} onSelect={done(onePage)} testId="doc-zoom-menu-page" />
      <MenuSeparator />
      <div className="doc-zoom-custom">
        <span>{t('zoomCustom')}</span>
        <input
          className="kui-ribbon-field doc-zoom-field"
          inputMode="numeric"
          aria-label={t('zoomCustom')}
          value={typed}
          onFocus={(event) => event.currentTarget.select()}
          onChange={(event) => setTyped(event.target.value)}
          onKeyDown={(event) => {
            if (event.key !== 'Escape') event.stopPropagation()
            if (event.key === 'Enter') {
              event.preventDefault()
              event.currentTarget.parentElement?.querySelector<HTMLButtonElement>('button')?.click()
            }
          }}
          data-testid="doc-zoom-menu-field"
        />
        <span>%</span>
        <button type="button" className="kui-button" data-variant="ghost" onClick={apply} data-testid="doc-zoom-menu-apply">
          {t('zoomApply')}
        </button>
      </div>
    </>
  )
}

/** The status bar's zoom: −, the slider, +, the percentage that opens the choices, and Fit (the page). */
export function ZoomControl(actions: ZoomActions) {
  const { t } = useKitEditor()
  const { zoom, zoomTo, onePage } = actions
  const shown = clampZoom(zoom)
  return (
    <div className="kui-zoom doc-zoom" data-testid="doc-zoom">
      <button type="button" className="kui-icon-button" aria-label={t('zoomOut')} title={t('zoomOut')} onClick={() => zoomTo(stepZoom(shown, -1))} data-testid="doc-zoom-out">
        −
      </button>
      <input type="range" min={0} max={1} step={0.001} value={toSlider(shown)} onChange={(event) => zoomTo(fromSlider(Number(event.target.value)))} onPointerUp={() => focusText()} aria-label={t('zoomMenu')} data-testid="doc-zoom-slider" />
      <button type="button" className="kui-icon-button" aria-label={t('zoomIn')} title={t('zoomIn')} onClick={() => zoomTo(stepZoom(shown, 1))} data-testid="doc-zoom-in">
        +
      </button>
      <MenuButton
        placement="auto"
        align="end"
        trigger={({ open, toggle }) => (
          <button type="button" className="kui-zoom-value doc-zoom-value" aria-expanded={open} aria-haspopup="menu" title={t('zoomMenu')} onClick={toggle} data-testid="kit-zoom">
            {Math.round(zoom * 100)}%
          </button>
        )}
      >
        <ZoomChoices {...actions} />
      </MenuButton>
      <button type="button" className="kui-button" data-variant="ghost" title={t('onePage')} onClick={() => onePage()} data-testid="doc-zoom-fit">
        {t('onePage')}
      </button>
    </div>
  )
}
