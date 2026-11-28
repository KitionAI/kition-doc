import { useEffect, useState } from 'react'
import { useKitEditor } from '@kition/sdk/editor'
import type { FormatState } from '../doc'
import { DialogShell, Row, numberOf, shown } from './Shell'

/**
 * Paragraph (Alt+Cmd+M; Layout › Indents and Paragraph Spacing; plan "Doc usable by hand" stage
 * 6A item 3; walkthrough L2, G8; WPS W-L6): the alignment, the left and right indents, the special
 * indent (first line or hanging) and by how much, the space before and after, and the line
 * spacing. An indent can be in characters (WPS's unit, the default in Chinese): it is stored as a
 * length at the caret's font size (two characters of 12 pt text are 24 pt), no new field in the
 * file. It starts from the caret's paragraph; OK runs one `paragraph.set` on the selected
 * paragraphs with what was changed (one undo step).
 */
const ALIGNS = ['left', 'center', 'right', 'justify', 'distribute'] as const
const UNITS = ['char', 'pt', 'cm'] as const
type Unit = (typeof UNITS)[number]
const SPECIALS = ['none', 'firstLine', 'hanging'] as const
type Special = (typeof SPECIALS)[number]
const RULES = ['single', 'oneAndHalf', 'double', 'multiple', 'atLeast', 'exactly'] as const
type Rule = (typeof RULES)[number]

type Resolved = { align?: string; indentLeft?: number; indentRight?: number; indentFirst?: number; spaceBefore?: number; spaceAfter?: number; lineSpacing?: { rule?: string; value?: number } }
type Fields = { align: string; left: string; right: string; leftUnit: Unit; rightUnit: Unit; special: Special; by: string; byUnit: Unit; before: string; after: string; rule: Rule; at: string }

const CM = 72 / 2.54

export function ParagraphDialog() {
  const { run, query, t, host } = useKitEditor()
  const chinese = host.init.locale.toLowerCase().startsWith('zh')
  const [size, setSize] = useState(12)
  const [start, setStart] = useState<Fields | null>(null)
  const [fields, setFields] = useState<Fields | null>(null)
  useEffect(() => {
    let stale = false
    void (async () => {
      const format = await query<FormatState>('format.state')
      const selection = await query<{ focus?: { story?: unknown; path?: number[] } }>('file.selection')
      const focus = selection?.focus
      const paragraph = focus?.path ? await query<{ resolved?: Resolved }>('file.paragraph', { path: focus.path, ...(focus.story ? { story: focus.story } : {}) }) : null
      if (stale) return
      const at = format?.size ?? 12
      const p = paragraph?.resolved ?? {}
      const unit: Unit = chinese ? 'char' : 'pt'
      const inUnit = (points: number, which: Unit) => shown(which === 'char' ? points / at : which === 'cm' ? points / CM : points)
      const first = p.indentFirst ?? 0
      const special: Special = first > 0 ? 'firstLine' : first < 0 ? 'hanging' : 'none'
      const line = p.lineSpacing ?? {}
      const multiple = line.rule === 'multiple' || line.rule == null
      const value = line.value ?? 1
      const rule: Rule = multiple ? (value === 1 ? 'single' : value === 1.5 ? 'oneAndHalf' : value === 2 ? 'double' : 'multiple') : line.rule === 'atLeast' ? 'atLeast' : 'exactly'
      const now: Fields = {
        align: p.align ?? 'left',
        left: inUnit(p.indentLeft ?? 0, unit),
        right: inUnit(p.indentRight ?? 0, unit),
        leftUnit: unit,
        rightUnit: unit,
        special,
        by: special === 'none' ? '' : inUnit(Math.abs(first), unit),
        byUnit: unit,
        before: shown(p.spaceBefore ?? 0),
        after: shown(p.spaceAfter ?? 0),
        rule,
        at: rule === 'multiple' || rule === 'atLeast' || rule === 'exactly' ? shown(value) : '',
      }
      setSize(at)
      setStart(now)
      setFields(now)
    })()
    return () => {
      stale = true
    }
  }, [query, chinese])
  const set = <K extends keyof Fields>(key: K, value: Fields[K]) => setFields((was) => (was ? { ...was, [key]: value } : was))
  const points = (text: string, unit: Unit) => {
    const value = numberOf(text)
    if (value == null) return null
    const pt = unit === 'char' ? value * size : unit === 'cm' ? value * CM : value
    return Math.max(-1584, Math.min(1584, Math.round(pt * 100) / 100))
  }

  const ok = async () => {
    if (!fields || !start) return false
    const props: Record<string, unknown> = {}
    if (fields.align !== start.align) props.align = fields.align
    if (fields.left !== start.left || fields.leftUnit !== start.leftUnit) {
      const value = points(fields.left || '0', fields.leftUnit)
      if (value != null) props.indentLeft = value
    }
    if (fields.right !== start.right || fields.rightUnit !== start.rightUnit) {
      const value = points(fields.right || '0', fields.rightUnit)
      if (value != null) props.indentRight = value
    }
    if (fields.special !== start.special || fields.by !== start.by || fields.byUnit !== start.byUnit) {
      const by = fields.special === 'none' ? 0 : Math.abs(points(fields.by || '0', fields.byUnit) ?? 0)
      props.indentFirst = fields.special === 'hanging' ? -by : by
    }
    if (fields.before !== start.before) {
      const value = numberOf(fields.before)
      if (value != null) Object.assign(props, { spaceBefore: Math.max(0, Math.min(1584, value)), spaceBeforeLines: null, spaceBeforeAuto: false })
    }
    if (fields.after !== start.after) {
      const value = numberOf(fields.after)
      if (value != null) Object.assign(props, { spaceAfter: Math.max(0, Math.min(1584, value)), spaceAfterLines: null, spaceAfterAuto: false })
    }
    if (fields.rule !== start.rule || fields.at !== start.at) {
      const value = numberOf(fields.at)
      const spacing =
        fields.rule === 'single' ? { rule: 'multiple', value: 1 } : fields.rule === 'oneAndHalf' ? { rule: 'multiple', value: 1.5 } : fields.rule === 'double' ? { rule: 'multiple', value: 2 } : value != null && value > 0 ? { rule: fields.rule, value } : null
      if (spacing) props.lineSpacing = spacing
    }
    if (!Object.keys(props).length) return true
    return (await run('paragraph.set', { props })) != null
  }

  const unitSelect = (key: 'leftUnit' | 'rightUnit' | 'byUnit', testId: string) =>
    fields ? (
      <select className="kui-field doc-dialog-unit" value={fields[key]} onChange={(event) => set(key, event.target.value as Unit)} aria-label={t('paraUnit')} data-testid={testId}>
        {UNITS.map((unit) => (
          <option key={unit} value={unit}>
            {t(`unit.${unit}`)}
          </option>
        ))}
      </select>
    ) : null
  const needsAt = fields?.rule === 'multiple' || fields?.rule === 'atLeast' || fields?.rule === 'exactly'

  return (
    <DialogShell title={t('paraTitle')} testId="doc-paragraph-dialog" onOk={ok} okDisabled={!fields} wide>
      {fields ? (
        <>
          <Row label={t('paraAlign')}>
            <select className="kui-field" value={fields.align} onChange={(event) => set('align', event.target.value)} data-testid="doc-paragraph-dialog-align">
              {ALIGNS.map((align) => (
                <option key={align} value={align}>
                  {t(`align.${align}`)}
                </option>
              ))}
            </select>
          </Row>
          <div className="doc-dialog-heading">{t('paraIndents')}</div>
          <div className="doc-dialog-columns">
            <Row label={t('paraLeft')}>
              <span className="doc-dialog-measure">
                <input className="kui-field" inputMode="decimal" value={fields.left} onChange={(event) => set('left', event.target.value)} data-testid="doc-paragraph-dialog-left" />
                {unitSelect('leftUnit', 'doc-paragraph-dialog-left-unit')}
              </span>
            </Row>
            <Row label={t('paraRight')}>
              <span className="doc-dialog-measure">
                <input className="kui-field" inputMode="decimal" value={fields.right} onChange={(event) => set('right', event.target.value)} data-testid="doc-paragraph-dialog-right" />
                {unitSelect('rightUnit', 'doc-paragraph-dialog-right-unit')}
              </span>
            </Row>
          </div>
          <div className="doc-dialog-columns">
            <Row label={t('paraSpecial')}>
              <select className="kui-field" value={fields.special} onChange={(event) => set('special', event.target.value as Special)} data-testid="doc-paragraph-dialog-special">
                {SPECIALS.map((special) => (
                  <option key={special} value={special}>
                    {t(`special.${special}`)}
                  </option>
                ))}
              </select>
            </Row>
            <Row label={t('paraBy')}>
              <span className="doc-dialog-measure">
                <input className="kui-field" inputMode="decimal" value={fields.by} disabled={fields.special === 'none'} onChange={(event) => set('by', event.target.value)} data-testid="doc-paragraph-dialog-by" />
                {unitSelect('byUnit', 'doc-paragraph-dialog-by-unit')}
              </span>
            </Row>
          </div>
          <div className="doc-dialog-heading">{t('paraSpacing')}</div>
          <div className="doc-dialog-columns">
            <Row label={t('paraBefore')}>
              <span className="doc-dialog-measure">
                <input className="kui-field" inputMode="decimal" value={fields.before} onChange={(event) => set('before', event.target.value)} data-testid="doc-paragraph-dialog-before" />
                <span className="doc-dialog-suffix">{t('unit.pt')}</span>
              </span>
            </Row>
            <Row label={t('paraAfter')}>
              <span className="doc-dialog-measure">
                <input className="kui-field" inputMode="decimal" value={fields.after} onChange={(event) => set('after', event.target.value)} data-testid="doc-paragraph-dialog-after" />
                <span className="doc-dialog-suffix">{t('unit.pt')}</span>
              </span>
            </Row>
          </div>
          <div className="doc-dialog-columns">
            <Row label={t('paraLineSpacing')}>
              <select className="kui-field" value={fields.rule} onChange={(event) => set('rule', event.target.value as Rule)} data-testid="doc-paragraph-dialog-rule">
                {RULES.map((rule) => (
                  <option key={rule} value={rule}>
                    {t(`lineRule.${rule}`)}
                  </option>
                ))}
              </select>
            </Row>
            <Row label={t('paraAt')}>
              <input className="kui-field" inputMode="decimal" value={needsAt ? fields.at : ''} disabled={!needsAt} onChange={(event) => set('at', event.target.value)} data-testid="doc-paragraph-dialog-at" />
            </Row>
          </div>
        </>
      ) : null}
    </DialogShell>
  )
}
