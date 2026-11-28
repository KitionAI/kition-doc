import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState, type KeyboardEvent, type ReactNode } from 'react'
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
  toast,
} from '@kition/sdk/editor'
import { engineManifest } from './engine'
import { useEngineBackdrop } from './backdrop'
import { CATALOGS } from './strings'
import { CONTEXT_TABS, EXPORT_COMMANDS, HOST_FILE_COMMANDS, DOC_PRIMARY, describe } from './doc'
import { FindBar, type FindRequest } from './FindBar'
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
import { focusText, takeKeyboardOnOpen, textInput, useKeyboardFallback } from './focus'
import { pressKey, type KeyContext } from './keys'
import { clipOf, copySelection, pasteClip, pasteFromClipboard, type ClipboardEditor } from './clipboard'
import { pageWidthArea, wholePage, zoomKeepingMiddle } from './zoom'
import { useFollowCaret } from './followCaret'
import { useInputMethod } from './inputMethod'
import { edgeScrolling } from './edgeScroll'
import { ZoomControl, type ZoomActions } from './ZoomControl'
import { DocContextMenu, type MenuAnswer, type OpenMenu } from './ContextMenu'
import { DocDialogs } from './dialogs/DocDialogs'
import { openDialog } from './dialogs/store'
import { ASKING, NOTICES, OWN_WINDOWS, PARAMS_FIRST, REQUESTS, unfilled, type AskContext } from './dialogs/asks'
import { ReviewTab, REVIEW_OWN } from './ribbon/ReviewTab'
import { ReadModeBar } from './ReadModeBar'
import { TableDesignGroups, TableLayoutGroups, TABLE_DESIGN_OWN, TABLE_LAYOUT_OWN } from './ribbon/TableGroups'

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
 * Cmd+H is the system's Hide on a Mac, so the engine's Replace key runs nothing; Replace is
 * Ctrl+H (keys.ts). The engine binds Escape to Collapse Selection; the text tool runs it when there is a selection,
 * and as a frame shortcut it would take every Escape, so Escape could never leave Focus.
 */
const hiddenShortcut = (command: string) => command === 'select.collapse' || command === 'edit.replace'

/**
 * Buttons the generated tabs leave out: what they run is an agent's query (a list) with nothing a
 * person would see, or needs a service Kition does not have (stage 6A item 7, the button census).
 * The commands stay, for agents and the command search.
 */
const HIDDEN_FROM_RIBBON = ['theme.list', 'research.open']

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
  // Select All keeps the view where it is (walkthrough C10; WPS W-D10): the caret it leaves at the
  // end of the document does not bring the last page into view, until the person's next key or press.
  const holdView = useRef(false)
  // A table just made from Insert: the ribbon goes to its Table Design tab once it shows (WPS
  // W-N2; stage 4 item 7). A caret only moved into a table leaves the tab where it is.
  const tableMade = useRef(false)
  // The editor's own runs (the ribbon, the keys table, dialogs) are the person's moves: the view
  // follows the caret after them (followCaret.ts).
  const personActed = useRef<() => void>(() => undefined)
  // What the dialogs a person asks for open (dialogs/asks.ts); filled in below, once the parts exist.
  const asks = useRef<AskContext | null>(null)
  // The engine's requests wait in the tool's next answer: asked for after a run that may make one.
  const refreshTools = useRef<() => void>(() => undefined)
  // What the commands' buttons are greyed by beyond the engine's own conditions (stage 6A item 7).
  const greyed = useRef<(id: string) => boolean>(() => false)
  // A run the editor does its own way first (Next Footnote with none), and what follows a run (a notice).
  const special = useRef<(id: string, params?: Record<string, unknown>) => Promise<string | null> | null>(() => null)
  const afterRun = useRef<(id: string, answer: Promise<string | null>) => void>(() => undefined)
  const editor = useMemo(() => {
    const ownWindow = (id: string, params?: Record<string, unknown>) => {
      const own = OWN_WINDOWS[id]
      if (!own || !asks.current || (id === NEW_COMMENT && hosted.on) || !unfilled(params, own.fills)) return false
      own.open(asks.current)
      return true
    }
    const ask: typeof ownEditor.ask = (id, values) => {
      if (!ownWindow(id, values)) ownEditor.ask(id, values)
    }
    const run: typeof ownEditor.run = (id, params, options) => {
      if (id === 'edit.selectAll') holdView.current = true
      if (id === 'table.insert' || id === 'table.quick' || id === 'table.fromText') tableMade.current = true
      personActed.current()
      if (ownWindow(id, params)) return Promise.resolve(null)
      if (PARAMS_FIRST.has(id) && unfilled(params, [])) {
        ownEditor.ask(id)
        return Promise.resolve(null)
      }
      const own = special.current(id, params)
      if (own) return own
      if (id !== NEW_COMMENT || !hosted.on) {
        const answer = ownEditor.run(id, params, options)
        if (ASKING.has(id)) void answer.then(() => refreshTools.current())
        afterRun.current(id, answer)
        return answer
      }
      if (hostCreates) startHosted()
      return Promise.resolve(null)
    }
    const enabled = (id: string) => (hosted.on && id === NEW_COMMENT ? hostCreates && hostSelection != null : ownEditor.enabled(id) && !greyed.current(id))
    return { ...ownEditor, run, ask, enabled }
  }, [hostCreates, hostSelection, hosted.on, ownEditor, startHosted])
  // Select All holds the view until the person's next key (but Cmd+A itself) or press.
  useEffect(() => {
    const release = (event: Event) => {
      if (event instanceof KeyboardEvent && (event.key === 'a' || event.key === 'A' || event.code === 'KeyA') && (event.metaKey || event.ctrlKey)) return
      holdView.current = false
    }
    document.addEventListener('keydown', release, true)
    document.addEventListener('pointerdown', release, true)
    return () => {
      document.removeEventListener('keydown', release, true)
      document.removeEventListener('pointerdown', release, true)
    }
  }, [])
  // Escape cancels first (KitPointer 4.4): the text tool takes Escape only while the engine has
  // something for it to cancel (`escape`: a gesture, a shape chosen to draw, a selection). Any
  // other Escape is left untaken, so in Focus it leaves Focus.
  const tools = useEngineTools(session.tool, doc != null, change, {
    initial: 'text',
    hostTools: {},
    escapable: () => state?.escape === true,
    // A dialog the engine asks for (its `ui_requests`, the binding's tool requests): dialogs/asks.ts.
    onDialog: (name, params) => {
      const open = REQUESTS[name]
      if (open && asks.current) open(asks.current, params && typeof params === 'object' ? (params as Record<string, unknown>) : {})
    },
  })
  refreshTools.current = tools.refresh
  const canvas = useRef<KitCanvasHandle>(null)
  useEngineBackdrop(doc, canvas)
  // A drag past the window's edge scrolls the view (edgeScroll.ts), whatever the text tool's
  // gesture is; it reads the tool's cursor (the crosshair while drawing) at each frame.
  const toolCursor = useRef(tools.canvasTool.cursor)
  toolCursor.current = tools.canvasTool.cursor
  const pageTool = useMemo(() => edgeScrolling(tools.canvasTool, () => {
    const view = canvas.current?.view()
    return { cursor: toolCursor.current, size: view ? { width: view.width, height: view.height } : null }
  }), [tools.canvasTool])
  // The keyboard is the text's from the moment the document opens (walkthrough K1), and comes back
  // to it when what had it goes away (the find bar, a dialog; K2): focus.ts.
  useKeyboardFallback()
  useEffect(() => {
    if (!doc) return undefined
    let frame = 0
    let tries = 0
    const take = () => {
      if (textInput()) takeKeyboardOnOpen()
      else if (tries++ < 120) frame = window.requestAnimationFrame(take)
    }
    take()
    return () => window.cancelAnimationFrame(frame)
  }, [doc])
  const viewMemory = useRef<RasterView | null>(null)
  const [zoom, setZoom] = useState(1)
  const [canvasHeight, setCanvasHeight] = useState(0)
  const lastCanvasView = useRef<KitCanvasView | null>(null)
  const [tab, setTab] = useState('home')
  // The find bar: open (on the replace field or not), the step the keys asked for, and a counter
  // that puts the keyboard back in its field; the text last searched for is shown when it opens again.
  const [find, setFind] = useState<{ replacing: boolean; request?: FindRequest; focus: number } | null>(null)
  const findText = useRef('')
  const findAsks = useRef(0)
  const [commentsOpen, setCommentsOpen] = useState(false)
  // New Comment (Cmd+Alt+A, Review, Insert, the right-click menu): the pane with the new comment's
  // box, which takes the keyboard; the text stays selected (stage 6A item 5; walkthrough K3).
  const [newCommentAsked, setNewCommentAsked] = useState(0)
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

  // The view follows the caret after the person's own key, press or command, at the same zoom,
  // only as far as shows its line (followCaret.ts; walkthrough A9, A10, D5, A11, C7, Q2). A caret
  // pushed on by an agent's or the host's edit does not move the person's view, nor does Select All.
  const page = state?.pages[state.page]
  // The whole document selected (Select All, three clicks in the selection bar) holds the view too.
  const allSelected = useRef(false)
  allSelected.current = state?.all === true
  const holding = useCallback(() => holdView.current || allSelected.current, [])
  personActed.current = useFollowCaret({ canvas, caret: state?.caret, editedBy: session.editedBy, held: holding })

  // An input method composes in the text at the caret, its candidate window under the line
  // (inputMethod.ts; stage 3, walkthrough D16).
  const placeInput = useInputMethod({
    composer: {
      invoke: doc ? (method, ...args) => doc.invoke(method, ...args) : null,
      inOrder: session.inOrder,
      settle: () => session.tool('tool', [], true),
      refresh: () => tools.refresh(),
    },
    canvas,
    at: state?.inputAt,
  })

  // The zoom (zoom.ts, ZoomControl.tsx): the status bar, View › Zoom, and the frame's Fit.
  const zoomActions: ZoomActions = {
    zoom,
    zoomTo: (to) => {
      if (canvas.current) zoomKeepingMiddle(canvas.current, to)
    },
    pageWidth: () => {
      if (page) canvas.current?.show(pageWidthArea(page))
    },
    onePage: () => {
      if (page) canvas.current?.show(wholePage(page))
    },
  }

  // The system clipboard (clipboard.ts): Copy and Cut from the keys and the ribbon; Cmd+V is the
  // browser's paste event, which carries the clipboard without asking for permission.
  const clipEditor: ClipboardEditor = { run: (command, params) => editor.run(command, params), enabled: editor.enabled, ask: editor.ask, t }
  const clipboard = (action: 'copy' | 'cut' | 'paste' | 'pasteText' | 'pasteSpecial') => {
    if (action === 'copy' || action === 'cut') void copySelection(clipEditor, action === 'cut', (state?.selected ?? 0) > 0 || state?.shape != null || editor.enabled('edit.cut'))
    else void pasteFromClipboard(clipEditor, action === 'paste' ? 'keep' : action === 'pasteText' ? 'text' : 'special')
  }
  const latestClip = useRef(clipEditor)
  latestClip.current = clipEditor
  useEffect(() => {
    const inText = (target: EventTarget | null) => target instanceof HTMLTextAreaElement && target.classList.contains('kui-text-sink')
    // Cmd+V in the text goes on to the browser, which pastes: the editor's shortcuts do not see it
    // (they would prevent the paste event).
    // A browser that pastes nothing on the key (no paste event follows it) still pastes, from the
    // clipboard API or the engine's own copy.
    let unpasted = 0
    const onKey = (event: globalThis.KeyboardEvent) => {
      const command = event.metaKey || event.ctrlKey
      if (!command || (event.metaKey && event.ctrlKey) || event.shiftKey || event.altKey || !(event.code === 'KeyV' || event.key.toLowerCase() === 'v') || !inText(event.target) || event.isComposing) return
      event.stopPropagation()
      window.clearTimeout(unpasted)
      unpasted = window.setTimeout(() => void pasteFromClipboard(latestClip.current, 'keep'), 100)
    }
    const onPaste = (event: ClipboardEvent) => {
      if (!inText(event.target) || !event.clipboardData) return
      window.clearTimeout(unpasted)
      event.preventDefault()
      void pasteClip(latestClip.current, clipOf(event.clipboardData), 'keep')
    }
    window.addEventListener('keydown', onKey, true)
    document.addEventListener('paste', onPaste, true)
    return () => {
      window.clearTimeout(unpasted)
      window.removeEventListener('keydown', onKey, true)
      document.removeEventListener('paste', onPaste, true)
    }
  }, [])

  // The right-click menu (ContextMenu.tsx; stage 2A, walkthrough R1): the engine settles the
  // selection for the press (in the editor's input order, after the presses before it) and says
  // what the menu is for; it opens at the pointer, or from the keyboard at the caret's foot.
  const [menu, setMenu] = useState<OpenMenu | null>(null)
  const closeMenu = useCallback(() => setMenu(null), [])
  const latestMenu = useRef({ session, tools })
  latestMenu.current = { session, tools }
  const openMenu = (client: { x: number; y: number } | null) => {
    const { session: now } = latestMenu.current
    const engine = now.doc
    const handle = canvas.current
    const shown = handle?.view()
    const surface = document.querySelector<HTMLCanvasElement>('[data-testid=kit-canvas-surface]')
    if (!engine || !shown || !surface || !(shown.zoom > 0)) return
    const box = surface.getBoundingClientRect()
    const point = client ? { x: shown.x + (client.x - box.left) / shown.zoom, y: shown.y + (client.y - box.top) / shown.zoom } : null
    void now
      .inOrder(async () => {
        const answer = await engine.invoke<string>('contextMenu', point?.x ?? Number.NaN, point?.y ?? Number.NaN, shown.zoom)
        // A settled tool call brings the editor's state up to date (the selection may have moved).
        await now.tool('tool', [], true)
        return answer
      })
      .then((raw) => {
        latestMenu.current.tools.refresh()
        const answer = JSON.parse(raw) as MenuAnswer
        const view = canvas.current?.view() ?? shown
        const at = client ?? (answer.at ? { x: box.left + (answer.at[0] - view.x) * view.zoom, y: box.top + (answer.at[1] - view.y) * view.zoom } : { x: box.left + 40, y: box.top + 40 })
        setMenu({ ...answer, client: at })
      })
      .catch(() => undefined)
  }
  const startComment = () => {
    setCommentsOpen(true)
    setNewCommentAsked((count) => count + 1)
  }
  const newComment = () => (hosted.on ? void editor.run(NEW_COMMENT) : startComment())

  // Insert › Pictures' file picker (the host's), for the engine's request of it.
  const pickPicture = useRef<() => void>(() => undefined)
  asks.current = {
    dialog: openDialog,
    find: (replacing) => openFind(replacing),
    params: (command) => ownEditor.ask(command),
    showCaret: () => personActed.current(),
    pasteSpecial: () => clipboard('pasteSpecial'),
    picture: () => pickPicture.current(),
    // The status bar's zoom choices.
    zoom: () => document.querySelector<HTMLButtonElement>('[data-testid=kit-zoom]')?.click(),
    tell: (key) => void toast('info', t(key)),
    close: () => host.close(),
    comment: startComment,
    commentsPane: () => setCommentsOpen((open) => !open),
  }
  // Review's buttons on comments and tracked changes are grey when the document has none (REVIEW_OWN).
  const [revisions, setRevisions] = useState<number | null>(null)
  useEffect(() => {
    if (tab !== 'review' || !doc) {
      setRevisions(null)
      return undefined
    }
    let stale = false
    void ownEditor.query<unknown[]>('change.list').then((list) => {
      if (!stale) setRevisions(Array.isArray(list) ? list.length : null)
    })
    return () => {
      stale = true
    }
  }, [tab, doc, change.revision, ownEditor.query])
  greyed.current = (id) => (REVIEW_OWN.comments.includes(id) && threads.length === 0) || (REVIEW_OWN.changes.includes(id) && revisions === 0)
  special.current = (id) => {
    // Next Footnote, and Show Notes (which only lists them to an agent), go to the next note's
    // reference; with none in the text the caret stays, and they say so.
    if (id !== 'footnote.next' && id !== 'footnote.show') return null
    const where = () => ownEditor.query<{ anchor?: unknown; focus?: unknown }>('file.selection').then((selection) => JSON.stringify([selection?.anchor, selection?.focus]))
    return where().then(async (before) => {
      const answer = await ownEditor.run('footnote.next')
      if ((await where()) === before) toast('info', t('noNotes'))
      return answer
    })
  }
  afterRun.current = (id, answer) => {
    const notice = NOTICES[id]
    if (notice) void answer.then((done) => done != null && toast('success', t(notice)))
  }

  const openFind = (replacing: boolean) => setFind((open) => ({ replacing: replacing || (open?.replacing ?? false), request: open?.request, focus: ++findAsks.current }))
  const closeFind = () => {
    setFind(null)
    window.requestAnimationFrame(() => focusText())
  }
  // The editor's own keys are a table (keys.ts), each through the editor's ordered runs.
  const keyContext: KeyContext = {
    run: (command, params = {}) => void editor.run(command, params),
    ask: (command) => editor.ask(command),
    openFind,
    findStep: (direction) => setFind((open) => ({ replacing: open?.replacing ?? false, request: { direction, id: ++findAsks.current }, focus: open?.focus ?? 0 })),
    clipboard,
    // Page Up and Page Down go a screen (the canvas's height), the view with them, so the caret
    // keeps its place on the screen (WPS W-D7); with Cmd, to the page before or after (W-D8).
    page: (direction, extend, screen) => {
      const command = direction > 0 ? 'caret.pageDown' : 'caret.pageUp'
      const handle = canvas.current
      const view = handle?.view()
      if (!screen || !handle || !view || !(view.zoom > 0) || view.height <= 0) {
        void editor.run(command, { extend })
        return
      }
      const distance = view.height / view.zoom
      handle.panTo(view.x, view.y + direction * distance)
      void editor.run(command, { extend, distance })
    },
    contextMenu: () => openMenu(null),
    selectAll: () => void editor.run(state?.selectsTable ? 'table.selectTable' : 'edit.selectAll'),
  }
  const keyDown = (event: KeyboardEvent<HTMLDivElement>) => pressKey(event, keyContext)

  // The commands the editor has a window for never show their parameters dialog, whatever asked
  // for it (a ribbon tab's command list, the command search): the window opens instead.
  const askedFor = ownEditor.dialog?.command.id
  useLayoutEffect(() => {
    if (!askedFor || !asks.current) return
    const own = OWN_WINDOWS[askedFor]
    if (!own || (askedFor === NEW_COMMENT && hosted.on) || !unfilled(ownEditor.dialog?.values, own.fills)) return
    ownEditor.setDialog(null)
    own.open(asks.current)
    // eslint-disable-next-line react-hooks/exhaustive-deps -- per dialog asked for
  }, [ownEditor.dialog])

  const contextTabs = state ? CONTEXT_TABS.filter((context) => context.when(editor.enabled, state)) : []
  const ribbons: Record<(typeof TABS)[number], ReactNode> = {
    file: (
      <>
        <CommandTab menu="File" exclude={[...HOST_FILE_COMMANDS, ...EXPORT_COMMANDS]} primary={DOC_PRIMARY.File} />
        <ExportGroup page={(state?.page ?? 0) + 1} pages={state?.pages.length ?? 1} />
      </>
    ),
    home: <HomeTab onFind={openFind} onClipboard={clipboard} painter={state?.painter ?? null} />,
    insert: <InsertTab />,
    draw: <DrawTab />,
    design: <CommandTab menu="Design" exclude={HIDDEN_FROM_RIBBON} primary={DOC_PRIMARY.Design} />,
    layout: <LayoutTab />,
    references: <CommandTab menu="References" exclude={HIDDEN_FROM_RIBBON} primary={DOC_PRIMARY.References} />,
    mailings: <CommandTab menu="Mailings" primary={DOC_PRIMARY.Mailings} />,
    review: <ReviewTab />,
    view: <ViewTab zoom={zoomActions} />,
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
          {context.id === 'tableDesign' ? <TableDesignGroups /> : null}
          {context.id === 'tableLayout' ? <TableLayoutGroups /> : null}
          <CommandTab menu={context.menu} extra={context.extra} primary={context.primary} exclude={context.id === 'tableDesign' ? TABLE_DESIGN_OWN : context.id === 'tableLayout' ? TABLE_LAYOUT_OWN : undefined} />
        </>
      ),
    })),
  ]
  const activeTab = tabs.some((item) => item.id === tab) ? tab : 'home'
  const tableTabShown = contextTabs.some((context) => context.id === 'tableDesign')
  useEffect(() => {
    if (tableMade.current && tableTabShown) {
      tableMade.current = false
      setTab('tableDesign')
    }
  }, [tableTabShown, state])
  // A contextual tab that went (the caret left the table, the picture was let go) gives the ribbon
  // back to Home, so coming back in later does not switch to it.
  useEffect(() => {
    if (state && tab !== activeTab) setTab(activeTab)
  }, [state, tab, activeTab])

  return (
    <KitEditorProvider editor={editor} tools={tools}>
      <EditorFrame
        tabs={tabs}
        tab={activeTab}
        onTab={setTab}
        canvas={canvas}
        onKeyDown={keyDown}
        onFit={zoomActions.onePage}
        keyboardHome={textInput}
        hiddenCommands={hiddenShortcut}
        statusEnd={
          <>
            <Button variant="ghost" aria-pressed={commentsOpen} data-testid="doc-comments-toggle" onClick={() => setCommentsOpen((open) => !open)}>
              {threads.length ? t('commentsCount', { count: threads.length }) : t('comments')}
            </Button>
            {state ? <ZoomControl {...zoomActions} /> : null}
          </>
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
              start={newCommentAsked}
              strings={{ title: t('comments'), newComment: t('newComment'), close: t('close'), selectFirst: t('commentOnSelection'), labels: commentLabels }}
            />
          ) : null
        }
      >
        <ReadModeBar />
        {find ? (
          <FrameChrome>
            <FindBar replacing={find.replacing} initial={findText.current} request={find.request} focus={find.focus} onText={(text) => (findText.current = text)} onClose={closeFind} />
          </FrameChrome>
        ) : null}
        <DocumentCanvas
          page={state?.pages[0]}
          canvas={canvas}
          viewMemory={viewMemory}
          panBounds={panBounds}
          framed={false}
          tool={pageTool}
          onMenu={openMenu}
          onView={(view) => {
            const previous = lastCanvasView.current
            lastCanvasView.current = view
            setZoom(view.zoom)
            setCanvasHeight(view.height)
            if (previous && previous.height !== view.height && previous.zoom === view.zoom) canvas.current?.panTo(view.x, previous.y)
            placeInput(view)
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
        <DocContextMenu menu={menu} onClose={closeMenu} onClipboard={clipboard} onNewComment={newComment} />
        <DocDialogs pick={pickPicture} />
      </EditorFrame>
    </KitEditorProvider>
  )
}
