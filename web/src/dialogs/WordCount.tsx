import { useEffect, useState } from 'react'
import { useKitEditor } from '@kition/sdk/editor'
import { DialogShell } from './Shell'

/**
 * Word Count (Review › Word Count; plan "Doc usable by hand" stage 6A item 4; walkthrough G8): the
 * pages, words, characters with and without spaces, paragraphs and lines of the selection, or of
 * the document with none (`file.wordCount`, which counts a Chinese character as a word). It changes
 * nothing; OK closes it.
 */
type Counts = { pages?: number; words?: number; characters?: number; charactersWithSpaces?: number; paragraphs?: number; lines?: number }
const ROWS: Array<keyof Counts> = ['pages', 'words', 'characters', 'charactersWithSpaces', 'paragraphs', 'lines']

export function WordCount() {
  const { query, t } = useKitEditor()
  const [counts, setCounts] = useState<Counts | null>(null)
  useEffect(() => {
    let stale = false
    void query<Counts>('file.wordCount').then((answer) => {
      if (!stale) setCounts(answer ?? {})
    })
    return () => {
      stale = true
    }
  }, [query])
  return (
    <DialogShell title={t('wordCountTitle')} testId="doc-word-count" onOk={() => true}>
      <table className="doc-dialog-table">
        <tbody>
          {ROWS.map((row) => (
            <tr key={row}>
              <th scope="row">{t(`count.${row}`)}</th>
              <td data-testid={`doc-word-count-${row}`}>{counts?.[row] ?? '…'}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </DialogShell>
  )
}
