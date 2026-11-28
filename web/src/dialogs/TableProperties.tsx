import { useEffect, useState } from 'react'
import { useKitEditor } from '@kition/sdk/editor'
import { DialogShell, Row, numberOf, shown } from './Shell'

/**
 * Table Properties (Table Layout › Properties, Cell Margins; the right-click menu's Table
 * Properties…; plan "Doc usable by hand" stage 6A item 4; walkthrough G8): the table's alignment
 * and indent; the caret's row's height and its rule, whether it may break across pages and repeats
 * as a header; its column's width; its cell's vertical alignment; and the table's cell margins.
 * It starts from the table as it is (`table.properties` with no settings answers them and changes
 * nothing). OK runs one `table.properties` with what was changed, and one `table.cellMargins` when
 * the margins were (a step each).
 */
type Current = {
  align?: string
  indent?: number
  columnWidth?: number | null
  cellMargins?: [number, number, number, number] | null
  row?: { height?: number | null; heightRule?: string; header?: boolean; cantSplit?: boolean } | null
  cell?: { valign?: string } | null
}
type Fields = { align: string; indent: string; height: string; rule: string; header: boolean; allowBreak: boolean; column: string; valign: string; top: string; left: string; bottom: string; right: string }

export function TableProperties() {
  const { run, query, t } = useKitEditor()
  const [start, setStart] = useState<Fields | null>(null)
  const [fields, setFields] = useState<Fields | null>(null)
  useEffect(() => {
    let stale = false
    void query<Current>('table.properties', {}).then((now) => {
      if (stale) return
      const margins = now?.cellMargins ?? [0, 5.4, 0, 5.4]
      const values: Fields = {
        align: now?.align ?? 'left',
        indent: shown(now?.indent ?? 0),
        height: shown(now?.row?.height ?? null),
        rule: now?.row?.heightRule === 'exact' ? 'exact' : 'atLeast',
        header: now?.row?.header === true,
        allowBreak: now?.row?.cantSplit !== true,
        column: shown(now?.columnWidth ?? null),
        valign: now?.cell?.valign ?? 'top',
        top: shown(margins[0]),
        left: shown(margins[1]),
        bottom: shown(margins[2]),
        right: shown(margins[3]),
      }
      setStart(values)
      setFields(values)
    })
    return () => {
      stale = true
    }
  }, [query])
  const set = <K extends keyof Fields>(key: K, value: Fields[K]) => setFields((was) => (was ? { ...was, [key]: value } : was))
  const clamp = (value: number | null, low: number, high: number) => (value == null ? null : Math.max(low, Math.min(high, value)))

  const ok = async () => {
    if (!fields || !start) return false
    const params: Record<string, unknown> = {}
    if (fields.align !== start.align) params.align = fields.align
    if (fields.indent !== start.indent) {
      const value = clamp(numberOf(fields.indent), -1584, 1584)
      if (value != null) params.indent = value
    }
    if (fields.height !== start.height || fields.rule !== start.rule) {
      params.rowHeight = fields.height.trim() === '' ? null : clamp(numberOf(fields.height), 1, 1584)
      params.rowHeightRule = fields.rule
    }
    if (fields.header !== start.header) params.headerRow = fields.header
    if (fields.allowBreak !== start.allowBreak) params.allowBreak = fields.allowBreak
    if (fields.column !== start.column) {
      const value = clamp(numberOf(fields.column), 1, 1584)
      if (value != null) params.columnWidth = value
    }
    if (fields.valign !== start.valign) params.valign = fields.valign
    if (Object.keys(params).length && (await run('table.properties', params)) == null) return false
    const margins = (['top', 'left', 'bottom', 'right'] as const).map((side) => clamp(numberOf(fields[side]), 0, 144))
    const marginsChanged = (['top', 'left', 'bottom', 'right'] as const).some((side) => fields[side] !== start[side])
    if (marginsChanged && margins.every((value) => value != null)) {
      const [top, left, bottom, right] = margins
      if ((await run('table.cellMargins', { top, left, bottom, right })) == null) return false
    }
    return true
  }

  const select = (key: 'align' | 'rule' | 'valign', values: string[], prefix: string, testId: string) =>
    fields ? (
      <select className="kui-field" value={fields[key]} onChange={(event) => set(key, event.target.value)} data-testid={testId}>
        {values.map((value) => (
          <option key={value} value={value}>
            {t(`${prefix}.${value}`)}
          </option>
        ))}
      </select>
    ) : null
  const number = (key: 'indent' | 'height' | 'column' | 'top' | 'left' | 'bottom' | 'right', testId: string, placeholder?: string) =>
    fields ? (
      <span className="doc-dialog-measure">
        <input className="kui-field" inputMode="decimal" value={fields[key]} placeholder={placeholder} onChange={(event) => set(key, event.target.value)} data-testid={testId} />
        <span className="doc-dialog-suffix">{t('unit.pt')}</span>
      </span>
    ) : null

  return (
    <DialogShell title={t('tableTitle')} testId="doc-table-properties" onOk={ok} okDisabled={!fields} wide>
      {fields ? (
        <>
          <div className="doc-dialog-heading">{t('tableTable')}</div>
          <div className="doc-dialog-columns">
            <Row label={t('tableAlign')}>{select('align', ['left', 'center', 'right'], 'align', 'doc-table-properties-align')}</Row>
            <Row label={t('tableIndent')}>{number('indent', 'doc-table-properties-indent')}</Row>
          </div>
          <div className="doc-dialog-heading">{t('tableRow')}</div>
          <div className="doc-dialog-columns">
            <Row label={t('tableRowHeight')}>{number('height', 'doc-table-properties-height', t('tableAuto'))}</Row>
            <Row label={t('tableRowRule')}>{select('rule', ['atLeast', 'exact'], 'heightRule', 'doc-table-properties-rule')}</Row>
          </div>
          <div className="doc-dialog-checks">
            <label className="doc-dialog-choice">
              <input type="checkbox" checked={fields.allowBreak} onChange={(event) => set('allowBreak', event.target.checked)} data-testid="doc-table-properties-break" />
              {t('tableAllowBreak')}
            </label>
            <label className="doc-dialog-choice">
              <input type="checkbox" checked={fields.header} onChange={(event) => set('header', event.target.checked)} data-testid="doc-table-properties-header" />
              {t('tableHeaderRow')}
            </label>
          </div>
          <div className="doc-dialog-columns">
            <div>
              <div className="doc-dialog-heading">{t('tableColumn')}</div>
              <Row label={t('tableColumnWidth')}>{number('column', 'doc-table-properties-column')}</Row>
            </div>
            <div>
              <div className="doc-dialog-heading">{t('tableCell')}</div>
              <Row label={t('tableValign')}>{select('valign', ['top', 'center', 'bottom'], 'valign', 'doc-table-properties-valign')}</Row>
            </div>
          </div>
          <div className="doc-dialog-heading">{t('tableMargins')}</div>
          <div className="doc-dialog-columns">
            <Row label={t('side.top')}>{number('top', 'doc-table-properties-top')}</Row>
            <Row label={t('side.bottom')}>{number('bottom', 'doc-table-properties-bottom')}</Row>
          </div>
          <div className="doc-dialog-columns">
            <Row label={t('side.left')}>{number('left', 'doc-table-properties-left')}</Row>
            <Row label={t('side.right')}>{number('right', 'doc-table-properties-right')}</Row>
          </div>
        </>
      ) : null}
    </DialogShell>
  )
}
