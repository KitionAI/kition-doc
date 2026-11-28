import { useEffect, useState } from 'react'
import { useKitEditor } from '@kition/sdk/editor'
import { DialogShell, Row } from './Shell'

/**
 * Hyperlink (Cmd+K; Insert › Link; the right-click menu's Hyperlink…; plan "Doc usable by hand"
 * stage 6A item 4; walkthrough G11): the text to show and the address. With text selected, the
 * selection is the link's text (shown, not edited); with none, the text typed goes in at the caret
 * (the address when left empty). OK runs one `link.insert`.
 */
export function LinkDialog() {
  const { run, query, t } = useKitEditor()
  const [selected, setSelected] = useState<string | null>(null)
  const [text, setText] = useState('')
  const [url, setUrl] = useState('')
  useEffect(() => {
    let stale = false
    void query<{ text?: string }>('file.selection').then((selection) => {
      if (stale) return
      const now = selection?.text ?? ''
      setSelected(now)
      setText(now)
    })
    return () => {
      stale = true
    }
  }, [query])
  const address = url.trim()
  const ok = async () => {
    if (!address) return false
    const params: Record<string, unknown> = { url: /^[a-z][a-z0-9+.-]*:|^#|^mailto:/i.test(address) ? address : address.includes('@') && !address.includes('/') ? `mailto:${address}` : `https://${address}` }
    if (!selected && text.trim()) params.text = text
    return (await run('link.insert', params)) != null
  }
  return (
    <DialogShell title={t('linkTitle')} testId="doc-link-dialog" onOk={ok} okDisabled={!address || selected == null}>
      <Row label={t('linkText')}>
        <input className="kui-field" value={text} disabled={!!selected} onChange={(event) => setText(event.target.value)} data-testid="doc-link-dialog-text" />
      </Row>
      <Row label={t('linkAddress')}>
        <input className="kui-field" value={url} placeholder="https://" data-first onChange={(event) => setUrl(event.target.value)} data-testid="doc-link-dialog-url" />
      </Row>
    </DialogShell>
  )
}
