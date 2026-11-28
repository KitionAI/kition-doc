import { useEffect, useState } from 'react'
import { useKitEditor } from '@kition/sdk/editor'
import { DialogShell, Row, numberOf, shown } from './Shell'

/**
 * Size (Picture Format › Size; plan "Doc usable by hand" stage 6A item 4; walkthrough P8): the
 * selected picture's (or shape's) width and height in points, the aspect ratio locked by default
 * (WPS), or a scale. It starts from the object's size (the Selection Pane's list, matched with the
 * selection); OK runs one `image.size`.
 */
type Listed = { pos?: { path?: number[]; off?: number }; size?: [number, number] }
type Ends = { anchor?: { path?: number[]; off?: number }; focus?: { path?: number[]; off?: number } }

const same = (a?: { path?: number[]; off?: number }, b?: { path?: number[]; off?: number }) => !!a && !!b && a.off === b.off && JSON.stringify(a.path) === JSON.stringify(b.path)

export function PictureSize() {
  const { run, query, t } = useKitEditor()
  const [size, setSize] = useState<[number, number] | null>(null)
  const [width, setWidth] = useState('')
  const [height, setHeight] = useState('')
  const [lock, setLock] = useState(true)
  useEffect(() => {
    let stale = false
    void (async () => {
      const ends = await query<Ends>('file.selection')
      const objects = (await query<Listed[]>('arrange.selectionPane')) ?? []
      if (stale) return
      const found = objects.find((object) => same(object.pos, ends?.anchor) || same(object.pos, ends?.focus))
      const now = found?.size ?? null
      setSize(now)
      setWidth(shown(now?.[0] ?? null, 1))
      setHeight(shown(now?.[1] ?? null, 1))
    })()
    return () => {
      stale = true
    }
  }, [query])
  const typeWidth = (text: string) => {
    setWidth(text)
    const value = numberOf(text)
    if (lock && size && value != null && size[0] > 0) setHeight(shown((value * size[1]) / size[0], 1))
  }
  const typeHeight = (text: string) => {
    setHeight(text)
    const value = numberOf(text)
    if (lock && size && value != null && size[1] > 0) setWidth(shown((value * size[0]) / size[1], 1))
  }
  const w = numberOf(width)
  const h = numberOf(height)
  const bad = w == null || h == null || w < 1 || h < 1 || w > 4000 || h > 4000
  const ok = async () => {
    if (bad) return false
    return (await run('image.size', { width: w, height: h, lockAspect: false })) != null
  }
  return (
    <DialogShell title={t('pictureSizeTitle')} testId="doc-picture-size" onOk={ok} okDisabled={bad}>
      <Row label={t('pictureWidth')}>
        <span className="doc-dialog-measure">
          <input className="kui-field" inputMode="decimal" value={width} onChange={(event) => typeWidth(event.target.value)} data-testid="doc-picture-size-width" />
          <span className="doc-dialog-suffix">{t('unit.pt')}</span>
        </span>
      </Row>
      <Row label={t('pictureHeight')}>
        <span className="doc-dialog-measure">
          <input className="kui-field" inputMode="decimal" value={height} onChange={(event) => typeHeight(event.target.value)} data-testid="doc-picture-size-height" />
          <span className="doc-dialog-suffix">{t('unit.pt')}</span>
        </span>
      </Row>
      <label className="doc-dialog-choice">
        <input type="checkbox" checked={lock} onChange={(event) => setLock(event.target.checked)} data-testid="doc-picture-size-lock" />
        {t('pictureLock')}
      </label>
    </DialogShell>
  )
}
