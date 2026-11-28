/**
 * Layout Options (plan "Doc usable by hand", stage 5A item 4; walkthrough P8; WPS W-O1, W-O7): a
 * small button beside the selected picture's top-right corner opens a panel of the ways text goes
 * around it, In Line with Text and the six wrappings, the current one checked; a choice runs one
 * `arrange.wrap` (one undo step). Picture Format's and Layout's Wrap Text open the same panel, by
 * the picture (`asks.ts`). It closes on Escape (which leaves the picture selected), a press
 * elsewhere, a choice or the wheel.
 */
import { useEffect } from 'react'
import { RotateCwIcon as RotateCw, TextWrapIcon as WrapText } from '@kition/tessera/icons/react'
import { CommandButton, CommandItem, ContextMenu, IconButton, MenuItem, MenuSeparator, RibbonGroup, RibbonMenu, useKitEditor, type ContextMenuPoint, type KitCanvasView } from '@kition/sdk/editor'
import type { PictureBox } from './doc'
import { focusText, keyboardLost } from './focus'

const WRAPPED = ['square', 'tight', 'through', 'topAndBottom', 'behindText', 'inFrontOfText'] as const
/** The button's size and its gap from the picture, screen pixels. */
const BUTTON = 24
const GAP = 8

/** Where the button is in the canvas's viewport (CSS pixels): beside the picture's top-right corner. */
export function buttonPlace(view: KitCanvasView, picture: PictureBox) {
  return { left: (picture.x + picture.width - view.x) * view.zoom + GAP, top: (picture.y - view.y) * view.zoom }
}

/** The button beside the selected picture, while it is in view. */
export function LayoutOptionsButton({ view, picture, onOpen }: { view: KitCanvasView; picture: PictureBox; onOpen: (at: ContextMenuPoint) => void }) {
  const { t } = useKitEditor()
  const { left, top } = buttonPlace(view, picture)
  if (top + BUTTON < 0 || top > view.height || left < 0 || left > view.width) return null
  return (
    <div className="doc-layout-options">
      <span className="doc-layout-options-button" style={{ left, top }}>
        <IconButton
          icon={WrapText}
          label={t('layoutOptions')}
          // The keyboard stays in the text (the panel gives it back there).
          onPointerDown={(event) => event.preventDefault()}
          onClick={(event) => {
            const box = event.currentTarget.getBoundingClientRect()
            onOpen({ x: box.left, y: box.bottom + 4 })
          }}
          data-testid="doc-layout-options"
        />
      </span>
    </div>
  )
}

/** The panel: In Line with Text, then the six wrappings, the picture's own checked. */
export function LayoutOptionsPanel({ at, wrap, onClose }: { at: ContextMenuPoint | null; wrap: string | null; onClose: () => void }) {
  const { run, enabled, t } = useKitEditor()
  useEffect(() => {
    if (!at) return undefined
    const close = () => onClose()
    // Escape only closes the panel: the picture stays selected (the editor's keys never see it).
    const escape = (event: KeyboardEvent) => {
      if (event.key !== 'Escape') return
      event.stopPropagation()
      event.preventDefault()
      onClose()
      window.setTimeout(() => {
        if (keyboardLost()) focusText()
      }, 0)
    }
    window.addEventListener('wheel', close, { capture: true, passive: true })
    window.addEventListener('blur', close)
    window.addEventListener('keydown', escape, true)
    return () => {
      window.removeEventListener('wheel', close, { capture: true })
      window.removeEventListener('blur', close)
      window.removeEventListener('keydown', escape, true)
    }
  }, [at, onClose])
  const close = () => {
    onClose()
    window.setTimeout(() => {
      if (keyboardLost()) focusText()
    }, 0)
  }
  const item = (value: string) => (
    <MenuItem key={value} label={t(`menuWrap.${value}`)} checked={wrap === value} disabled={!enabled('arrange.wrap')} onSelect={() => void run('arrange.wrap', { wrap: value })} testId={`doc-layout-wrap-${value}`} />
  )
  return (
    <ContextMenu at={at} onClose={close} label={t('layoutOptions')} testId="doc-layout-options-panel">
      {item('inline')}
      <MenuSeparator />
      {WRAPPED.map(item)}
    </ContextMenu>
  )
}

/** Picture Format's Arrange: Wrap Text (the Layout Options panel) and Rotate. */
export function PictureArrangeGroup() {
  const { phrase: p, t, enabled } = useKitEditor()
  return (
    <RibbonGroup label={p('Arrange')} icon={WrapText}>
      <CommandButton command="arrange.wrap" icon={WrapText} label={p('Wrap Text')} primary />
      <RibbonMenu icon={RotateCw} label={t('rotateMenu')} disabled={!enabled('arrange.rotate')} testId="doc-picture-rotate" primary>
        {(['right', 'left', 'flipVertical', 'flipHorizontal'] as const).map((direction) => (
          <CommandItem key={direction} command="arrange.rotate" params={{ direction }} label={t(`rotate.${direction}`)} />
        ))}
      </RibbonMenu>
    </RibbonGroup>
  )
}
