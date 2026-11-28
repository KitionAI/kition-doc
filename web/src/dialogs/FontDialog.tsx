import { useEffect, useState } from 'react'
import { STANDARD_COLORS, useKitEditor } from '@kition/sdk/editor'
import { FONTS, type FormatState } from '../doc'
import { CHINESE_SIZES, sizeOf } from '../fontSizes'
import { DialogShell, Row, numberOf } from './Shell'

/**
 * Font (Cmd+D; Home › Font's launcher; plan "Doc usable by hand" stage 6A item 2; walkthrough G8;
 * WPS W-L6): the Chinese and the Western font, the style, the size (a Chinese size name or points,
 * chosen or typed), the colour, the underline, strikethrough, superscript and subscript, and the
 * spacing between characters, over a line of preview. It starts from the selection's formatting;
 * OK runs one `format.set` with only what was changed (one undo step); a field left as it was
 * keeps each character's own value.
 */
const CHINESE_FONTS = ['宋体', '黑体', '楷体', '仿宋', '微软雅黑', '等线']
const UNDERLINES = ['none', 'single', 'double', 'thick', 'dotted', 'dash', 'wave', 'words'] as const
const STYLES = ['regular', 'italic', 'bold', 'boldItalic'] as const
type Style = (typeof STYLES)[number]

const styleOf = (format: FormatState): Style => (format.bold && format.italic ? 'boldItalic' : format.bold ? 'bold' : format.italic ? 'italic' : 'regular')

type Fields = { eastAsia: string; font: string; style: Style; size: string; color: string; underline: string; strike: boolean; superscript: boolean; subscript: boolean; spacing: string }

export function FontDialog() {
  const { run, query, t } = useKitEditor()
  const [start, setStart] = useState<Fields | null>(null)
  const [fields, setFields] = useState<Fields | null>(null)
  useEffect(() => {
    let stale = false
    void query<FormatState>('format.state').then((format) => {
      if (stale) return
      const now: Fields = {
        eastAsia: '',
        font: format?.font ?? '',
        style: styleOf(format ?? {}),
        size: format?.size != null ? String(format.size) : '',
        color: format?.color ?? '',
        underline: format?.underline ? 'single' : 'none',
        strike: format?.strike === true,
        superscript: format?.superscript === true,
        subscript: format?.subscript === true,
        spacing: '',
      }
      setStart(now)
      setFields(now)
    })
    return () => {
      stale = true
    }
  }, [query])
  const set = <K extends keyof Fields>(key: K, value: Fields[K]) => setFields((was) => (was ? { ...was, [key]: value } : was))
  const size = fields ? sizeOf(fields.size) : null
  const badSize = fields != null && fields.size.trim() !== '' && size == null

  const ok = async () => {
    if (!fields || !start) return false
    const props: Record<string, unknown> = {}
    if (fields.eastAsia.trim()) props.fontEa = fields.eastAsia.trim()
    if (fields.font.trim() && fields.font !== start.font) props.font = fields.font.trim()
    if (fields.style !== start.style) {
      props.bold = fields.style === 'bold' || fields.style === 'boldItalic'
      props.italic = fields.style === 'italic' || fields.style === 'boldItalic'
    }
    if (size != null && fields.size !== start.size) props.size = size
    if (fields.color && fields.color !== start.color) props.color = fields.color
    if (fields.underline !== start.underline) props.underline = fields.underline
    if (fields.strike !== start.strike) props.strike = fields.strike
    if (fields.superscript !== start.superscript || fields.subscript !== start.subscript) props.vertAlign = fields.superscript ? 'superscript' : fields.subscript ? 'subscript' : 'baseline'
    const spacing = numberOf(fields.spacing)
    if (spacing != null) props.spacing = Math.max(-100, Math.min(100, spacing))
    if (!Object.keys(props).length) return true
    return (await run('format.set', { props })) != null
  }

  const preview = fields
    ? {
        fontFamily: [fields.font, fields.eastAsia].filter(Boolean).map((name) => `"${name}"`).join(', ') || undefined,
        fontSize: `${Math.min(48, (size ?? 12) * (4 / 3))}px`,
        fontWeight: fields.style === 'bold' || fields.style === 'boldItalic' ? 700 : 400,
        fontStyle: fields.style === 'italic' || fields.style === 'boldItalic' ? 'italic' : 'normal',
        textDecoration: [fields.underline !== 'none' ? 'underline' : '', fields.strike ? 'line-through' : ''].filter(Boolean).join(' ') || 'none',
        color: fields.color && fields.color !== 'auto' ? `#${fields.color}` : undefined,
        letterSpacing: numberOf(fields.spacing) != null ? `${numberOf(fields.spacing)}pt` : undefined,
      }
    : {}

  return (
    <DialogShell title={t('fontTitle')} testId="doc-font-dialog" onOk={ok} okDisabled={!fields || badSize} wide>
      {fields ? (
        <>
          <div className="doc-dialog-columns">
            <Row label={t('fontEastAsia')}>
              <input className="kui-field" list="doc-font-ea-list" value={fields.eastAsia} placeholder={t('fontUnchanged')} onChange={(event) => set('eastAsia', event.target.value)} data-testid="doc-font-dialog-ea" />
            </Row>
            <Row label={t('fontWestern')}>
              <input className="kui-field" list="doc-font-list" value={fields.font} placeholder={t('fontUnchanged')} onChange={(event) => set('font', event.target.value)} data-testid="doc-font-dialog-font" />
            </Row>
          </div>
          <datalist id="doc-font-ea-list">
            {CHINESE_FONTS.map((name) => (
              <option key={name} value={name} />
            ))}
          </datalist>
          <datalist id="doc-font-list">
            {FONTS.map((name) => (
              <option key={name} value={name} />
            ))}
          </datalist>
          <div className="doc-dialog-columns">
            <Row label={t('fontStyle')}>
              <select className="kui-field" value={fields.style} onChange={(event) => set('style', event.target.value as Style)} data-testid="doc-font-dialog-style">
                {STYLES.map((style) => (
                  <option key={style} value={style}>
                    {t(`fontStyle.${style}`)}
                  </option>
                ))}
              </select>
            </Row>
            <Row label={t('fontSize')}>
              <input className="kui-field" list="doc-size-list" value={fields.size} aria-invalid={badSize} onChange={(event) => set('size', event.target.value)} data-testid="doc-font-dialog-size" />
            </Row>
          </div>
          <datalist id="doc-size-list">
            {CHINESE_SIZES.map(([name]) => (
              <option key={name} value={name} />
            ))}
            {[8, 9, 10, 10.5, 11, 12, 14, 16, 18, 20, 22, 24, 26, 28, 36, 48, 72].map((value) => (
              <option key={value} value={String(value)} />
            ))}
          </datalist>
          <div className="doc-dialog-columns">
            <Row label={t('fontColor')}>
              <select className="kui-field" value={fields.color} onChange={(event) => set('color', event.target.value)} data-testid="doc-font-dialog-color">
                {fields.color && fields.color !== 'auto' && !STANDARD_COLORS.some((color) => color.replace('#', '').toUpperCase() === fields.color) ? <option value={fields.color}>#{fields.color}</option> : null}
                {!fields.color ? <option value="" /> : null}
                <option value="auto">{t('fontColorAuto')}</option>
                {STANDARD_COLORS.map((color) => (
                  <option key={color} value={color.replace('#', '').toUpperCase()}>
                    {color.toUpperCase()}
                  </option>
                ))}
              </select>
            </Row>
            <Row label={t('fontUnderline')}>
              <select className="kui-field" value={fields.underline} onChange={(event) => set('underline', event.target.value)} data-testid="doc-font-dialog-underline">
                {UNDERLINES.map((kind) => (
                  <option key={kind} value={kind}>
                    {t(`underline.${kind}`)}
                  </option>
                ))}
              </select>
            </Row>
          </div>
          <div className="doc-dialog-checks">
            <label className="doc-dialog-choice">
              <input type="checkbox" checked={fields.strike} onChange={(event) => set('strike', event.target.checked)} data-testid="doc-font-dialog-strike" />
              {t('fontStrike')}
            </label>
            <label className="doc-dialog-choice">
              <input type="checkbox" checked={fields.superscript} onChange={(event) => setFields((was) => (was ? { ...was, superscript: event.target.checked, subscript: event.target.checked ? false : was.subscript } : was))} data-testid="doc-font-dialog-superscript" />
              {t('fontSuperscript')}
            </label>
            <label className="doc-dialog-choice">
              <input type="checkbox" checked={fields.subscript} onChange={(event) => setFields((was) => (was ? { ...was, subscript: event.target.checked, superscript: event.target.checked ? false : was.superscript } : was))} data-testid="doc-font-dialog-subscript" />
              {t('fontSubscript')}
            </label>
          </div>
          <Row label={t('fontSpacing')}>
            <input className="kui-field" inputMode="decimal" value={fields.spacing} placeholder={t('fontUnchanged')} onChange={(event) => set('spacing', event.target.value)} data-testid="doc-font-dialog-spacing" />
          </Row>
          <div className="doc-dialog-preview" aria-label={t('fontPreview')} data-testid="doc-font-dialog-preview">
            <span style={preview}>{t('fontPreviewText')}</span>
          </div>
        </>
      ) : null}
    </DialogShell>
  )
}

