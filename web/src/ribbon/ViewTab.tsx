import { BookOpenIcon as BookOpen, FileTextIcon as FileText, GlobeIcon as Globe, Grid3x3Icon as Grid3x3, ListTreeIcon as ListTree, MaximizeIcon as Maximize, PanelLeftIcon as PanelLeft, RulerIcon as Ruler, ScanSearchIcon as ScanSearch, StretchHorizontalIcon as StretchHorizontal, FileIcon as File, NotebookTextIcon as NotebookText } from '@kition/tessera/icons/react'
import { CommandButton, RibbonButton, RibbonGroup, RibbonStack, useEngineQuery, useKitEditor } from '@kition/sdk/editor'
import type { ViewState } from '../doc'

export function ViewTab({ onZoom }: { onZoom: (to: 'pageWidth' | 'onePage' | number) => void }) {
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
      <RibbonGroup label={p('Show')} icon={Ruler}>
        <RibbonStack>
          <CommandButton command="view.ruler" primary icon={Ruler} pressed={view.ruler} label={p('Ruler')} />
          <CommandButton command="view.gridlines" icon={Grid3x3} pressed={view.gridlines} label={p('Gridlines')} />
          <CommandButton command="view.navigationPane" primary icon={PanelLeft} pressed={view.navPane} label={p('Navigation Pane')} />
        </RibbonStack>
      </RibbonGroup>
      <RibbonGroup label={p('Zoom')} icon={ScanSearch}>
        <RibbonButton icon={ScanSearch} size="large" label={t('zoom100')} onClick={() => onZoom(1)} data-testid="doc-zoom-100" primary />
        <RibbonStack>
          <RibbonButton icon={File} label={t('onePage')} onClick={() => onZoom('onePage')} data-testid="doc-zoom-page" primary />
          <RibbonButton icon={StretchHorizontal} label={t('pageWidth')} onClick={() => onZoom('pageWidth')} data-testid="doc-zoom-width" primary />
        </RibbonStack>
      </RibbonGroup>
    </>
  )
}
