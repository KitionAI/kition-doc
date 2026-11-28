import { openDialog, type DocDialogKind } from './store'

/**
 * What opens when the person asks for a dialog (plan "Doc usable by hand", stage 6A item 1;
 * walkthrough G8, G11, K3, L2, P8; gap R7). Two ways in, one table each:
 *
 * - The engine's requests (`ui_requests`, 21 places in `crates/engine/engine/src/cmd`), which the
 *   binding hands the editor as tool requests (`ToolRequest::Dialog`, `onDialog`): each name below
 *   opens something a person sees. A request no row names opens nothing, and `ENGINE_REQUESTS`
 *   lists every one the engine makes (a test keeps the two equal).
 * - The commands the editor has a window of its own for (`OWN_WINDOWS`): run without the values
 *   they need, or asked for (a ribbon button's parameters dialog, a key, the right-click menu),
 *   they open the window instead; its OK runs the command with the values filled in.
 *
 * Programmatic calls (an agent's) never open dialogs: the binding drops their requests, and an
 * agent's call names its values.
 */

/** What a request or a command can open in the editor. */
export type AskContext = {
  /** The editor's own dialog. */
  dialog: (kind: DocDialogKind) => void
  /** The find bar (`replacing`: on its replace field). */
  find: (replacing: boolean) => void
  /** The command's parameters (kit-editor's generic dialog). */
  params: (command: string) => void
  /** Brings the caret into view, as a person's move does. */
  showCaret: () => void
  /** Paste Special (the clipboard's formats). */
  pasteSpecial: () => void
  /** Insert › Pictures: the host's file picker. */
  picture: () => void
  /** The status bar's zoom choices (200%, 100%, 75%, Page Width, One Page, a percentage). */
  zoom: () => void
  /** A notice: what the host does instead (opening, saving under another name, printing). */
  tell: (key: string) => void
  /** Closes the document (the host's). */
  close: () => void
  /** The new comment's box in the comments pane. */
  comment: () => void
  /** Shows or hides the comments pane. */
  commentsPane: () => void
}

/** Every request the engine's commands make (`s.ui_requests.push` in `crates/engine/engine/src/cmd`), and the command that makes it. */
export const ENGINE_REQUESTS: Record<string, string[]> = {
  find: ['edit.find', 'edit.advancedFind'],
  scrollTo: ['edit.goto'],
  goto: ['edit.goto'],
  font: ['format.fontDialog'],
  pageSetup: ['page.pageSetup'],
  manageStyles: ['style.manage'],
  paragraph: ['paragraph.dialog'],
  saveAs: ['file.share', 'file.save', 'file.saveAs'],
  pasteSpecial: ['edit.pasteSpecial'],
  insertPicture: ['image.insert'],
  link: ['link.insert'],
  newWindow: ['view.newWindow'],
  split: ['view.split'],
  zoom: ['view.zoom'],
  print: ['file.print'],
  close: ['file.close'],
  options: ['file.options'],
  openFile: ['file.open'],
}

/** The engine's requests: what each opens. */
export const REQUESTS: Record<string, (context: AskContext, params: Record<string, unknown>) => void> = {
  find: (c) => c.find(false),
  scrollTo: (c) => c.showCaret(),
  // Go To has no window of its own: its parameters (a page, a bookmark, a paragraph).
  goto: (c) => c.params('edit.goto'),
  font: (c) => c.dialog('font'),
  // Page Setup and Manage Styles have no window of their own yet: their parameters.
  pageSetup: (c) => c.params('page.pageSetup'),
  manageStyles: (c) => c.params('style.manage'),
  paragraph: (c) => c.dialog('paragraph'),
  pasteSpecial: (c) => c.pasteSpecial(),
  insertPicture: (c) => c.picture(),
  link: (c) => c.dialog('link'),
  zoom: (c) => c.zoom(),
  close: (c) => c.close(),
  // The host's: the editor says where they are.
  saveAs: (c) => c.tell('askSaveAs'),
  openFile: (c) => c.tell('askOpen'),
  print: (c) => c.tell('askPrint'),
  options: (c) => c.tell('askOptions'),
  newWindow: (c) => c.tell('askNewWindow'),
  split: (c) => c.tell('askSplit'),
}

/** The commands whose run may ask for a dialog: after the person runs one, the editor reads the engine's requests. */
export const ASKING = new Set(Object.values(ENGINE_REQUESTS).flat())

/**
 * The commands with a window of their own, and when it opens instead of running: `true` when the
 * run names none of the values the window fills (`fills`).
 */
export const OWN_WINDOWS: Record<string, { open: (context: AskContext) => void; fills: string[] }> = {
  'format.fontDialog': { open: (c) => c.dialog('font'), fills: [] },
  'paragraph.dialog': { open: (c) => c.dialog('paragraph'), fills: [] },
  'paragraph.indents': { open: (c) => c.dialog('paragraph'), fills: ['left', 'right', 'firstLine'] },
  'paragraph.spacing': { open: (c) => c.dialog('paragraph'), fills: ['before', 'after'] },
  'link.insert': { open: (c) => c.dialog('link'), fills: ['url'] },
  'comment.new': { open: (c) => c.comment(), fills: ['text'] },
  'file.wordCount': { open: (c) => c.dialog('wordCount'), fills: ['includeTextBoxes'] },
  'table.properties': { open: (c) => c.dialog('tableProperties'), fills: ['align', 'width', 'widthPct', 'indent', 'rowHeight', 'rowHeightRule', 'allowBreak', 'headerRow', 'columnWidth', 'cellWidth', 'valign'] },
  'table.cellMargins': { open: (c) => c.dialog('tableProperties'), fills: ['top', 'left', 'bottom', 'right'] },
  'image.size': { open: (c) => c.dialog('pictureSize'), fills: ['width', 'height', 'scale'] },
  'edit.find': { open: (c) => c.find(false), fills: ['text'] },
  'edit.advancedFind': { open: (c) => c.find(false), fills: ['text', 'highlight'] },
  'edit.replace': { open: (c) => c.find(true), fills: ['text', 'with'] },
  'view.zoom': { open: (c) => c.zoom(), fills: ['percent', 'fit'] },
  // Review's Comments Pane and Show Comments: the editor's comments pane.
  'view.commentsPane': { open: (c) => c.commentsPane(), fills: ['value'] },
  'comment.pane': { open: (c) => c.commentsPane(), fills: [] },
}

/**
 * The commands whose work shows nothing on the page when the document is already as they would
 * leave it (nothing to update): a notice says it is done, in words true either way.
 */
export const NOTICES: Record<string, string> = {
  'toc.update': 'done.tocUpdate',
  'toc.remove': 'done.tocRemove',
  'tableOfFigures.update': 'done.generatedUpdate',
  'index.update': 'done.generatedUpdate',
  'field.updateAll': 'done.fieldsUpdate',
  'theme.setDefault': 'done.themeDefault',
}

/**
 * The commands that, run with no values, change nothing a person sees, so the ribbon's button
 * asks for them first (the command's parameters dialog), as its menu would.
 */
export const PARAMS_FIRST = new Set(['image.crop', 'image.corrections', 'image.effects', 'change.options', 'table.look', 'table.sort', 'table.setStyle', 'page.color', 'theme.effects', 'footnote.options', 'citation.sources', 'citation.style', 'spelling.language', 'file.restrictEditing', 'crossReference.insert', 'toc.addText'])

/** Whether `params` names none of `fills` (an empty run: the window opens). */
export function unfilled(params: Record<string, unknown> | undefined, fills: string[]) {
  if (!params) return true
  const given = Object.keys(params).filter((key) => params[key] !== undefined)
  return fills.length ? !given.some((key) => fills.includes(key)) : given.length === 0
}

export { openDialog }
