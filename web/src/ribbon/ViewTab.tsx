import { BookOpenIcon as BookOpen, FileTextIcon as FileText, GlobeIcon as Globe, ListTreeIcon as ListTree, MaximizeIcon as Maximize, ScanSearchIcon as ScanSearch, StretchHorizontalIcon as StretchHorizontal, FileIcon as File, NotebookTextIcon as NotebookText } from '@kition/tessera/icons/react'
import { CommandButton, RibbonButton, RibbonGroup, RibbonStack, useEngineQuery, useKitEditor } from '@kition/sdk/editor'
import type { ViewState } from '../doc'
import type { ZoomActions } from '../ZoomControl'

/**
 * View › Zoom runs the status bar's zoom (ZoomControl.tsx): 100%, the current page whole, its width.
 * Show's Ruler, Gridlines and Navigation Pane are not here: the editor draws no ruler, gridlines
 * or navigation pane (stage 6B, cut for the launch; stage 6A item 7: no button that shows
 * nothing); the commands stay for agents and the command search.
 */
export function ViewTab({ zoom }: { zoom: ZoomActions }) {
  const { phrase: p, t } = useKitEditor()
  const view = useEngineQuery<ViewState>('view.state') ?? {}
  return (
    <>
      <RibbonGroup label={p('Views')} icon={BookOpen}>
        <CommandButton command="view.readMode" primary icon={BookOpen} size="large" pressed={view.readMode} label={p('Read Mode')} />
        <CommandButton command="view.printLayout" primary icon={FileText} size="large" pressed={view.mode === 'print'} label={p('Print Layout')} />
        <CommandButton command="view.webLayout" primary icon={Globe} size="large" pressed={view.mode === 'web'} label={p('Web Layout')} />
        <RibbonStack>
          <CommandButton command="view.outline" icon={ListTree} pressed={view.mode === 'outline'} label={p('Outline')} />
          <CommandButton command="view.draft" icon={NotebookText} pressed={view.mode === 'draft'} label={p('Draft')} />
        </RibbonStack>
      </RibbonGroup>
      <RibbonGroup label={p('Immersive')} icon={Maximize}>
        <CommandButton command="view.focus" primary icon={Maximize} size="large" pressed={view.focusMode} label={p('Focus')} />
      </RibbonGroup>
      <RibbonGroup label={p('Zoom')} icon={ScanSearch}>
        <RibbonButton icon={ScanSearch} size="large" label={t('zoom100')} onClick={() => zoom.zoomTo(1)} data-testid="doc-zoom-100" primary />
        <RibbonStack>
          <RibbonButton icon={File} label={t('onePage')} onClick={zoom.onePage} data-testid="doc-zoom-page" primary />
          <RibbonButton icon={StretchHorizontal} label={t('pageWidth')} onClick={zoom.pageWidth} data-testid="doc-zoom-width" primary />
        </RibbonStack>
      </RibbonGroup>
    </>
  )
}
