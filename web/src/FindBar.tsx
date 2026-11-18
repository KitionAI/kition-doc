import { useRef, useState } from 'react'
import { ChevronDownIcon as ChevronDown, ChevronUpIcon as ChevronUp, XIcon as X } from '@kition/tessera-icons/react'
import { Button, IconButton, useKitEditor } from '@kition/kit-editor'

/** Find and replace under the ribbon (the engine's `edit.find`, `edit.findNext`, `edit.replace`). */
export function FindBar({ replacing, onClose }: { replacing: boolean; onClose: () => void }) {
  const { run, enabled, t } = useKitEditor()
  // Replacing edits the document: a viewer (or a document that cannot be edited) finds only.
  const canReplace = enabled('edit.replace')
  const [text, setText] = useState('')
  const [replacement, setReplacement] = useState('')
  const [found, setFound] = useState<{ count: number; index: number } | null>(null)
  // Typing starts a search per key; only the latest one's answer is shown.
  const latest = useRef(0)

  const find = async (value: string) => {
    setText(value)
    const ask = ++latest.current
    if (!value) {
      setFound(null)
      return
    }
    const answer = await run('edit.find', { text: value })
    if (ask !== latest.current) return
    const count = answer ? (JSON.parse(answer) as { count: number }).count : 0
    setFound({ count, index: count ? 0 : -1 })
  }
  const step = async (command: 'edit.findNext' | 'edit.findPrevious') => {
    const answer = await run(command, {})
    if (answer) setFound(JSON.parse(answer) as { count: number; index: number })
  }

  return (
    <div className="kui-bar" data-testid="doc-find-bar">
      <input
        className="kui-field"
        style={{ width: 220 }}
        autoFocus
        value={text}
        placeholder={t('findPlaceholder')}
        onChange={(event) => void find(event.target.value)}
        onKeyDown={(event) => {
          if (event.key === 'Enter') void step(event.shiftKey ? 'edit.findPrevious' : 'edit.findNext')
          // Closing the bar is what this Escape cancelled.
          if (event.key === 'Escape') {
            event.preventDefault()
            onClose()
          }
        }}
        data-testid="doc-find-text"
      />
      <span className="kui-muted" style={{ minWidth: 48 }} data-testid="doc-find-count">
        {found ? `${found.count ? found.index + 1 : 0}/${found.count}` : ''}
      </span>
      <IconButton icon={ChevronUp} label={t('findPrevious')} disabled={!found?.count} onClick={() => void step('edit.findPrevious')} />
      <IconButton icon={ChevronDown} label={t('findNext')} disabled={!found?.count} onClick={() => void step('edit.findNext')} />
      {replacing ? (
        <>
          <input className="kui-field" style={{ width: 200 }} value={replacement} disabled={!canReplace} placeholder={t('replacePlaceholder')} onChange={(event) => setReplacement(event.target.value)} data-testid="doc-replace-text" />
          <Button disabled={!canReplace || !found?.count} onClick={() => void run('edit.replace', { text, with: replacement }).then(() => find(text))}>
            {t('replace')}
          </Button>
          <Button disabled={!enabled('edit.replaceAll') || !found?.count} onClick={() => void run('edit.replaceAll', { text, with: replacement }).then(() => find(text))} data-testid="doc-replace-all">
            {t('replaceAll')}
          </Button>
        </>
      ) : null}
      <span style={{ flex: 1 }} />
      <IconButton icon={X} label={t('close')} onClick={onClose} />
    </div>
  )
}
