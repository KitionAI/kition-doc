import { useEffect, useRef, useState } from 'react'
import { CaseSensitiveIcon as CaseSensitive, ChevronDownIcon as ChevronDown, ChevronUpIcon as ChevronUp, WholeWordIcon as WholeWord, XIcon as X } from '@kition/tessera/icons/react'
import { Button, IconButton, useKitEditor } from '@kition/sdk/editor'

/** A step asked for from outside the bar (Cmd+G, F3, Cmd+Shift+G): `id` grows with each press. */
export type FindRequest = { direction: 1 | -1; id: number }

/**
 * Find and replace under the ribbon (the engine's `edit.find`, `edit.findNext`, `edit.replace`;
 * plan "Doc usable by hand", stage 0A, section 2 row 30; walkthrough Q2, Q4, K2; WPS W-R1–W-R5).
 * Enter finds the next match and Shift+Enter the previous one; Match Case and Whole Words narrow
 * the search; Escape and Close leave the last match selected, and the keyboard goes back to the text.
 * `initial` is the text last searched for, shown again when the bar opens; `request` steps
 * through the matches from the keys; `focus` (a counter) puts the keyboard back in the field.
 */
export function FindBar({ replacing, initial, request, focus, onText, onClose }: { replacing: boolean; initial: string; request?: FindRequest; focus: number; onText: (text: string) => void; onClose: () => void }) {
  const { run, enabled, t } = useKitEditor()
  // Replacing edits the document: a viewer (or a document that cannot be edited) finds only.
  const canReplace = enabled('edit.replace')
  const [text, setText] = useState(initial)
  const [replacement, setReplacement] = useState('')
  const [matchCase, setMatchCase] = useState(false)
  const [wholeWord, setWholeWord] = useState(false)
  const [found, setFound] = useState<{ count: number; index: number } | null>(null)
  const field = useRef<HTMLInputElement>(null)
  // Typing starts a search per key; only the latest one's answer is shown.
  const latest = useRef(0)
  const options = { matchCase, wholeWord }

  const find = async (value: string, with_ = options) => {
    setText(value)
    onText(value)
    const ask = ++latest.current
    if (!value) {
      setFound(null)
      return
    }
    const answer = await run('edit.find', { text: value, ...with_ })
    if (ask !== latest.current) return
    const count = answer ? (JSON.parse(answer) as { count: number }).count : 0
    setFound({ count, index: count ? 0 : -1 })
  }
  const step = async (command: 'edit.findNext' | 'edit.findPrevious') => {
    const answer = await run(command, {})
    if (answer) setFound(JSON.parse(answer) as { count: number; index: number })
  }
  const toggle = (next: typeof options) => {
    setMatchCase(next.matchCase)
    setWholeWord(next.wholeWord)
    void find(text, next)
  }

  // Cmd+G and Cmd+Shift+G: the next or the previous match of the text searched for.
  const stepRef = useRef(step)
  stepRef.current = step
  const textRef = useRef(text)
  textRef.current = text
  useEffect(() => {
    if (request && textRef.current) void stepRef.current(request.direction > 0 ? 'edit.findNext' : 'edit.findPrevious')
  }, [request])
  useEffect(() => {
    if (focus) {
      field.current?.focus()
      field.current?.select()
    }
  }, [focus])

  return (
    <div className="kui-bar" data-testid="doc-find-bar">
      <input
        ref={field}
        className="kui-field"
        style={{ width: 220 }}
        autoFocus
        value={text}
        placeholder={t('findPlaceholder')}
        onFocus={(event) => event.currentTarget.select()}
        onChange={(event) => void find(event.target.value)}
        onKeyDown={(event) => {
          // Enter, Cmd+G and F3 find the next match; with Shift the previous one (the editor's keys
          // do not reach a field).
          const command = event.metaKey || event.ctrlKey
          if (event.key === 'Enter' || event.key === 'F3' || (command && !event.altKey && event.code === 'KeyG')) {
            event.preventDefault()
            void step(event.shiftKey ? 'edit.findPrevious' : 'edit.findNext')
          }
          // Closing the bar is what this Escape cancelled.
          if (event.key === 'Escape') {
            event.preventDefault()
            onClose()
          }
        }}
        data-testid="doc-find-text"
      />
      <IconButton icon={CaseSensitive} label={t('matchCase')} pressed={matchCase} onClick={() => toggle({ matchCase: !matchCase, wholeWord })} data-testid="doc-find-case" />
      <IconButton icon={WholeWord} label={t('wholeWord')} pressed={wholeWord} onClick={() => toggle({ matchCase, wholeWord: !wholeWord })} data-testid="doc-find-word" />
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
          <Button disabled={!enabled('edit.replaceAll') || !found?.count} onClick={() => void run('edit.replaceAll', { text, with: replacement, ...options }).then(() => find(text))} data-testid="doc-replace-all">
            {t('replaceAll')}
          </Button>
        </>
      ) : null}
      <span style={{ flex: 1 }} />
      <IconButton icon={X} label={t('close')} onClick={onClose} data-testid="doc-find-close" />
    </div>
  )
}
