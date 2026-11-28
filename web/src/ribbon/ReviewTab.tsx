import { FileDiffIcon as FileDiff } from '@kition/tessera/icons/react'
import { CommandButton, CommandTab, RibbonGroup, useEngineQuery, useKitEditor } from '@kition/sdk/editor'
import { DOC_PRIMARY } from '../doc'

/**
 * The commands Review's buttons on them grey out without: comments, and tracked changes (App's
 * `greyed`; stage 6A item 7, the button census). The engine runs them all the same (an agent's
 * call on a document without any changes nothing).
 */
export const REVIEW_OWN = {
  comments: ['comment.next', 'comment.previous', 'comment.deleteCurrent'],
  changes: ['change.accept', 'change.reject', 'change.acceptAll', 'change.rejectAll', 'change.next', 'change.previous', 'change.markup', 'change.showMarkup'],
}

/**
 * Review's buttons that only answer an agent a list (proofing issues, suggestions, the reviewing
 * pane's list, the accessibility check) or need a word chosen first (Ignore All, Add to
 * Dictionary: the right-click menu on the word has them): not on the ribbon; the commands stay for agents and the command search.
 */
const HIDDEN = ['change.track', 'change.list', 'spelling.issues', 'spelling.suggestions', 'spelling.ignoreAll', 'spelling.addToDictionary', 'spelling.thesaurus', 'file.accessibility']

/**
 * Review: the engine's commands, with Track Changes pressed while changes are being tracked
 * (walkthrough K5; WPS shows its 修订 button pressed).
 */
export function ReviewTab() {
  const { phrase: p } = useKitEditor()
  const info = useEngineQuery<{ trackChanges?: boolean }>('file.info') ?? {}
  return (
    <>
      <RibbonGroup label={p('Tracking')} icon={FileDiff}>
        <CommandButton command="change.track" primary icon={FileDiff} size="large" pressed={info.trackChanges === true} label={p('Track Changes')} />
      </RibbonGroup>
      <CommandTab menu="Review" exclude={HIDDEN} primary={DOC_PRIMARY.Review} />
    </>
  )
}
