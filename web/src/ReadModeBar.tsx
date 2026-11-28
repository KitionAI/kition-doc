import { Button, FrameChrome, useEngineQuery, useKitEditor } from '@kition/sdk/editor'
import type { ViewState } from './doc'

/**
 * Read Mode says so (stage 6A item 6; walkthrough V1): while it is on, a bar under the ribbon tells
 * the person the document is open for reading (typing does nothing) and has the way out, as
 * Print Layout is.
 */
export function ReadModeBar() {
  const { run, t } = useKitEditor()
  const view = useEngineQuery<ViewState>('view.state')
  if (!view?.readMode) return null
  return (
    <FrameChrome>
      <div className="kui-bar doc-read-mode" role="status" data-testid="doc-read-mode">
        <span>{t('readModeOn')}</span>
        <span style={{ flex: 1 }} />
        <Button variant="secondary" onClick={() => void run('view.printLayout')} data-testid="doc-read-mode-exit">
          {t('readModeExit')}
        </Button>
      </div>
    </FrameChrome>
  )
}
