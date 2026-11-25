import { useCallback, useEffect, useMemo, useRef, useState, type KeyboardEvent, type ReactNode } from 'react'
import {
  Button,
  CommandTab,
  ChartDesignGroups,
  DrawingFormatGroup,
  KitEditorProvider,
  EditorFrame,
  FrameChrome,
  createKitStrings,
  useEngineTools,
  useKitEditorState,
  useKitSession,
  usePresence,
  type FrameTab,
  type HostConnection,
  type KitCanvasHandle,
  type KitCanvasView,
  type Phrases,
  type RasterRect,
  type RasterView,
} from '@kition/sdk/editor'
import { engineManifest } from './engine'
import { useEngineBackdrop } from './backdrop'
import { CATALOGS } from './strings'
import { CONTEXT_TABS, EXPORT_COMMANDS, HOST_FILE_COMMANDS, DOC_PRIMARY, describe, pageWidthArea } from './doc'
import { FindBar } from './FindBar'
import { CommentsPane, commenting, markerDecoration, useCommentMarkers, type CommentThread } from './Comments'
import { HostCommentLayer, hostHighlightDecoration, useHostComments } from './HostComments'
import { bothDecorations, peerMarks, placesOf, presenceDecoration, sharedPresence, type PlacedCaret, type SharedCaret } from './presence'
import { HomeTab } from './ribbon/HomeTab'
import { InsertTab } from './ribbon/InsertTab'
import { DrawTab } from './ribbon/DrawTab'
import { LayoutTab } from './ribbon/LayoutTab'
import { ViewTab } from './ribbon/ViewTab'
import { ExportGroup } from './ribbon/ExportGroup'
import { DocumentCanvas } from './DocumentCanvas'
import { pagePanBounds } from './pageView'

const NO_PAGES: RasterRect[] = []
/** Word's New Comment: in a host that keeps comment threads, the host's new thread instead. */
const NEW_COMMENT = 'comment.new'

/** kit-editor's `CommentsPanel` labels, translated as `comment.<label>`. */
const COMMENT_LABELS = ['title', 'empty', 'newComment', 'placeholder', 'replyPlaceholder', 'comment', 'reply', 'cancel', 'resolve', 'reopen', 'delete', 'resolved', 'readOnlyHistory', 'you'] as const

/** The threads, asked of the engine after every change (the editor's own query, outside its provider). */
function useCommentThreadsOf(editor: { query: <R>(id: string, params?: Record<string, unknown>) => Promise<R | null>; session: { doc: unknown; change: { revision: unknown } } }) {
  const [threads, setThreads] = useState<CommentThread[]>([])
  const { query, session } = editor
  useEffect(() => {
    if (!session.doc) return undefined
    let stale = false
    void query<CommentThread[]>('comment.threads').then((next) => {
      if (!stale) setThreads(next ?? [])
    })
    return () => {
      stale = true
    }
  }, [query, session.doc, session.change.revision])
  return threads
}

/**
 * The engine binds Escape to Collapse Selection; the text tool runs it when there is a selection,
 * and as a frame shortcut it would take every Escape, so Escape could never leave Focus.
 */
const hiddenShortcut = (command: string) => command === 'select.collapse'

const TABS = ['file', 'home', 'insert', 'draw', 'design', 'layout', 'references', 'mailings', 'review', 'view'] as const
const TAB_MENUS: Record<string, string> = { file: 'File', home: 'Home', insert: 'Insert', draw: 'Draw', design: 'Design', layout: 'Layout', references: 'References', mailings: 'Mailings', review: 'Review', view: 'View' }

export function App({ host, phrase }: { host: HostConnection; phrase: Phrases }) {
  const t = useMemo(() => createKitStrings(CATALOGS, host.init.locale), [host])
  // A commenter's session stays read-only: kit-editor runs only the manifest's comment commands, and
  // the engine writes comments and refuses every other edit itself (`setCommentOnly`).
  const commenter = commenting(host)
  const manifest = useMemo(() => engineManifest(commenter), [commenter])
  const session = useKitSession(host, manifest, describe)
  const ownEditor = useKitEditorState(session, host, t, 'doc', phrase)
  const { doc, change, state } = session
  // Comment threads the host keeps (Kit UI protocol section 7k, `HostComments.tsx`): a new comment
  // is the host's thread on the selection, and Review › New Comment (and its shortcut) starts one.
  const revision = useMemo(() => ({ revision: change.revision, state }), [change.revision, state])
  const commentLabel = useCallback((page: number) => t('commentAnchorLabel', { page }), [t])
  const hosted = useHostComments({ host, query: ownEditor.query, doc, revision, pages: state?.pages ?? NO_PAGES, label: commentLabel })
  const { create: createHosted, canCreate: hostCreates, selection: hostSelection } = hosted
  const startHosted = useCallback(() => void createHosted({ selectFirst: t('commentSelectFirst'), failed: t('commentFailed') }), [createHosted, t])
  const editor = useMemo(() => {
    if (!hosted.on) return ownEditor
    const run: typeof ownEditor.run = (id, params) => {
      if (id !== NEW_COMMENT) return ownEditor.run(id, params)
      if (hostCreates) startHosted()
      return Promise.resolve(null)
    }
    const enabled = (id: string) => (id === NEW_COMMENT ? hostCreates && hostSelection != null : ownEditor.enabled(id))
    return { ...ownEditor, run, enabled }
  }, [hostCreates, hostSelection, hosted.on, ownEditor, startHosted])
  // Escape cancels first (KitPointer 4.4): the text tool takes Escape only while the engine has
  // something for it to cancel (`escape`: a gesture, a shape chosen to draw, a selection). Any
  // other Escape is left untaken, so in Focus it leaves Focus.
  const tools = useEngineTools(session.tool, doc != null, change, { initial: 'text', hostTools: {}, escapable: () => state?.escape === true })
  const canvas = useRef<KitCanvasHandle>(null)
  useEngineBackdrop(doc, canvas)
  const viewMemory = useRef<RasterView | null>(null)
  const [zoom, setZoom] = useState(1)
  const [canvasHeight, setCanvasHeight] = useState(0)
  const lastCanvasView = useRef<KitCanvasView | null>(null)
  const [tab, setTab] = useState('home')
  const [find, setFind] = useState<{ replacing: boolean } | null>(null)
  const [commentsOpen, setCommentsOpen] = useState(false)
  const [activeThread, setActiveThread] = useState<string | null>(null)

  const threads = useCommentThreadsOf(editor)
  const commentLabels = useMemo(() => Object.fromEntries(COMMENT_LABELS.map((key) => [key, t(`comment.${key}`)])), [t])
  const markers = useCommentMarkers(threads, state?.pages ?? NO_PAGES, activeThread)
  // Presence (Kit UI protocol section 7e): this person's caret and selection as replica text
  // positions, and the other people's placed on these pages (`presence.ts`).
  const { query } = editor
  const [shared, setShared] = useState<SharedCaret | null>(null)
  useEffect(() => {
    if (!doc) return undefined
    let stale = false
    void query<SharedCaret>('presence.caret').then((answer) => {
      if (!stale) setShared(answer)
    })
    return () => {
      stale = true
    }
  }, [change.revision, doc, query, state])
  const local = useMemo(() => sharedPresence(shared), [shared])
  const { peers } = usePresence(host, local)
  const [placed, setPlaced] = useState<{ peers: typeof peers; places: PlacedCaret[] }>({ peers: [], places: [] })
  useEffect(() => {
    if (!doc || !peers.length) return undefined
    let stale = false
    void query<PlacedCaret[]>('presence.locate', { places: placesOf(peers) }).then((places) => {
      if (!stale) setPlaced({ peers, places: Array.isArray(places) ? places : [] })
    })
    return () => {
      stale = true
    }
  }, [change.revision, doc, peers, query])
  const pages = state?.pages ?? NO_PAGES
  const panBounds = useMemo(() => pagePanBounds(pages, canvasHeight, zoom), [pages, canvasHeight, zoom])
  const marks = useMemo(() => (placed.peers === peers ? peerMarks(peers, placed.places, pages) : []), [pages, peers, placed])
  // A kit's decoration replaces the engine's overlays (the caret and the selection): drawn first.
  const overlays = tools?.decorate ?? null
  const hostMarks = hosted.marks
  const decorate = useMemo(
    () => bothDecorations(bothDecorations(bothDecorations(overlays, hostHighlightDecoration(hostMarks)), markerDecoration(markers)), presenceDecoration(marks)),
    [overlays, hostMarks, markers, marks],
  )

  // A page made current some other way than on the canvas (Page Down, Go To) comes into view; a
  // page the caret was pushed to by an agent's or the host's edit (text inserted before it) does
  // not move the person's view.
  const page = state?.pages[state.page]
  const shown = useRef({ page: -1, revision: -1 })
  const editedBy = session.editedBy
  useEffect(() => {
    if (!state || !page) return
    const revision = change.revision
    if (shown.current.page === state.page) {
      shown.current.revision = revision
      return
    }
    const first = shown.current.page < 0
    const pushed = editedBy != null && revision !== shown.current.revision
    shown.current = { page: state.page, revision }
    if (first || pushed) return
    const view = viewMemory.current
    const box = canvas.current
    if (!view || !box) return
    const height = window.innerHeight / view.zoom
    if (page.y1 < view.y || page.y0 > view.y + height) canvas.current?.show(pageWidthArea(page)!)
  }, [change.revision, editedBy, page, state])

  const zoomTo = (to: 'pageWidth' | 'onePage' | number) => {
    const current = state?.pages[state.page]
    if (typeof to === 'number') canvas.current?.zoomTo(to)
    else if (current) canvas.current?.show(to === 'onePage' ? { x0: current.x0 - 24, y0: current.y0 - 24, x1: current.x1 + 24, y1: current.y1 + 24 } : pageWidthArea(current)!)
  }

  const keyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    const command = event.metaKey || event.ctrlKey
    if (command && (event.key === 'f' || event.key === 'h')) {
      setFind({ replacing: event.key === 'h' })
      return true
    }
    return false
  }

  const contextTabs = state ? CONTEXT_TABS.filter((context) => context.when(editor.enabled, state)) : []
  const ribbons: Record<(typeof TABS)[number], ReactNode> = {
    file: (
      <>
        <CommandTab menu="File" exclude={[...HOST_FILE_COMMANDS, ...EXPORT_COMMANDS]} primary={DOC_PRIMARY.File} />
        <ExportGroup page={(state?.page ?? 0) + 1} pages={state?.pages.length ?? 1} />
      </>
    ),
    home: <HomeTab onFind={(replacing) => setFind({ replacing })} />,
    insert: <InsertTab />,
    draw: <DrawTab />,
    design: <CommandTab menu="Design" primary={DOC_PRIMARY.Design} />,
    layout: <LayoutTab />,
    references: <CommandTab menu="References" primary={DOC_PRIMARY.References} />,
    mailings: <CommandTab menu="Mailings" primary={DOC_PRIMARY.Mailings} />,
    review: <CommandTab menu="Review" primary={DOC_PRIMARY.Review} />,
    view: <ViewTab onZoom={zoomTo} />,
  }
  const tabs: FrameTab[] = [
    ...TABS.map((id) => ({ id, label: phrase(TAB_MENUS[id]), view: id === 'review' || id === 'view', ribbon: ribbons[id] })),
    ...contextTabs.map((context) => ({
      id: context.id,
      label: context.label ? t(context.label) : phrase(context.menu),
      ribbon: (
        <>
          {context.id === 'shapeFormat' && state?.shape ? <DrawingFormatGroup ids={state.shapes?.length ? state.shapes : [state.shape]} testId="doc-drawing" /> : null}
          {context.id === 'chartDesign' && state?.chart ? <ChartDesignGroups id={state.chart} testId="doc-chart" /> : null}
          <CommandTab menu={context.menu} extra={context.extra} primary={context.primary} />
        </>
      ),
    })),
  ]
  const activeTab = tabs.some((item) => item.id === tab) ? tab : 'home'

  return (
    <KitEditorProvider editor={editor} tools={tools}>
      <EditorFrame
        tabs={tabs}
        tab={activeTab}
        onTab={setTab}
        canvas={canvas}
        zoom={zoom}
        onKeyDown={keyDown}
        hiddenCommands={hiddenShortcut}
        statusEnd={
          <Button variant="ghost" aria-pressed={commentsOpen} data-testid="doc-comments-toggle" onClick={() => setCommentsOpen((open) => !open)}>
            {threads.length ? t('commentsCount', { count: threads.length }) : t('comments')}
          </Button>
        }
        status={
          state ? (
            <>
              <span data-testid="doc-status">{t('pageOf', { page: state.page + 1, pages: state.pages.length })}</span>
              <span data-testid="doc-words">{t('words', { count: state.words })}</span>
              {state.selected ? <span>{t('characters', { count: state.selected })}</span> : null}
            </>
          ) : null
        }
        panels={
          commentsOpen ? (
            <CommentsPane
              threads={threads}
              active={activeThread}
              onActive={setActiveThread}
              onClose={() => setCommentsOpen(false)}
              onNew={hosted.on ? (hostCreates ? startHosted : null) : undefined}
              strings={{ title: t('comments'), newComment: t('newComment'), close: t('close'), selectFirst: t('commentOnSelection'), labels: commentLabels }}
            />
          ) : null
        }
      >
        {find ? (
          <FrameChrome>
            <FindBar replacing={find.replacing} onClose={() => setFind(null)} />
          </FrameChrome>
        ) : null}
        <DocumentCanvas
          page={state?.pages[0]}
          canvas={canvas}
          viewMemory={viewMemory}
          panBounds={panBounds}
          framed={false}
          onView={(view) => {
            const previous = lastCanvasView.current
            lastCanvasView.current = view
            setZoom(view.zoom)
            setCanvasHeight(view.height)
            if (previous && previous.height !== view.height && previous.zoom === view.zoom) canvas.current?.panTo(view.x, previous.y)
          }}
          decorate={decorate}
          overlay={
            hosted.on
              ? (view) => (
                  <HostCommentLayer
                    view={view}
                    marks={hostMarks}
                    pages={state?.pages ?? NO_PAGES}
                    selection={hostSelection}
                    canCreate={hostCreates}
                    onCreate={startHosted}
                    onOpen={hosted.open}
                    strings={{ comment: t('commentAction'), open: (count) => t('commentThread', { count }) }}
                  />
                )
              : undefined
          }
        />
      </EditorFrame>
    </KitEditorProvider>
  )
}
