import { TextAlignCenterIcon as AlignCenter, Grid2x2Icon as Grid2x2, PaintBucketIcon as PaintBucket } from '@kition/tessera/icons/react'
import { ColorGrid, CommandItem, RibbonGroup, RibbonMenu, useKitEditor } from '@kition/sdk/editor'

/**
 * Table Design's Shading and Borders, and Table Layout's cell Alignment, as menus (stage 6A item
 * 4; walkthrough H5): run without a choice they changed nothing a person sees. Shading and Borders
 * are the menus Home's paragraph ones are, for the selected cells (the table's with the caret in
 * it); Alignment is the nine places of the cell (WPS).
 */
export const TABLE_DESIGN_OWN = ['table.shading', 'table.borders']
export const TABLE_LAYOUT_OWN = ['table.cellAlign']

const hex = (color: string) => color.replace(/^#/, '').toUpperCase()
const BORDERS = ['all', 'outside', 'inside', 'none', 'top', 'bottom', 'left', 'right'] as const
const ALIGNS = ['topLeft', 'topCenter', 'topRight', 'centerLeft', 'center', 'centerRight', 'bottomLeft', 'bottomCenter', 'bottomRight'] as const

export function TableDesignGroups() {
  const { run, enabled, t, phrase: p } = useKitEditor()
  return (
    <RibbonGroup label={p('Borders')} icon={Grid2x2}>
      <RibbonMenu icon={PaintBucket} label={p('Shading')} disabled={!enabled('table.shading')} testId="doc-table-shading" primary>
        <CommandItem command="table.shading" params={{ color: null }} label={p('No Color')} />
        <ColorGrid onPick={(color) => void run('table.shading', { color: hex(color) })} testId="doc-table-shading-colors" />
      </RibbonMenu>
      <RibbonMenu icon={Grid2x2} label={p('Borders')} disabled={!enabled('table.borders')} testId="doc-table-borders" primary>
        {BORDERS.map((kind) => (
          <CommandItem key={kind} command="table.borders" params={{ kind }} label={t(`menuBorders.${kind}`)} />
        ))}
      </RibbonMenu>
    </RibbonGroup>
  )
}

export function TableLayoutGroups() {
  const { enabled, t, phrase: p } = useKitEditor()
  return (
    <RibbonGroup label={p('Alignment')} icon={AlignCenter}>
      <RibbonMenu icon={AlignCenter} label={p('Alignment')} disabled={!enabled('table.cellAlign')} testId="doc-table-cell-align" primary>
        {ALIGNS.map((value) => (
          <CommandItem key={value} command="table.cellAlign" params={{ value }} label={t(`menuAlign.${value}`)} />
        ))}
      </RibbonMenu>
    </RibbonGroup>
  )
}
